//! Meeting notes: on or off with its shortcut, and a recording kept on disk as a meeting: `mic.wav` (Me) and
//! `them.wav` (the apps using the mic). The audio reaches it only through `Mic` and `Them`, so tests drive it with fakes.

use crate::addon::MuteState;
use crate::runtime::{RATE, Segment};
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
    /// Keep what I say while muted in Teams, instead of leaving it out. Read when a meeting is shown, so it works on old meetings too.
    #[serde(default)]
    pub keep_teams: bool,
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
        keep_teams: read(conn, "meeting_keep_teams")?.is_some_and(|v| v == "1"),
    })
}

pub fn set(conn: &Connection, n: &Notes) -> Result<(), String> {
    if !KEYS.contains(&n.key.as_str()) {
        return Err(format!("{} isn't one of the choices.", n.key));
    }
    for (k, v) in [("meeting_on", if n.on { "1" } else { "0" }), ("meeting_key", n.key.as_str()), ("meeting_keep_teams", if n.keep_teams { "1" } else { "0" })] {
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

/// Speech-to-text for a stretch of 16 kHz mono audio, times in seconds from its start. The app's is the
/// dictation model (`Session`), so a meeting never loads a second copy.
pub trait Stt: Send + Sync {
    fn transcribe(&self, audio: &[f32]) -> Result<Vec<Segment>, String>;
}

/// The most audio in one call: Parakeet takes about 4 to 5 minutes (`runtime.rs`).
const CHUNK: usize = 4 * 60 * RATE as usize;
/// Quieter than this (about -46 dB) all through a chunk is not speech: not worth the model's time.
const QUIET: i16 = 160;

/// Where to cut `samples` into pieces of at most `max`: at the quietest tenth of a second in the last quarter
/// before the limit, so a word isn't split.
fn cuts(samples: &[i16], max: usize) -> Vec<(usize, usize)> {
    let (mut out, mut from) = (vec![], 0);
    while samples.len() - from > max {
        let (win, end) = (RATE as usize / 10, from + max);
        let energy = |a: usize| samples[a..a + win].iter().map(|s| (*s as i64).pow(2)).sum::<i64>();
        let at = (end.saturating_sub(max / 4).max(from)..=end - win).step_by(win).min_by_key(|a| energy(*a)).unwrap_or(end - win);
        let cut = at + win / 2;
        out.push((from, cut));
        from = cut;
    }
    out.push((from, samples.len()));
    out
}

/// Sixteen kHz mono samples of a track. An unfinished file reads up to the last flush, which is what a crash keeps.
fn wav_samples(path: &std::path::Path) -> Result<Vec<i16>, String> {
    // A crash before the first flush leaves no header at all: nothing was kept.
    if std::fs::metadata(path).map_or(0, |m| m.len()) < 44 {
        return Ok(vec![]);
    }
    let r = hound::WavReader::open(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    r.into_samples::<i16>().collect::<Result<_, _>>().map_err(|e| format!("Couldn't read {}: {e}", path.display()))
}

/// Meetings that still need writing up: cut off while recording (a crash or quit), or while transcribing.
const UNFINISHED: &str = "('recording', 'recorded', 'transcribing')";

pub struct Meetings {
    db: Arc<Mutex<Connection>>,
    /// The data folder: each meeting gets `meetings/<id>/` in it.
    dir: PathBuf,
    mic: Box<dyn Mic>,
    them: Box<dyn Them>,
    active: Mutex<Option<Active>>,
    /// One meeting is transcribed at a time.
    job: Mutex<()>,
    chunk: usize,
    /// The mirrored mute state last heard, recording or not, so a meeting starts knowing it. `Unknown` once the add-on drops.
    mirrored: Mutex<Option<MuteState>>,
}

/// `at` on the meeting clock that starts at `origin`.
fn on_clock(origin: Instant, at: Instant) -> u64 {
    (at.saturating_duration_since(origin).as_secs_f64() * RATE as f64) as u64
}

impl Meetings {
    pub fn new(db: Arc<Mutex<Connection>>, dir: PathBuf, mic: Box<dyn Mic>, them: Box<dyn Them>) -> Self {
        Meetings { db, dir, mic, them, active: Default::default(), job: Default::default(), chunk: CHUNK, mirrored: Default::default() }
    }

    /// Whether Myna can't hear the other people on the call (the mic records regardless).
    pub fn deaf(&self) -> bool {
        self.active.lock().unwrap().as_ref().is_some_and(|a| a.them.lock().unwrap().deaf())
    }

    /// The mic as the add-on mirrors it: `Some(true)` muted, `Some(false)` live, None when Myna isn't seeing it.
    pub fn muted(&self) -> Option<bool> {
        match (*self.mirrored.lock().unwrap())? {
            MuteState::Muted => Some(true),
            MuteState::Unmuted => Some(false),
            MuteState::Unknown => None,
        }
    }

    /// The add-on saw the mute button in `state` at `at` (unix ms); `now` is the clock now. Kept on the timeline only while recording.
    pub fn mute(&self, state: MuteState, at: i64, now: i64) {
        *self.mirrored.lock().unwrap() = Some(state);
        if let Some((id, started)) = self.running() {
            // The add-on's clock is only trusted to within the meeting: never before it began or after now.
            self.mark(id, (at.min(now) - started).max(0), state);
        }
    }

    fn mark(&self, id: i64, at_ms: i64, state: MuteState) {
        let _ = self.db.lock().unwrap().execute("INSERT INTO mute_event(meeting, at_ms, state, source) VALUES (?1, ?2, ?3, 'teams')", (id, at_ms, state.as_str()));
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
                drop(slot);
                if let Some(s) = *self.mirrored.lock().unwrap() {
                    self.mark(id, 0, s); // what the button said when the meeting began
                }
                Ok(id)
            }
            Err(e) => {
                let _ = self.db.lock().unwrap().execute("DELETE FROM meeting WHERE id = ?1", [id]);
                let _ = std::fs::remove_dir_all(&folder);
                Err(e)
            }
        }
    }

    /// The meetings a launch must finish: the app died or quit before they were written up.
    pub fn unfinished(&self) -> Vec<i64> {
        let db = self.db.lock().unwrap();
        let Ok(mut q) = db.prepare(&format!("SELECT id FROM meeting WHERE state IN {UNFINISHED} ORDER BY id")) else { return vec![] };
        q.query_map([], |r| r.get(0)).map(|r| r.flatten().collect()).unwrap_or_default()
    }

    /// Whether a failed meeting can be tried again.
    pub fn failed(&self, id: i64) -> bool {
        let state: Option<String> = self.db.lock().unwrap().query_row("SELECT state FROM meeting WHERE id = ?1", [id], |r| r.get(0)).optional().unwrap_or(None);
        state.is_some_and(|s| s == "failed")
    }

    /// Writes up meeting `id` from its two tracks: Me from the mic, Them from the call, merged by time. `progress`
    /// gets 0 to 1 as chunks finish. Fails, with the reason kept in the meeting, without touching the audio.
    pub fn transcribe(&self, id: i64, stt: &dyn Stt, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let _one = self.job.lock().unwrap();
        let set = |sql: &str, p: &[&dyn rusqlite::ToSql]| self.db.lock().unwrap().execute(sql, p).map(drop).map_err(|e| format!("Couldn't save the meeting: {e}"));
        set("UPDATE meeting SET state = 'transcribing', failure = NULL WHERE id = ?1", &[&id])?;
        let done = self.write_up(id, stt, progress);
        if let Err(e) = &done {
            let _ = set("UPDATE meeting SET state = 'failed', failure = ?1 WHERE id = ?2", &[e, &id]);
        }
        done
    }

    /// The meeting's mute changes, in time order, ms from its start.
    fn timeline(&self, id: i64) -> Result<Vec<(i64, MuteState)>, String> {
        let db = self.db.lock().unwrap();
        let rows = db.prepare("SELECT at_ms, state FROM mute_event WHERE meeting = ?1 ORDER BY at_ms, id").and_then(|mut q| {
            q.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))).and_then(|r| r.collect::<Result<Vec<_>, _>>())
        });
        let rows = rows.map_err(|e| format!("Couldn't read the mute timeline: {e}"))?;
        Ok(rows.into_iter().map(|(at, s)| (at, match s.as_str() { "muted" => MuteState::Muted, "unmuted" => MuteState::Unmuted, _ => MuteState::Unknown })).collect())
    }

    fn write_up(&self, id: i64, stt: &dyn Stt, progress: &mut dyn FnMut(f32)) -> Result<(), String> {
        let folder = self.dir.join("meetings").join(id.to_string());
        let mic = wav_samples(&folder.join("mic.wav"))?;
        // The call's track may never have been made (the meeting ended as it started).
        let them = if folder.join("them.wav").exists() { wav_samples(&folder.join("them.wav"))? } else { vec![] };
        let rate = RATE as f64;
        let events = self.timeline(id)?;
        let pieces: Vec<_> = [("Me", &mic), ("Them", &them)].into_iter().flat_map(|(who, s)| cuts(s, self.chunk).into_iter().map(move |(a, b)| (who, &s[a..b], a))).collect();
        // shortcut: both tracks sit in memory as 16-bit samples (115 MB an hour each); stream from disk past a few hours.
        let mut said: Vec<(f64, f64, &str, String)> = vec![];
        for (i, (who, audio, at)) in pieces.iter().enumerate() {
            progress(i as f32 / pieces.len() as f32);
            if audio.iter().all(|s| s.abs() < QUIET) {
                continue;
            }
            let audio: Vec<f32> = audio.iter().map(|s| *s as f32 / 32768.0).collect();
            for s in stt.transcribe(&audio)? {
                let text = s.text.trim();
                let off = *at as f64 / rate;
                if !text.is_empty() {
                    said.push((off + s.start as f64, off + s.end as f64, who, text.into()));
                }
            }
        }
        said.sort_by(|a, b| a.0.total_cmp(&b.0)); // stable: Me before Them when they start together
        let mut db = self.db.lock().unwrap();
        let save = |db: &mut Connection| -> rusqlite::Result<()> {
            let tx = db.transaction()?;
            tx.execute("DELETE FROM segment WHERE meeting = ?1", [id])?; // a retry starts clean
            for (start, end, who, text) in &said {
                let (start, end) = ((start * 1000.0).round() as i64, (end * 1000.0).round() as i64);
                let mute = if *who == "Me" { mute_of(&events, start, end).map(MuteState::as_str) } else { None };
                tx.execute("INSERT INTO segment(meeting, start_ms, end_ms, speaker, text, mute) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", (id, start, end, who, text, mute))?;
            }
            // A meeting cut off by a crash never got its length.
            tx.execute("UPDATE meeting SET state = 'done', failure = NULL, duration_ms = COALESCE(duration_ms, ?2) WHERE id = ?1", (id, mic.len() as i64 * 1000 / RATE as i64))?;
            tx.commit()
        };
        save(&mut db).map_err(|e| format!("Couldn't save the notes: {e}"))
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

/// A meeting as the Meetings list shows it.
#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Row {
    pub id: i64,
    pub title: String,
    pub started: i64,
    pub duration_ms: Option<i64>,
    pub source_app: Option<String>,
    /// "recorded", "transcribing", "done" or "failed".
    pub state: String,
    pub failure: Option<String>,
}

