//! Meeting notes: on or off with its shortcut, and a recording kept on disk as a meeting: `mic.wav` (Me) and
//! `them.wav` (the apps using the mic). The audio reaches it only through `Mic` and `Them`, so tests drive it with fakes.

use crate::runtime::RATE;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rusqlite::{Connection, OptionalExtension};
use std::ffi::{CStr, c_char, c_void};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// The shortcuts to pick from; "" is no shortcut.
pub const KEYS: [&str; 3] = ["Ctrl+Alt+KeyM", "Ctrl+Alt+KeyN", ""];

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Notes {
    pub on: bool,
    /// An accelerator, the form both the global shortcut and the menu read; "" for none.
    pub key: String,
}

fn read(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read meeting notes' settings: {e}"))
}

pub fn get(conn: &Connection) -> Result<Notes, String> {
    Ok(Notes {
        on: read(conn, "meeting_on")?.is_some_and(|v| v == "1"),
        key: read(conn, "meeting_key")?.filter(|k| KEYS.contains(&k.as_str())).unwrap_or(KEYS[0].into()),
    })
}

pub fn set(conn: &Connection, n: &Notes) -> Result<(), String> {
    if !KEYS.contains(&n.key.as_str()) {
        return Err(format!("{} isn't one of the choices.", n.key));
    }
    for (k, v) in [("meeting_on", if n.on { "1" } else { "0" }), ("meeting_key", n.key.as_str())] {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v])
            .map_err(|e| format!("Couldn't save meeting notes' settings: {e}"))?;
    }
    Ok(())
}

pub type Sink = Box<dyn FnMut(&[f32]) + Send>;
pub type Stop = Box<dyn FnOnce() + Send>;

/// The mic. `open` starts it and hands `on` 16 kHz mono samples as they come; the returned function stops it,
/// and `on` has seen everything by the time it returns.
pub trait Mic: Send + Sync {
    fn open(&self, on: Sink) -> Result<Stop, String>;
}

/// The default input device through cpal, on its own thread (a cpal stream can't move between threads).
pub struct Cpal;

impl Mic for Cpal {
    fn open(&self, mut on: Sink) -> Result<Stop, String> {
        let (stop, stop_rx) = channel::<()>();
        let (ready_tx, ready) = channel();
        let (done_tx, done) = channel::<()>();
        std::thread::spawn(move || {
            let run = || {
                let dev = cpal::default_host().default_input_device().ok_or("No microphone found.")?;
                let cfg = dev.default_input_config().map_err(|e| format!("Couldn't open the microphone: {e}"))?;
                // shortcut: f32 only, like dictation; convert other formats if a mic ever reports one.
                if cfg.sample_format() != cpal::SampleFormat::F32 {
                    return Err(format!("The microphone gives {} samples, which Myna can't read yet.", cfg.sample_format()));
                }
                let mut chunks = Chunks::new(cfg.sample_rate(), cfg.channels());
                let emit = move |data: &[f32], last: bool| chunks.push(data, last, &mut on);
                let shared = Arc::new(Mutex::new(emit));
                let cb = shared.clone();
                let stream = dev
                    .build_input_stream(cfg.config(), move |data: &[f32], _: &_| (cb.lock().unwrap())(data, false), |e| eprintln!("meeting microphone: {e}"), None)
                    .map_err(|e| format!("Couldn't open the microphone: {e}"))?;
                stream.play().map_err(|e| format!("Couldn't start the microphone: {e}"))?;
                Ok((stream, shared))
            };
            match run() {
                Ok((stream, shared)) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = stop_rx.recv(); // a message, or the stop function dropped
                    drop(stream);
                    (shared.lock().unwrap())(&[], true);
                }
                Err(e) => drop(ready_tx.send(Err(e))),
            }
            let _ = done_tx.send(());
        });
        ready.recv().map_err(|_| "The microphone stopped unexpectedly.".to_string())??;
        Ok(Box::new(move || {
            let _ = stop.send(());
            let _ = done.recv();
        }))
    }
}

/// Interleaved samples gathered a tenth of a second at a time and handed on as 16 kHz mono, so resampling's
/// rounding stays tiny (48 kHz and 44.1 kHz give whole samples).
struct Chunks {
    rate: u32,
    channels: u16,
    pending: Vec<f32>,
}

impl Chunks {
    fn new(rate: u32, channels: u16) -> Self {
        Chunks { rate, channels, pending: vec![] }
    }

