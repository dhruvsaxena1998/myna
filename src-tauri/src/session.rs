//! The Dictation session: one dictation from shortcut to paste, and the speech model kept between dictations.
//! It reaches the Mac only through `World`, so tests drive it with a fake.

use crate::models::{self, System};
use crate::permissions::Mic;
use crate::runtime::{self, Transcriber};
use crate::{dictation, pill, rules, voice};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

/// What the pill shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Pill {
    /// `hold`: let go of `key` to paste; otherwise press it again, before `limit` seconds run out.
    Listening { hold: bool, key: String, limit: u64 },
    Writing,
    Added,
    /// Nothing.
    Idle,
    Error { title: String, hint: String },
}

/// A mic recording in progress.
pub trait Recording: Send {
    /// The mic's current loudness, 0 to 1.
    fn level(&self) -> f32;
    /// Stops the mic and returns what it heard as 16 kHz mono.
    fn finish(self: Box<Self>) -> Result<Vec<f32>, String>;
}

/// Everything the session needs from the Mac.
pub trait World: Send + Sync + 'static {
    fn mic(&self) -> Mic;
    /// Shows macOS's mic pop-up and waits for the answer.
    fn ask_mic(&self);
    fn ax(&self) -> bool;
    /// What macOS says about Apple Speech, which decides the active speech model.
    fn speech(&self) -> System;
    fn record(&self) -> Box<dyn Recording>;
    /// Fails with `dictation::NO_AX` when Accessibility is off, leaving the clipboard as it was.
    fn paste(&self, text: &str) -> Result<(), String>;
    fn show(&self, p: Pill);
    /// The mic loudness for the level bars, 0 to 1.
    fn level(&self, level: f32);
    /// Moves the pill to "bottom", "top-right", ... on the screen the user is typing on.
    fn place(&self, place: String);
    fn transcriber(&self, id: &str, models_dir: &Path) -> Result<Box<dyn Transcriber>, String>;
}

type Loaded = Option<(String, Box<dyn Transcriber>)>;

pub struct Session {
    world: Box<dyn World>,
    db: Arc<Mutex<Connection>>,
    models_dir: PathBuf,
    macos: u32,
    recording: Mutex<Option<Box<dyn Recording>>>,
    /// The loaded speech-to-text model and its id, kept between dictations (loading takes seconds).
    transcriber: Mutex<Loaded>,
    /// Bumped on every dictation, so an unload timer knows the model was used after it started.
    used: AtomicU64,
}

impl Session {
    pub fn new(world: Box<dyn World>, db: Arc<Mutex<Connection>>, models_dir: PathBuf, macos: u32) -> Arc<Self> {
        Arc::new(Session { world, db, models_dir, macos, recording: Default::default(), transcriber: Default::default(), used: Default::default() })
    }

    /// The global shortcut went down (`pressed`) or up. Hold mode starts on press and stops on release;
    /// tap mode toggles on release, since a press may repeat while the key is down and a release comes once.
    pub fn key(self: &Arc<Self>, pressed: bool) {
        let tap = voice::get(&self.db.lock().unwrap()).is_ok_and(|v| v.mode == "tap");
        match (pressed, tap) {
            (true, false) => self.start(true),
            (false, false) => self.stop(),
            (false, true) => self.toggle(),
            (true, true) => {}
        }
    }

    /// The menu item, or a tap of the shortcut: start, or stop if already listening.
    pub fn toggle(self: &Arc<Self>) {
        if self.recording.lock().unwrap().is_some() {
            self.stop();
        } else {
            self.start(false);
        }
    }

    /// Recording, or still turning the last recording into text.
    pub fn busy(&self) -> bool {
        // The transcriber stays locked after the key is let go, until the words are pasted.
        self.recording.lock().unwrap().is_some() || self.transcriber.try_lock().is_err()
    }