/// The state of the mute button over a stretch of speech, from the timeline: the state that covers more than half
/// of it, else `Unknown` (shown, never hidden). Before the first change it's `Unknown`. None when the add-on never
/// reported, so a meeting without it has no marks.
// shortcut: a line is judged whole; a long line that's half muted is shown, not split. Split at the change if that proves too coarse.
fn mute_of(events: &[(i64, MuteState)], start: i64, end: i64) -> Option<MuteState> {
    if events.is_empty() {
        return None;
    }
    let end = end.max(start + 1); // a zero-length line still falls somewhere
    let mut over = [0i64; 3];
    let slot = |s: MuteState| match s { MuteState::Unmuted => 0, MuteState::Muted => 1, MuteState::Unknown => 2 };
    let (mut from, mut state) = (i64::MIN / 2, MuteState::Unknown);
    for (t, s) in events.iter().copied().chain([(i64::MAX / 2, MuteState::Unknown)]) {
        over[slot(state)] += (end.min(t) - start.max(from)).max(0);
        (from, state) = (t, s);
    }
    Some(match over {
        [u, _, _] if u * 2 > end - start => MuteState::Unmuted,
        [_, m, _] if m * 2 > end - start => MuteState::Muted,
        _ => MuteState::Unknown,
    })
}

