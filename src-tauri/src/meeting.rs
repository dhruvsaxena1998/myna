//! Meeting notes: on or off with its shortcut, and a mic recording kept on disk as a meeting.
//! The mic reaches it only through `Mic`, so tests drive it with a fake.

use crate::runtime::RATE;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rusqlite::{Connection, OptionalExtension};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

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
                let (rate, channels) = (cfg.sample_rate(), cfg.channels());
                // A tenth of a second at a time, so resampling's rounding stays tiny (48 kHz and 44.1 kHz give whole samples).
                let chunk = (rate as usize / 10) * channels as usize;
                let mut pending: Vec<f32> = vec![];
                let emit = move |data: &[f32], last: bool| {
                    pending.extend_from_slice(data);
                    while pending.len() >= chunk || (last && !pending.is_empty()) {
                        let n = pending.len().min(chunk);
                        on(&crate::dictation::to_16k_mono(&pending[..n], rate, channels));
                        pending.drain(..n);
                    }
                };
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

type Wav = hound::WavWriter<BufWriter<File>>;

struct Active {
    id: i64,
    started: i64,
    stop: Stop,
    wav: Arc<Mutex<Option<Wav>>>,
}

pub struct Meetings {
    db: Arc<Mutex<Connection>>,
    /// The data folder: each meeting gets `meetings/<id>/` in it.
    dir: PathBuf,
    mic: Box<dyn Mic>,
    active: Mutex<Option<Active>>,
}

impl Meetings {
    pub fn new(db: Arc<Mutex<Connection>>, dir: PathBuf, mic: Box<dyn Mic>) -> Self {
        Meetings { db, dir, mic, active: Default::default() }
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
            let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let wav = hound::WavWriter::create(folder.join("mic.wav"), spec).map_err(|e| format!("Couldn't create the recording: {e}"))?;
            let wav = Arc::new(Mutex::new(Some(wav)));
            let w = wav.clone();
            let mut since_flush = 0;
            let on = move |samples: &[f32]| {
                let mut slot = w.lock().unwrap();
                let Some(wav) = slot.as_mut() else { return };
                for s in samples {
                    let _ = wav.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
                }
                // A flush rewrites the header too, so a crash keeps everything up to the last second.
                since_flush += samples.len();
                if since_flush >= RATE as usize {
                    since_flush = 0;
                    let _ = wav.flush();
                }
            };
            Ok(Active { id, started: now, stop: self.mic.open(Box::new(on))?, wav })
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
        let wav = a.wav.lock().unwrap().take().ok_or("The recording was already closed.")?;
        let ms = wav.len() as i64 * 1000 / RATE as i64;
        wav.finalize().map_err(|e| format!("Couldn't finish the recording: {e}"))?;
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

    fn meetings(dir: &std::path::Path, mic: impl Mic + 'static) -> Meetings {
        Meetings::new(Arc::new(Mutex::new(crate::storage::open(dir).unwrap())), dir.into(), Box::new(mic))
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