    /// Frees the speech-to-text model once it has sat unused for the chosen time.
    pub fn schedule_unload(self: &Arc<Self>) {
        let mine = self.used.fetch_add(1, Ordering::SeqCst) + 1;
        let secs = models::unload_after(&self.db.lock().unwrap()).unwrap_or(-1);
        if secs < 0 {
            return;
        }
        let s = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(secs as u64));
            if s.used.load(Ordering::SeqCst) == mine && s.recording.lock().unwrap().is_none() {
                *s.transcriber.lock().unwrap() = None;
            }
        });
    }

    /// The active speech-to-text model's id, if it's downloaded.
    fn active_stt(&self) -> Result<String, String> {
        let tasks = models::list(&self.db.lock().unwrap(), &self.models_dir, self.macos, &self.world.speech())?;
        let stt = tasks.into_iter().find(|t| t.id == "stt").ok_or("No speech-to-text task.")?;
        let id = stt.active.ok_or("No speech-to-text model runs on this Mac")?;
        match stt.models.iter().find(|m| m.id == id) {
            Some(m) if m.installed => Ok(id),
            Some(m) if m.system => Err(format!("Download {}’s language in Settings › AI & privacy", m.name)),
            Some(m) => Err(format!("Download {} in Settings › AI & privacy", m.name)),
            None => Err(format!("Unknown model {id}")),
        }
    }

    /// Loads the active model into `slot` unless it's already there.
    fn load(&self, slot: &mut Loaded) -> Result<(), String> {
        let id = self.active_stt()?;
        if slot.as_ref().is_none_or(|(loaded, _)| *loaded != id) {
            *slot = None; // free the old model before loading the new one
            *slot = Some((id.clone(), self.world.transcriber(&id, &self.models_dir)?));
        }
        Ok(())
    }

    /// Starts recording. `hold`: it stops when the shortcut is let go; otherwise on the next tap, or at the time limit.
    fn start(self: &Arc<Self>, hold: bool) {
        let mut rec = self.recording.lock().unwrap();
        if rec.is_some() {
            return; // key repeat
        }
        let Ok(v) = voice::get(&self.db.lock().unwrap()) else { return };
        if let Ok(look) = pill::look(&self.db.lock().unwrap()) {
            self.world.place(look.place);
        }
        // Say what's missing before the user talks, not after.
        let missing = match self.world.mic() {
            Mic::Allowed => None,
            Mic::Ask => {
                let s = self.clone();
                std::thread::spawn(move || s.world.ask_mic()); // macOS's pop-up; the next press records
                return;
            }
            Mic::Denied => Some(("Needs the Microphone", pill::ALLOW.to_string())),
        };
        let missing = missing
            .or_else(|| (!self.world.ax()).then(|| ("Needs Accessibility", pill::ALLOW.to_string())))
            .or_else(|| self.active_stt().err().map(|e| ("No speech model", e)));
        if let Some((title, hint)) = missing {
            return self.world.show(Pill::Error { title: title.into(), hint });
        }
        *rec = Some(self.world.record());
        drop(rec);
        let mine = self.used.fetch_add(1, Ordering::SeqCst) + 1; // a pending unload must not fire mid-dictation
        self.world.show(Pill::Listening { hold, key: v.key, limit: voice::TAP_LIMIT_SECS });
        let s = self.clone();
        std::thread::spawn(move || {
            // Load the model while the user talks; a failure shows when they let go.
            let _ = s.load(&mut s.transcriber.lock().unwrap());
        });
        let s = self.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            // Level bars, until this recording ends.
            while s.used.load(Ordering::SeqCst) == mine {
                if !hold && started.elapsed().as_secs() >= voice::TAP_LIMIT_SECS {
                    s.stop();
                    break;
                }
                match s.recording.lock().unwrap().as_ref() {
                    Some(r) => s.world.level(r.level()),
                    None => break,
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }

    fn stop(self: &Arc<Self>) {
        let Some(rec) = self.recording.lock().unwrap().take() else { return };
        let s = self.clone();
        std::thread::spawn(move || {
            let run = || -> Result<Pill, String> {
                let audio = rec.finish()?;
                if audio.len() < runtime::RATE as usize / 4 {
                    return Ok(Pill::Idle); // a tap, not speech
                }
                s.world.show(Pill::Writing);
                let mut slot = s.transcriber.lock().unwrap();
                s.load(&mut slot)?;
                let words = rules::load(&s.db.lock().unwrap())?;
                let model = slot.as_mut().unwrap().1.as_mut();
                model.bias(&rules::prompt(&words));
                let text = dictation::text(model, &audio, &words)?;
                drop(slot);
                if text.is_empty() {
                    let tap = voice::get(&s.db.lock().unwrap()).is_ok_and(|v| v.mode == "tap");
                    let hint = if tap { "Press the keys, talk, then press them again" } else { "Hold the keys while you talk" };
                    return Ok(Pill::Error { title: "Didn’t hear anything".into(), hint: hint.into() });
                }
                s.world.paste(&text)?;
                Ok(Pill::Added)
            };
            s.world.show(match run() {
                Ok(p) => p,
                Err(e) if e == dictation::NO_AX => Pill::Error { title: "Needs Accessibility".into(), hint: pill::ALLOW.into() },
                Err(e) => Pill::Error { title: "Couldn’t add your words".into(), hint: e },
            });
            s.schedule_unload();
        });
    }
}

/// The real Mac: permissions, the mic, the clipboard, the pill window and the model runtimes.
pub struct Mac(pub AppHandle);

impl Recording for dictation::Recording {
    fn level(&self) -> f32 {
        dictation::Recording::level(self)
    }
    fn finish(self: Box<Self>) -> Result<Vec<f32>, String> {
        dictation::Recording::finish(*self)
    }
}

impl World for Mac {
    fn mic(&self) -> Mic {
        crate::permissions::mic()
    }
    fn ask_mic(&self) {
        crate::permissions::ask_mic()
    }
    fn ax(&self) -> bool {
        crate::permissions::ax()
    }
    fn speech(&self) -> System {
        crate::apple::status()
    }
    fn record(&self) -> Box<dyn Recording> {
        Box::new(dictation::record())
    }
    fn paste(&self, text: &str) -> Result<(), String> {
        dictation::paste(text)
    }
    fn show(&self, p: Pill) {
        match p {
            Pill::Listening { hold, key, limit } => pill::listening(&self.0, hold, &key, limit),
            Pill::Writing => pill::show(&self.0, "writing"),
            Pill::Added => pill::show(&self.0, "added"),
            Pill::Idle => pill::show(&self.0, "idle"),
            Pill::Error { title, hint } => pill::error(&self.0, &title, &hint),
        }
    }
    fn level(&self, level: f32) {
        pill::level(&self.0, level)
    }
    fn place(&self, place: String) {
        pill::place(&self.0, place)
    }
    fn transcriber(&self, id: &str, models_dir: &Path) -> Result<Box<dyn Transcriber>, String> {
        runtime::transcriber(id, models_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Segment;
    use std::time::Instant;

    /// What the fake Mac answers, and what the session asked of it.
    #[derive(Default)]
    struct Log {
        mic: Option<Mic>,
        ax: bool,
        speech: System,
        /// What each recording hears; one second of sound unless set.
        audio: Option<Vec<f32>>,
        /// What the transcriber hears in that audio.
        words: Vec<&'static str>,
        /// Scripted failures: opening the mic, loading the model, transcribing.
        mic_fails: Option<&'static str>,
        load_fails: Option<&'static str>,
        transcribe_fails: Option<&'static str>,
        asked_mic: usize,
        recorded: usize,
        loaded: Vec<String>,
        freed: Vec<String>,
        shown: Vec<Pill>,
        pasted: Vec<String>,
    }

    #[derive(Clone, Default)]
    struct FakeMac(Arc<Mutex<Log>>);

    struct FakeRecording(Result<Vec<f32>, String>);

    /// A model that hears the log's words, or fails as scripted, and notes when it's freed.
    struct FakeModel(String, FakeMac);

    impl Transcriber for FakeModel {
        fn transcribe(&mut self, _: &[f32]) -> Result<Vec<Segment>, String> {
            let l = self.1.0.lock().unwrap();
            if let Some(e) = l.transcribe_fails {
                return Err(e.into());
            }
            Ok(l.words.iter().map(|t| Segment { start: 0.0, end: 1.0, text: t.to_string() }).collect())
        }
    }

    impl Drop for FakeModel {
        fn drop(&mut self) {
            self.1.0.lock().unwrap().freed.push(self.0.clone());
        }
    }

    impl Recording for FakeRecording {
        fn level(&self) -> f32 {
            0.5
        }
        fn finish(self: Box<Self>) -> Result<Vec<f32>, String> {
            self.0
        }
    }

    impl World for FakeMac {
        fn mic(&self) -> Mic {
            self.0.lock().unwrap().mic.unwrap_or(Mic::Allowed)
        }
        fn ask_mic(&self) {
            self.0.lock().unwrap().asked_mic += 1;
        }
        fn ax(&self) -> bool {
            self.0.lock().unwrap().ax
        }
        fn speech(&self) -> System {
            self.0.lock().unwrap().speech.clone()
        }
        fn record(&self) -> Box<dyn Recording> {
            let mut l = self.0.lock().unwrap();
            l.recorded += 1;
            let audio = l.audio.clone().unwrap_or(vec![0.1; runtime::RATE as usize]);
            Box::new(FakeRecording(l.mic_fails.map_or(Ok(audio), |e| Err(e.into()))))
        }
        fn paste(&self, text: &str) -> Result<(), String> {
            let mut l = self.0.lock().unwrap();
            if !l.ax {
                return Err(dictation::NO_AX.into());
            }
            l.pasted.push(text.into());
            Ok(())
        }
        fn show(&self, p: Pill) {
            self.0.lock().unwrap().shown.push(p);
        }
        fn level(&self, _: f32) {}
        fn place(&self, _: String) {}
        fn transcriber(&self, id: &str, _: &Path) -> Result<Box<dyn Transcriber>, String> {
            let mut l = self.0.lock().unwrap();
            if let Some(e) = l.load_fails {
                return Err(e.into());
            }
            l.loaded.push(id.into());
            Ok(Box::new(FakeModel(id.into(), self.clone())))
        }
    }

    impl FakeMac {
        /// Waits until `done` holds for the log, failing the test after a second.
        fn wait(&self, done: impl Fn(&Log) -> bool) {
            let t = Instant::now();
            while !done(&self.0.lock().unwrap()) {
                assert!(t.elapsed() < Duration::from_secs(1), "timed out waiting on the fake Mac");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        fn shown(&self) -> Vec<Pill> {
            self.0.lock().unwrap().shown.clone()
        }
    }

    /// A session on a Mac that allows everything and has Apple Speech, with a fresh database.
    fn session(mode: &str) -> (Arc<Session>, FakeMac, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::storage::open(tmp.path()).unwrap();
        let mut v = voice::get(&db).unwrap();
        v.mode = mode.into();
        voice::set(&db, &v).unwrap();
        let mac = FakeMac::default();
        {
            let mut l = mac.0.lock().unwrap();
            l.ax = true;
            l.speech = System { language: Some("English".into()), installed: true };
            l.words = vec![" Hello", "there. "];
        }
        let s = Session::new(Box::new(mac.clone()), Arc::new(Mutex::new(db)), tmp.path().join("models"), 26);
        (s, mac, tmp)
    }

    fn listening(hold: bool) -> Pill {
        Pill::Listening { hold, key: "Alt+Space".into(), limit: voice::TAP_LIMIT_SECS }
    }

    fn error(title: &str, hint: &str) -> Pill {
        Pill::Error { title: title.into(), hint: hint.into() }
    }

    #[test]
    fn hold_press_talk_and_let_go_pastes_the_words() {
        let (s, mac, _tmp) = session("hold");
        s.key(true);
        s.key(false);
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.shown(), [listening(true), Pill::Writing, Pill::Added]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
    }

    #[test]
    fn tap_mode_and_the_menu_item_start_on_one_call_and_paste_on_the_next() {
        let (s, mac, _tmp) = session("tap");
        s.key(true); // presses do nothing in tap mode
        assert_eq!(mac.0.lock().unwrap().recorded, 0);
        s.key(false);
        assert_eq!(mac.shown(), [listening(false)]);
        s.key(true);
        s.key(false);
        mac.wait(|l| l.pasted.len() == 1);

        s.toggle();
        s.toggle();
        mac.wait(|l| l.pasted.len() == 2);
        assert_eq!(mac.shown()[3], listening(false));
        assert_eq!(mac.0.lock().unwrap().recorded, 2);
    }

    #[test]
    fn a_press_while_recording_is_ignored() {
        let (s, mac, _tmp) = session("hold");
        s.key(true);
        s.key(true); // key repeat
        s.key(true);
        s.key(false);
        mac.wait(|l| l.pasted.len() == 1);
        assert_eq!(mac.0.lock().unwrap().recorded, 1);
        assert_eq!(mac.shown(), [listening(true), Pill::Writing, Pill::Added]);
    }

    #[test]
    fn checks_before_recording_run_mic_then_accessibility_then_model() {
        let (s, mac, _tmp) = session("hold");
        let no_model = "Download Parakeet TDT v3 in Settings › AI & privacy";
        let press = |set: &dyn Fn(&mut Log)| {
            set(&mut mac.0.lock().unwrap());
            mac.0.lock().unwrap().shown.clear();
            s.key(true);
            s.key(false);
            mac.shown()
        };

        // Nothing allowed and no model: macOS asks for the mic first, and the pill stays quiet.
        let shown = press(&|l| {
            l.mic = Some(Mic::Ask);
            l.ax = false;
            l.speech = System::default();
        });
        assert_eq!(shown, []);
        mac.wait(|l| l.asked_mic == 1);
        assert_eq!(press(&|l| l.mic = Some(Mic::Denied)), [error("Needs the Microphone", pill::ALLOW)]);
        assert_eq!(press(&|l| l.mic = Some(Mic::Allowed)), [error("Needs Accessibility", pill::ALLOW)]);
        assert_eq!(press(&|l| l.ax = true), [error("No speech model", no_model)]);
        assert_eq!(mac.0.lock().unwrap().recorded, 0);
    }

    #[test]
    fn the_model_loads_while_the_user_talks_and_stays_loaded() {
        let (s, mac, _tmp) = session("hold");
        s.key(true);
        mac.wait(|l| l.loaded.len() == 1); // before the key is let go
        s.key(false);
        mac.wait(|l| l.pasted.len() == 1);
        s.key(true);
        s.key(false);
        mac.wait(|l| l.pasted.len() == 2);
        assert_eq!(mac.0.lock().unwrap().loaded, ["apple-speech"]);
    }

    /// One whole dictation in `s`'s mode; returns what the pill showed after Listening.
    fn dictate(s: &Arc<Session>, mac: &FakeMac, tap: bool) -> Vec<Pill> {
        mac.0.lock().unwrap().shown.clear();
        if tap {
            s.toggle();
            s.toggle();
        } else {
            s.key(true);
            s.key(false);
        }
        // Ends on Added, Idle or an error.
        mac.wait(|l| l.shown.last().is_some_and(|p| !matches!(p, Pill::Listening { .. } | Pill::Writing)));
        mac.shown().into_iter().filter(|p| !matches!(p, Pill::Listening { .. })).collect()
    }

    #[test]
    fn a_recording_under_a_quarter_second_shows_and_pastes_nothing() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().audio = Some(vec![0.1; runtime::RATE as usize / 4 - 1]);
        assert_eq!(dictate(&s, &mac, false), [Pill::Idle]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
    }

    #[test]
    fn no_words_heard_says_so_with_a_hint_for_the_mode() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().words = vec!["Um,", " uh."]; // all filler: nothing left after rules cleanup
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, error("Didn’t hear anything", "Hold the keys while you talk")]);
        let (s, mac, _tmp) = session("tap");
        mac.0.lock().unwrap().words = vec![];
        assert_eq!(dictate(&s, &mac, true), [Pill::Writing, error("Didn’t hear anything", "Press the keys, talk, then press them again")]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
    }

    #[test]
    fn accessibility_lost_mid_dictation_pastes_nothing() {
        let (s, mac, _tmp) = session("hold");
        s.key(true);
        mac.0.lock().unwrap().ax = false;
        s.key(false);
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.shown()[1..], [Pill::Writing, error("Needs Accessibility", pill::ALLOW)]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
    }

    #[test]
    fn a_model_or_mic_failure_says_couldnt_add_your_words_with_the_reason() {
        let (s, mac, _tmp) = session("hold");
        let fails = |e: &str| error("Couldn’t add your words", e);
        mac.0.lock().unwrap().load_fails = Some("Couldn't load Apple Speech: broken");
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, fails("Couldn't load Apple Speech: broken")]);
        let mut l = mac.0.lock().unwrap();
        (l.load_fails, l.transcribe_fails) = (None, Some("The model choked."));
        drop(l);
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, fails("The model choked.")]);
        let mut l = mac.0.lock().unwrap();
        (l.transcribe_fails, l.mic_fails) = (None, Some("The microphone stopped unexpectedly."));
        drop(l);
        assert_eq!(dictate(&s, &mac, false), [fails("The microphone stopped unexpectedly.")]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
    }

    #[test]
    fn switching_models_frees_the_old_one_and_loads_the_new_one_on_the_next_dictation() {
        let (s, mac, tmp) = session("hold");
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        // Whisper downloaded (a sparse file at its catalog size) and picked on AI & privacy.
        let dir = tmp.path().join("models/whisper-large-v3-turbo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::File::create(dir.join("ggml-large-v3-turbo.bin")).unwrap().set_len(1624555275).unwrap();
        let speech = mac.speech();
        models::select(&s.db.lock().unwrap(), "stt", "whisper-large-v3-turbo", 26, &speech).unwrap();
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        let l = mac.0.lock().unwrap();
        assert_eq!((l.loaded.as_slice(), l.freed.as_slice()), (&["apple-speech".to_string(), "whisper-large-v3-turbo".into()][..], &["apple-speech".to_string()][..]));
    }
}