#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Line {
    pub id: i64,
    pub speaker: String,
    pub start_ms: i64,
    /// Empty when `hidden`: what's left out never leaves the backend.
    pub text: String,
    /// Said while the add-on showed the mic muted.
    pub muted: bool,
    pub included: bool,
    /// Muted, not included, and not kept by the setting: collapsed on the page, out of Copy and anything sent to an AI.
    pub hidden: bool,
}

/// One meeting and what was said in it, for its page.
#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Page {
    #[serde(flatten)]
    pub meeting: Row,
    pub segments: Vec<Line>,
}

/// Every meeting, newest first. The one still recording isn't in it: the list shows it as its live card.
pub fn list(db: &Connection) -> Result<Vec<Row>, String> {
    let sql = "SELECT id, title, started, duration_ms, source_app, state, failure FROM meeting WHERE state <> 'recording' ORDER BY started DESC, id DESC";
    let rows = db.prepare(sql).and_then(|mut q| {
        q.query_map([], |r| Ok(Row { id: r.get(0)?, title: r.get(1)?, started: r.get(2)?, duration_ms: r.get(3)?, source_app: r.get(4)?, state: r.get(5)?, failure: r.get(6)? })).and_then(|r| r.collect())
    });
    rows.map_err(|e| format!("Couldn't read your meetings: {e}"))
}