    /// `last` hands on the rest too.
    fn push(&mut self, data: &[f32], last: bool, on: &mut impl FnMut(&[f32])) {
        let chunk = (self.rate as usize / 10) * self.channels as usize;
        self.pending.extend_from_slice(data);
        while self.pending.len() >= chunk || (last && !self.pending.is_empty()) {
            let n = self.pending.len().min(chunk);
            on(&crate::dictation::to_16k_mono(&self.pending[..n], self.rate, self.channels));
            self.pending.drain(..n);
        }
    }
}

/// What the other side of the call sends.
pub enum Heard<'a> {
    /// 16 kHz mono samples, the last of them heard at the instant given.
    Audio(Instant, &'a [f32]),
    /// The apps being listened to, when an app starts using the mic.
    Apps(Vec<String>),
    /// Listening stopped working: why.
    Failed(String),
}
pub type Ear = Box<dyn FnMut(Heard) + Send>;

/// The other people on the call: the sound of the apps using the mic. `open` starts listening; the returned
/// function stops it, and `on` has heard everything by the time it returns.
pub trait Them: Send + Sync {
    fn open(&self, on: Ear) -> Result<Stop, String>;
}

unsafe extern "C" {
    fn myna_tap_start(ctx: *mut c_void, audio: extern "C" fn(*mut c_void, *const f32, isize, u32, f64), event: extern "C" fn(*mut c_void, *const c_char));
    fn myna_tap_stop();
}

/// Core Audio process taps through the Swift bridge (`swift/Sources/MynaSwift/Tap.swift`). One at a time.
pub struct Tap;

struct TapCtx {
    on: Ear,
    chunks: Chunks,
}

extern "C" fn tap_audio(ctx: *mut c_void, data: *const f32, count: isize, channels: u32, rate: f64) {
    let mut ctx = unsafe { &*ctx.cast::<Mutex<TapCtx>>() }.lock().unwrap();
    let TapCtx { on, chunks } = &mut *ctx;
    // A new tap after an app joined can come at another rate.
    if (chunks.rate, chunks.channels) != (rate as u32, channels as u16) {
        chunks.push(&[], true, &mut |s| on(Heard::Audio(Instant::now(), s)));
        *chunks = Chunks::new(rate as u32, channels as u16);
    }
    let data = unsafe { std::slice::from_raw_parts(data, count.max(0) as usize) };
    chunks.push(data, false, &mut |s| on(Heard::Audio(Instant::now(), s)));
}

extern "C" fn tap_event(ctx: *mut c_void, json: *const c_char) {
    #[derive(serde::Deserialize)]
    struct Event {
        apps: Option<Vec<String>>,
        error: Option<String>,
    }
    let json = unsafe { CStr::from_ptr(json) }.to_string_lossy();
    let mut ctx = unsafe { &*ctx.cast::<Mutex<TapCtx>>() }.lock().unwrap();
    match serde_json::from_str::<Event>(&json) {
        Ok(Event { apps: Some(apps), .. }) => (ctx.on)(Heard::Apps(apps)),
        Ok(Event { error: Some(e), .. }) => (ctx.on)(Heard::Failed(e)),
        _ => (ctx.on)(Heard::Failed(format!("The call's audio answered oddly: {json}"))),
    }
}

impl Them for Tap {
    fn open(&self, on: Ear) -> Result<Stop, String> {
        let ctx = Box::into_raw(Box::new(Mutex::new(TapCtx { on, chunks: Chunks::new(48_000, 2) })));
        unsafe { myna_tap_start(ctx.cast(), tap_audio, tap_event) };
        let ctx = ctx as usize; // a raw pointer isn't Send
        Ok(Box::new(move || {
            unsafe { myna_tap_stop() }; // no callbacks after this, so the context can go
            let ctx = unsafe { Box::from_raw(ctx as *mut Mutex<TapCtx>) }.into_inner().unwrap();
            let TapCtx { mut on, mut chunks } = ctx;
            chunks.push(&[], true, &mut |s| on(Heard::Audio(Instant::now(), s)));
        }))
    }
}

type Wav = hound::WavWriter<BufWriter<File>>;

fn create(path: PathBuf) -> Result<Wav, String> {
    let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    hound::WavWriter::create(path, spec).map_err(|e| format!("Couldn't create the recording: {e}"))
}

/// Writes `samples`, flushing about once a second: a flush rewrites the header too, so a crash keeps everything
/// up to the last second.
fn write(wav: &mut Wav, since_flush: &mut usize, samples: &[f32]) {
    for s in samples {
        let _ = wav.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
    *since_flush += samples.len();
    if *since_flush >= RATE as usize {
        *since_flush = 0;
        let _ = wav.flush();
    }
}

/// Gaps shorter than this are the jitter of audio arriving in buffers, not silence.
const SLACK: u64 = RATE as u64 / 4;
/// This long with an app on the call and only digital silence: Myna can't hear it (macOS hands a denied tap zeros).
const DEAF: u64 = RATE as u64 * 20;

/// The Them track: its file, on the meeting clock (16 kHz samples since mic.wav's first), so sample N in
/// them.wav is sample N in mic.wav.
struct Track {
    wav: Option<Wav>,
    since_flush: usize,
    /// When an app first used the mic, on the meeting clock.
    app_at: Option<u64>,
    /// Where the latest audio ended.
    end: u64,
    /// Anything but digital silence since.
    heard: bool,
    failed: bool,
}

impl Track {
    fn written(&self) -> u64 {
        self.wav.as_ref().map_or(0, |w| w.len() as u64)
    }

    /// Silence up to `to`, for the time before the first app or a gap.
    fn pad(&mut self, to: u64) {
        let gap = to.saturating_sub(self.written()) as usize;
        if let Some(wav) = self.wav.as_mut() {
            write(wav, &mut self.since_flush, &vec![0.0; gap]);
        }
    }

    /// `samples` whose last was heard at `end`.
    fn put(&mut self, end: u64, samples: &[f32]) {
        let start = end.saturating_sub(samples.len() as u64);
        if start > self.written() + SLACK {
            self.pad(start);
        }
        self.heard |= samples.iter().any(|s| *s != 0.0);
        self.end = self.end.max(end);
        if let Some(wav) = self.wav.as_mut() {
            write(wav, &mut self.since_flush, samples);
        }
    }

    /// Myna can't hear the other people: listening failed, or an app has been on the call a while in digital silence.
    fn deaf(&self) -> bool {
        self.failed || self.app_at.is_some_and(|a| !self.heard && self.end >= a + DEAF)
    }
}

struct Active {
    id: i64,
    started: i64,
    stop: Stop,
    wav: Arc<Mutex<Option<Wav>>>,
    stop_them: Option<Stop>,
    them: Arc<Mutex<Track>>,
}

pub struct Meetings {
    db: Arc<Mutex<Connection>>,
    /// The data folder: each meeting gets `meetings/<id>/` in it.
    dir: PathBuf,
    mic: Box<dyn Mic>,
    them: Box<dyn Them>,
    active: Mutex<Option<Active>>,
}

/// `at` on the meeting clock that starts at `origin`.
fn on_clock(origin: Instant, at: Instant) -> u64 {
    (at.saturating_duration_since(origin).as_secs_f64() * RATE as f64) as u64
}

impl Meetings {
    pub fn new(db: Arc<Mutex<Connection>>, dir: PathBuf, mic: Box<dyn Mic>, them: Box<dyn Them>) -> Self {
        Meetings { db, dir, mic, them, active: Default::default() }
    }

    /// Whether Myna can't hear the other people on the call (the mic records regardless).
    pub fn deaf(&self) -> bool {
        self.active.lock().unwrap().as_ref().is_some_and(|a| a.them.lock().unwrap().deaf())
    }

    /// The recording in progress: its id and when it started (unix ms).
    pub fn running(&self) -> Option<(i64, i64)> {
        self.active.lock().unwrap().as_ref().map(|a| (a.id, a.started))
    }

    /// Starts a meeting at `now` (unix ms). Returns its id; one already recording is left alone.
    pub fn start(&self, now: i64) -> Result<i64, String> {
        let mut slot = self.active.lock().unwrap();
        if let Some(a) = slot.as_ref() {
            return Ok(a.id);
        }
        let db = self.db.lock().unwrap();
        db.execute("INSERT INTO meeting(started, state) VALUES (?1, 'recording')", [now]).map_err(|e| format!("Couldn't save the meeting: {e}"))?;
        let id = db.last_insert_rowid();
        drop(db);
        let folder = self.dir.join("meetings").join(id.to_string());
        let open = || -> Result<Active, String> {
            std::fs::create_dir_all(&folder).map_err(|e| format!("Couldn't create {}: {e}", folder.display()))?;
            let wav = Arc::new(Mutex::new(Some(create(folder.join("mic.wav"))?)));
            let them = Arc::new(Mutex::new(Track { wav: Some(create(folder.join("them.wav"))?), since_flush: 0, app_at: None, end: 0, heard: false, failed: false }));
            // The meeting clock starts with the mic's first sample.
            let origin = Arc::new(OnceLock::<Instant>::new());
            let (w, o) = (wav.clone(), origin.clone());
            let mut since_flush = 0;
            let on = move |samples: &[f32]| {
                o.get_or_init(|| Instant::now().checked_sub(Duration::from_secs_f64(samples.len() as f64 / RATE as f64)).unwrap_or_else(Instant::now));
                if let Some(wav) = w.lock().unwrap().as_mut() {
                    write(wav, &mut since_flush, samples);
                }
            };
            let stop = self.mic.open(Box::new(on))?;
            let (t, db) = (them.clone(), self.db.clone());
            let ear = move |h: Heard| {
                let mut t = t.lock().unwrap();
                match h {
                    // Before the mic's first sample there's no meeting clock yet; that's a moment at most.
                    Heard::Audio(at, samples) => {
                        if let Some(o) = origin.get() {
                            t.put(on_clock(*o, at), samples)
                        }
                    }
                    Heard::Apps(apps) if t.app_at.is_none() && !apps.is_empty() => {
                        t.app_at = Some(origin.get().map_or(0, |o| on_clock(*o, Instant::now())));
                        let _ = db.lock().unwrap().execute("UPDATE meeting SET source_app = ?1 WHERE id = ?2", (&apps[0], id));
                    }
                    Heard::Apps(_) => {}
                    Heard::Failed(e) => {
                        eprintln!("meeting: can't hear the call: {e}");
                        t.failed = true;
                    }
                }
            };
            // Never lose the user's audio: the mic records even when the call can't be heard.
            let stop_them = self.them.open(Box::new(ear)).inspect_err(|e| {
                eprintln!("meeting: can't hear the call: {e}");
                them.lock().unwrap().failed = true;
            });
            Ok(Active { id, started: now, stop, wav, stop_them: stop_them.ok(), them })
        };
        match open() {
            Ok(a) => {
                *slot = Some(a);
                Ok(id)
            }
            Err(e) => {
                let _ = self.db.lock().unwrap().execute("DELETE FROM meeting WHERE id = ?1", [id]);
                let _ = std::fs::remove_dir_all(&folder);
                Err(e)
            }
        }
    }

    /// Stops the recording and keeps it: the audio is finished and the meeting is marked recorded.
    pub fn stop(&self) -> Result<Option<i64>, String> {
        let Some(a) = self.active.lock().unwrap().take() else { return Ok(None) };
        (a.stop)();
        if let Some(stop) = a.stop_them {
            stop();
        }
        let wav = a.wav.lock().unwrap().take().ok_or("The recording was already closed.")?;
        let ms = wav.len() as i64 * 1000 / RATE as i64;
        // Both tracks last the whole meeting: Them is silence after the call's last sound, or all of it if no app joined.
        let mut them = a.them.lock().unwrap();
        them.pad(wav.len() as u64);
        let them = them.wav.take().ok_or("The recording was already closed.")?;
        wav.finalize().map_err(|e| format!("Couldn't finish the recording: {e}"))?;
        them.finalize().map_err(|e| format!("Couldn't finish the recording: {e}"))?;
        self.db
            .lock()
            .unwrap()
            .execute("UPDATE meeting SET duration_ms = ?1, state = 'recorded' WHERE id = ?2", (ms, a.id))
            .map_err(|e| format!("Couldn't save the meeting: {e}"))?;
        Ok(Some(a.id))
    }
}

/// "mm:ss", like the design's `fmtDur`.
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hands `on` two seconds of a tone at once, like a mic that already heard it.
    struct Fake(usize);
    impl Mic for Fake {
        fn open(&self, mut on: Sink) -> Result<Stop, String> {
            on(&(0..self.0).map(|i| (i as f32 / 50.0).sin() * 0.5).collect::<Vec<_>>());
            Ok(Box::new(|| {}))
        }
    }

    /// A call: hands `on` what's in it at once, heard `at` seconds from now (before, if negative).
    struct Call(Vec<(f64, Event)>);
    #[derive(Clone)]
    enum Event {
        Apps(&'static str),
        Audio(f32, usize),
        Failed,
    }
    impl Them for Call {
        fn open(&self, mut on: Ear) -> Result<Stop, String> {
            let now = Instant::now();
            for (at, e) in self.0.clone() {
                match e {
                    Event::Apps(a) => on(Heard::Apps(vec![a.into()])),
                    Event::Audio(v, n) => {
                        let d = Duration::from_secs_f64(at.abs());
                        on(Heard::Audio(if at < 0.0 { now - d } else { now + d }, &vec![v; n]))
                    }
                    Event::Failed => on(Heard::Failed("denied".into())),
                }
            }
            Ok(Box::new(|| {}))
        }
    }

    fn meetings(dir: &std::path::Path, mic: impl Mic + 'static) -> Meetings {
        meetings_with(dir, mic, Call(vec![]))
    }

    fn meetings_with(dir: &std::path::Path, mic: impl Mic + 'static, them: impl Them + 'static) -> Meetings {
        Meetings::new(Arc::new(Mutex::new(crate::storage::open(dir).unwrap())), dir.into(), Box::new(mic), Box::new(them))
    }

    fn samples(path: std::path::PathBuf) -> Vec<i16> {
        hound::WavReader::open(path).unwrap().into_samples::<i16>().map(Result::unwrap).collect()
    }

    fn track() -> (tempfile::TempDir, Track) {
        let tmp = tempfile::tempdir().unwrap();
        let wav = create(tmp.path().join("them.wav")).unwrap();
        (tmp, Track { wav: Some(wav), since_flush: 0, app_at: None, end: 0, heard: false, failed: false })
    }

    #[test]
    fn them_is_silence_before_the_first_sound_and_in_gaps_but_not_for_jitter() {
        let (_tmp, mut t) = track();
        let r = RATE as u64;
        t.put(2 * r, &[0.5; RATE as usize]); // the first second of sound ends at 2 s
        assert_eq!(t.written(), 2 * r, "a second of silence first");
        t.put(3 * r + 100, &[0.5; RATE as usize]); // 100 samples late: jitter
        assert_eq!(t.written(), 3 * r);
        t.put(6 * r, &[0.5; RATE as usize]); // nothing from 3 s to 5 s
        assert_eq!(t.written(), 6 * r);
        let wav = t.wav.take().unwrap();
        wav.finalize().unwrap();
        let s = samples(_tmp.path().join("them.wav"));
        let loud = |a: u64, b: u64| s[a as usize..b as usize].iter().all(|v| *v != 0);
        let quiet = |a: u64, b: u64| s[a as usize..b as usize].iter().all(|v| *v == 0);
        assert!(quiet(0, r) && loud(r, 3 * r) && quiet(3 * r, 5 * r) && loud(5 * r, 6 * r));
    }

    #[test]
    fn a_tap_that_only_gives_silence_with_an_app_on_the_call_is_deaf() {
        let (_tmp, mut t) = track();
        let r = RATE as u64;
        t.put(30 * r, &[0.0; 160]);
        assert!(!t.deaf(), "no app on the mic yet: silence is right");
        t.app_at = Some(30 * r);
        t.put(30 * r + DEAF - 160, &[0.0; 160]);
        assert!(!t.deaf(), "not long enough yet");
        t.put(30 * r + DEAF, &[0.0; 160]);
        assert!(t.deaf());
        t.put(30 * r + DEAF + 160, &[0.1; 160]);
        assert!(!t.deaf(), "someone spoke");
        let (_tmp, mut t) = track();
        t.failed = true;
        assert!(t.deaf());
    }

    #[test]
    fn them_wav_lines_up_with_mic_wav_and_the_first_app_is_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        // The mic has heard 4 s by the time the call opens; Zoom's half second of sound ended 1 s before that.
        let call = Call(vec![(0.0, Event::Apps("Zoom")), (-1.0, Event::Audio(0.5, RATE as usize / 2)), (0.0, Event::Apps("Google Chrome"))]);
        let m = meetings_with(tmp.path(), Fake(RATE as usize * 4), call);
        let id = m.start(1_000).unwrap();
        assert!(!m.deaf());
        m.stop().unwrap();
        let (mic, them) = (samples(tmp.path().join(format!("meetings/{id}/mic.wav"))), samples(tmp.path().join(format!("meetings/{id}/them.wav"))));
        assert_eq!(them.len(), mic.len(), "Them lasts the whole meeting");
        let first = them.iter().position(|v| *v != 0).unwrap() as i64;
        assert!((first - RATE as i64 * 5 / 2).abs() < RATE as i64 / 20, "starts at 2.5 s on the mic's clock, not {first}");
        let src: Option<String> = m.db.lock().unwrap().query_row("SELECT source_app FROM meeting WHERE id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(src.as_deref(), Some("Zoom"));
    }

    #[test]
    fn a_call_myna_cant_hear_still_records_the_mic() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings_with(tmp.path(), Fake(RATE as usize), Call(vec![(0.0, Event::Apps("Zoom")), (25.0, Event::Audio(0.0, RATE as usize))]));
        m.start(1).unwrap();
        assert!(m.deaf(), "25 s of digital silence with Zoom on the mic");
        m.stop().unwrap();
        let m = meetings_with(tmp.path(), Fake(RATE as usize), Call(vec![(0.0, Event::Failed)]));
        let id = m.start(1).unwrap();
        assert!(m.deaf());
        m.stop().unwrap();
        assert_eq!(samples(tmp.path().join(format!("meetings/{id}/mic.wav"))).len(), RATE as usize);
        struct Refuses;
        impl Them for Refuses {
            fn open(&self, _: Ear) -> Result<Stop, String> {
                Err("no".into())
            }
        }
        let m = meetings_with(tmp.path(), Fake(RATE as usize), Refuses);
        assert!(m.start(1).is_ok() && m.deaf());
    }

    #[test]
    fn start_and_stop_write_a_16k_wav_and_one_meeting_row() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(RATE as usize * 2 + 5));
        let id = m.start(1_000).unwrap();
        assert_eq!(m.start(2_000), Ok(id), "a second start leaves the recording alone");
        assert_eq!(m.running(), Some((id, 1_000)));
        assert_eq!(m.stop().unwrap(), Some(id));
        assert_eq!((m.running(), m.stop()), (None, Ok(None)));
        let r = hound::WavReader::open(tmp.path().join(format!("meetings/{id}/mic.wav"))).unwrap();
        assert_eq!((r.spec().sample_rate, r.spec().channels, r.spec().bits_per_sample), (16_000, 1, 16));
        assert_eq!(r.len(), RATE * 2 + 5);
        assert_eq!(samples(tmp.path().join(format!("meetings/{id}/them.wav"))), vec![0; RATE as usize * 2 + 5], "no app on the call: silence");
        let db = m.db.lock().unwrap();
        let row: (i64, i64, i64, Option<String>, String, String) = db
            .query_row("SELECT id, started, duration_ms, source_app, title, state FROM meeting", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .unwrap();
        assert_eq!(row, (id, 1_000, 2_000, None, "Untitled meeting".into(), "recorded".into()));
        assert_eq!(db.query_row("SELECT count(*) FROM meeting", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn a_mic_that_wont_open_leaves_no_meeting_behind() {
        struct Broken;
        impl Mic for Broken {
            fn open(&self, _: Sink) -> Result<Stop, String> {
                Err("No microphone found.".into())
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Broken);
        assert_eq!(m.start(1), Err("No microphone found.".into()));
        assert_eq!(m.db.lock().unwrap().query_row("SELECT count(*) FROM meeting", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert!(!tmp.path().join("meetings/1").exists());
    }

    #[test]
    fn an_unfinished_wav_still_reads_up_to_the_last_flush() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(RATE as usize * 3));
        let id = m.start(1).unwrap();
        // The app dies here: no stop, no finalize.
        let r = hound::WavReader::open(tmp.path().join(format!("meetings/{id}/mic.wav"))).unwrap();
        assert!(r.len() >= RATE, "flushed once a second: {}", r.len());
    }

    #[test]
    fn settings_default_off_and_save_only_listed_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let n = |on, key: &str| Notes { on, key: key.into() };
        assert_eq!(get(&conn), Ok(n(false, "Ctrl+Alt+KeyM")));
        set(&conn, &n(true, "")).unwrap();
        assert_eq!(get(&conn), Ok(n(true, "")));
        assert!(set(&conn, &n(true, "Alt+Space")).is_err());
        assert_eq!(get(&conn), Ok(n(true, "")));
    }

    #[test]
    fn the_clock_reads_like_the_design() {
        assert_eq!((clock(0), clock(61_999), clock(3_725_000)), ("00:00".into(), "01:01".into(), "62:05".into()));
    }
}