pub fn page(db: &Connection, id: i64) -> Result<Page, String> {
    let meeting = list(db)?.into_iter().find(|m| m.id == id).ok_or("That meeting isn't there anymore.")?;
    Ok(Page { meeting, segments: lines(db, id)? })
}

fn lines(db: &Connection, id: i64) -> Result<Vec<Line>, String> {
    let keep = get(db)?.keep_teams;
    db.prepare("SELECT id, speaker, start_ms, text, mute, included FROM segment WHERE meeting = ?1 ORDER BY start_ms, id")
        .and_then(|mut q| {
            q.query_map([id], |r| {
                let (mute, included): (Option<String>, bool) = (r.get(4)?, r.get(5)?);
                let muted = mute.as_deref() == Some("muted");
                let hidden = muted && !included && !keep;
                Ok(Line { id: r.get(0)?, speaker: r.get(1)?, start_ms: r.get(2)?, text: if hidden { String::new() } else { r.get(3)? }, muted, included, hidden })
            })
            .and_then(|r| r.collect())
        })
        .map_err(|e| format!("Couldn't read the meeting: {e}"))
}

/// The one way a meeting's words leave it, for Copy and for anything sent to an AI provider: "Me 00:12\nwhat was said",
/// a blank line between. Lines said while muted are left out unless included or kept.
pub fn outgoing(db: &Connection, id: i64) -> Result<String, String> {
    Ok(lines(db, id)?.iter().filter(|l| !l.hidden).map(|l| format!("{} {}\n{}", l.speaker, clock(l.start_ms), l.text)).collect::<Vec<_>>().join("\n\n"))
}

/// Keeps these lines in the notes, with their "said while muted" marker.
pub fn include(db: &Connection, ids: &[i64]) -> Result<(), String> {
    for id in ids {
        db.execute("UPDATE segment SET included = 1 WHERE id = ?1", [id]).map_err(|e| format!("Couldn't keep that line: {e}"))?;
    }
    Ok(())
}

/// "Today 14:05", "Yesterday 09:00" or "3 Nov 14:05", on this Mac's clock: the menubar's Recent items.
pub fn when(ms: i64, now: i64) -> String {
    // The local day number and broken-down time of `ms`.
    let local = |ms: i64| {
        let t = (ms / 1000) as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&t, &mut tm) };
        ((t as i64 + tm.tm_gmtoff as i64).div_euclid(86_400), tm)
    };
    let ((day, tm), (today, _)) = (local(ms), local(now));
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let at = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
    match today - day {
        0 => format!("Today {at}"),
        1 => format!("Yesterday {at}"),
        _ => format!("{} {} {at}", tm.tm_mday, MONTHS[tm.tm_mon as usize % 12]),
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

    /// A ±0.5 square wave for `secs` seconds, or silence: every tenth of a second of it carries the same energy.
    fn wave(loud: bool, secs: f64) -> Vec<i16> {
        (0..(secs * RATE as f64) as usize).map(|i| if !loud { 0 } else if (i / 50) % 2 == 0 { 16_000 } else { -16_000 }).collect()
    }

    fn write_track(dir: &std::path::Path, id: i64, name: &str, pieces: &[(bool, f64)]) {
        let folder = dir.join(format!("meetings/{id}"));
        std::fs::create_dir_all(&folder).unwrap();
        let mut w = create(folder.join(name)).unwrap();
        for (loud, secs) in pieces {
            wave(*loud, *secs).into_iter().for_each(|s| w.write_sample(s).unwrap());
        }
        w.finalize().unwrap();
    }

    /// Says "1", "2", ... for each stretch of audio it's given, 0.1 s in, and counts what it was fed.
    #[derive(Default)]
    struct Count(Mutex<Vec<usize>>, Option<usize>);
    impl Stt for Count {
        fn transcribe(&self, audio: &[f32]) -> Result<Vec<Segment>, String> {
            let mut calls = self.0.lock().unwrap();
            calls.push(audio.len());
            if self.1 == Some(calls.len()) {
                return Err("The model fell over.".into());
            }
            Ok(vec![Segment { start: 0.1, end: 0.2, text: format!(" {} ", calls.len()) }])
        }
    }

    fn said(m: &Meetings, id: i64) -> Vec<(i64, String, String)> {
        let db = m.db.lock().unwrap();
        let mut q = db.prepare("SELECT start_ms, speaker, text FROM segment WHERE meeting = ?1 ORDER BY id").unwrap();
        q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    fn state(m: &Meetings, id: i64) -> (String, Option<String>) {
        m.db.lock().unwrap().query_row("SELECT state, failure FROM meeting WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
    }

    /// Chunks of 2 s. The mic cuts in its quiet gap at 1.85 s, then where a window first ties (3.4 s, 4.95 s);
    /// the call is silent until 3 s, so its first chunk and its last are skipped.
    fn two_tracks(tmp: &std::path::Path) -> (Meetings, i64) {
        let mut m = meetings(tmp, Fake(0));
        m.chunk = 2 * RATE as usize;
        let id = m.db.lock().unwrap().query_row("INSERT INTO meeting(started, state) VALUES (1, 'recorded') RETURNING id", [], |r| r.get(0)).unwrap();
        write_track(tmp, id, "mic.wav", &[(true, 1.8), (false, 0.4), (true, 3.8)]);
        write_track(tmp, id, "them.wav", &[(false, 3.0), (true, 1.0), (false, 2.0)]);
        (m, id)
    }

    #[test]
    fn me_and_them_are_merged_by_time_across_chunks_and_silence_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let (m, id) = two_tracks(tmp.path());
        let stt = Count::default();
        let mut seen = vec![];
        m.transcribe(id, &stt, &mut |p| seen.push(p)).unwrap();
        assert_eq!(stt.0.lock().unwrap().len(), 6, "four chunks of Me and two of Them; two silent ones never reached the model");
        let line = |ms: i64, who: &str, n: &str| (ms, who.to_string(), n.to_string());
        assert_eq!(said(&m, id), [line(100, "Me", "1"), line(1650, "Them", "5"), line(1950, "Me", "2"), line(3200, "Them", "6"), line(3500, "Me", "3"), line(5050, "Me", "4")]);
        assert_eq!(state(&m, id), ("done".into(), None));
        assert!(seen.windows(2).all(|w| w[0] <= w[1]) && seen[0] == 0.0 && *seen.last().unwrap() < 1.0, "{seen:?}");
        let ms: i64 = m.db.lock().unwrap().query_row("SELECT duration_ms FROM meeting", [], |r| r.get(0)).unwrap();
        assert_eq!(ms, 6_000, "a meeting without a length gets the mic's");
    }

    #[test]
    fn me_lines_get_their_mute_state_from_the_timeline_and_them_never_do() {
        let tmp = tempfile::tempdir().unwrap();
        let (m, id) = two_tracks(tmp.path());
        let marks = |m: &Meetings| -> Vec<(String, Option<String>)> {
            let db = m.db.lock().unwrap();
            let mut q = db.prepare("SELECT speaker, mute FROM segment WHERE meeting = ?1 ORDER BY start_ms").unwrap();
            q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
        };
        m.transcribe(id, &Count::default(), &mut |_| {}).unwrap();
        assert!(marks(&m).iter().all(|(_, s)| s.is_none()), "no add-on, no marks");
        for (at, s) in [(0, "unmuted"), (1900, "muted"), (3000, "unmuted"), (5000, "unknown")] {
            m.db.lock().unwrap().execute("INSERT INTO mute_event(meeting, at_ms, state, source) VALUES (?1, ?2, ?3, 'teams')", (id, at, s)).unwrap();
        }
        m.transcribe(id, &Count::default(), &mut |_| {}).unwrap(); // Me lines at 100, 1950, 3500 and 5050 ms
        let me: Vec<_> = marks(&m).into_iter().map(|(who, s)| (who, s.unwrap_or_default())).collect();
        let want = |who: &str, s: &str| (who.to_string(), s.to_string());
        assert_eq!(me, [want("Me", "unmuted"), want("Them", ""), want("Me", "muted"), want("Them", ""), want("Me", "unmuted"), want("Me", "unknown")]);
    }

    #[test]
    fn a_line_is_muted_when_more_than_half_of_it_is_and_the_edges_fall_as_documented() {
        use MuteState::*;
        let tl = [(0, Unmuted), (1000, Muted), (2000, Unmuted)];
        assert_eq!(mute_of(&[], 0, 100), None);
        assert_eq!(mute_of(&tl, 1100, 1900), Some(Muted));
        assert_eq!(mute_of(&tl, 900, 1100), Some(Unknown), "half and half is shown");
        assert_eq!(mute_of(&tl, 600, 1300), Some(Unmuted), "300 of 700 muted");
        assert_eq!(mute_of(&tl, 500, 1100), Some(Unmuted), "100 of 600 muted");
        assert_eq!(mute_of(&tl, 1000, 1999), Some(Muted), "starts on the change");
        assert_eq!(mute_of(&tl, 0, 1000), Some(Unmuted), "ends on the change");
        assert_eq!(mute_of(&tl, 1500, 1500), Some(Muted), "a zero-length line sits in the state it starts in");
        assert_eq!(mute_of(&tl, 1950, 2050), Some(Unknown), "50 muted, 50 unmuted");
        assert_eq!(mute_of(&[(500, Muted)], 0, 400), Some(Unknown), "before the first report nothing is known");
    }

    #[test]
    fn the_mirrored_state_is_timelined_only_while_recording_and_a_drop_is_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(0));
        m.mute(MuteState::Muted, 500, 500); // not recording: remembered, not kept
        assert_eq!(m.muted(), Some(true));
        let id = m.start(1000).unwrap(); // starts knowing what the button said
        m.mute(MuteState::Unmuted, 3000, 3100);
        m.mute(MuteState::Muted, 1_000_000, 4000); // a clock from the future is held to now
        m.mute(MuteState::Unknown, 5000, 5000); // the add-on dropped
        assert_eq!(m.muted(), None);
        let rows: Vec<(i64, String)> = {
            let db = m.db.lock().unwrap();
            let mut q = db.prepare("SELECT at_ms, state FROM mute_event WHERE meeting = ?1 ORDER BY id").unwrap();
            q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
        };
        assert_eq!(rows, [(0, "muted".into()), (2000, "unmuted".into()), (3000, "muted".into()), (4000, "unknown".into())]);
    }

    #[test]
    fn muted_lines_stay_out_of_what_leaves_the_meeting_until_included_or_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(0));
        let db = m.db.lock().unwrap();
        db.execute("INSERT INTO meeting(started, state) VALUES (1, 'done')", []).unwrap();
        for (start, who, text, mute) in [(1000, "Me", "hello", None), (2000, "Me", "secret", Some("muted")), (3000, "Them", "right", None), (4000, "Me", "maybe", Some("unknown"))] {
            db.execute("INSERT INTO segment(meeting, start_ms, end_ms, speaker, text, mute) VALUES (1, ?1, 0, ?2, ?3, ?4)", (start, who, text, mute)).unwrap();
        }
        let out = |db: &Connection| outgoing(db, 1).unwrap();
        assert_eq!(out(&db), "Me 00:01\nhello\n\nThem 00:03\nright\n\nMe 00:04\nmaybe");
        let p = page(&db, 1).unwrap();
        assert!(p.segments[1].hidden && p.segments[1].text.is_empty(), "the words don't even reach the page");
        include(&db, &[p.segments[1].id]).unwrap(); // survives a reopen: it's in the file
        assert!(out(&db).contains("secret") && page(&db, 1).unwrap().segments[1].included);
        db.execute("UPDATE segment SET included = 0", []).unwrap();
        assert!(!out(&db).contains("secret"));
        set(&db, &Notes { on: true, key: KEYS[0].into(), keep_teams: true }).unwrap();
        assert!(out(&db).contains("secret"), "the setting keeps muted lines, old meetings too");
    }

    #[test]
    fn a_failure_keeps_the_audio_and_the_reason_and_try_again_finishes() {
        let tmp = tempfile::tempdir().unwrap();
        let (m, id) = two_tracks(tmp.path());
        assert_eq!(m.transcribe(id, &Count(Default::default(), Some(2)), &mut |_| {}), Err("The model fell over.".into()));
        assert_eq!(state(&m, id), ("failed".into(), Some("The model fell over.".into())));
        assert!(said(&m, id).is_empty() && m.failed(id));
        assert!(tmp.path().join(format!("meetings/{id}/mic.wav")).exists() && tmp.path().join(format!("meetings/{id}/them.wav")).exists());
        assert!(m.unfinished().is_empty(), "a failed one waits for Try again, not for the next launch");
        m.transcribe(id, &Count::default(), &mut |_| {}).unwrap();
        assert_eq!((state(&m, id), said(&m, id).len(), m.failed(id)), (("done".into(), None), 6, false));
        m.transcribe(id, &Count::default(), &mut |_| {}).unwrap();
        assert_eq!(said(&m, id).len(), 6, "writing up again replaces the notes");
    }

    #[test]
    fn a_meeting_cut_off_mid_recording_is_found_and_written_up_on_the_next_launch() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(RATE as usize * 3));
        let id = m.start(1).unwrap();
        // The app dies here: no stop, no finalize. The next launch opens the same folder.
        let next = meetings(tmp.path(), Fake(0));
        assert_eq!(next.unfinished(), [id]);
        let stt = Count::default();
        next.transcribe(id, &stt, &mut |_| {}).unwrap();
        assert_eq!(*stt.0.lock().unwrap(), [RATE as usize * 3], "the flushed mic audio; the call was silent");
        assert_eq!((said(&next, id).len(), state(&next, id)), (1, ("done".into(), None)));
        assert!(next.unfinished().is_empty());
    }

    #[test]
    fn cuts_land_in_the_quiet_and_never_exceed_the_limit() {
        let tone: Vec<i16> = [wave(true, 1.0), wave(false, 0.4), wave(true, 1.0)].concat();
        let max = 1.5 * RATE as f64;
        let c = cuts(&tone, max as usize);
        assert_eq!(c.len(), 2);
        assert!(c[0].1 > (1.0 * RATE as f64) as usize && c[0].1 < (1.4 * RATE as f64) as usize, "{c:?}");
        assert_eq!(cuts(&tone, 10 * RATE as usize), [(0, tone.len())]);
    }

    #[test]
    fn settings_default_off_and_save_only_listed_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let n = |on, key: &str| Notes { on, key: key.into(), keep_teams: false };
        assert_eq!(get(&conn), Ok(n(false, "Ctrl+Alt+KeyM")));
        set(&conn, &n(true, "")).unwrap();
        assert_eq!(get(&conn), Ok(n(true, "")));
        assert!(set(&conn, &n(true, "Alt+Space")).is_err());
        assert_eq!(get(&conn), Ok(n(true, "")));
        set(&conn, &Notes { keep_teams: true, ..n(true, "") }).unwrap();
        assert!(get(&conn).unwrap().keep_teams);
    }

    #[test]
    fn the_clock_reads_like_the_design() {
        assert_eq!((clock(0), clock(61_999), clock(3_725_000)), ("00:00".into(), "01:01".into(), "62:05".into()));
    }

    #[test]
    fn list_is_newest_first_without_the_recording_and_page_has_the_lines_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let m = meetings(tmp.path(), Fake(0));
        let db = m.db.lock().unwrap();
        for (started, state) in [(10, "done"), (30, "recording"), (20, "failed")] {
            db.execute("INSERT INTO meeting(started, state, failure) VALUES (?1, ?2, ?3)", (started, state, (state == "failed").then_some("No model."))).unwrap();
        }
        for (start, who, text) in [(900, "Them", "b"), (100, "Me", "a")] {
            db.execute("INSERT INTO segment(meeting, start_ms, end_ms, speaker, text) VALUES (1, ?1, 0, ?2, ?3)", (start, who, text)).unwrap();
        }
        let rows = list(&db).unwrap();
        assert_eq!(rows.iter().map(|r| (r.id, r.state.as_str())).collect::<Vec<_>>(), [(3, "failed"), (1, "done")]);
        assert_eq!(rows[0].failure.as_deref(), Some("No model."));
        let p = page(&db, 1).unwrap();
        assert_eq!(p.segments.iter().map(|l| (l.speaker.as_str(), l.start_ms, l.text.as_str())).collect::<Vec<_>>(), [("Me", 100, "a"), ("Them", 900, "b")]);
        assert_eq!(p.meeting.title, "Untitled meeting");
        assert!(page(&db, 2).is_err(), "still recording: no page");
        assert!(page(&db, 99).is_err());
    }

    #[test]
    fn when_says_today_and_yesterday() {
        let now = 1_700_000_000_000;
        assert!(when(now, now).starts_with("Today "));
        assert!(when(now - 86_400_000, now).starts_with("Yesterday "));
        assert!(!when(now - 5 * 86_400_000, now).contains("day"));
    }
}
