//! The Dictation session: one dictation from shortcut to paste, and the speech model kept between dictations.
//! It reaches the Mac only through `World`, so tests drive it with a fake.

use crate::ai;
use crate::models::{self, System};
use crate::permissions::Mic;
use crate::runtime::{self, Transcriber};
use crate::trace::{self, Step, Trace};
use crate::{cleanup, dictation, pill, rules, voice};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::AppHandle;

/// What the pill shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Pill {
    /// `hold`: let go of `key` to paste; otherwise press it again, before `limit` seconds run out.
    Listening { hold: bool, key: String, limit: u64 },
    Writing,
    /// The LLM cleanup is running, after Writing.
    Cleaning,
    Added,
    /// Added, but as the rules left it: `reason` says why the cleanup didn't count.
    Fallback { reason: String },
    /// After Added or Fallback, with Ask how it went on: 👍/👎 for the saved dictation `trace`.
    Ask { trace: i64 },
    /// Nothing.
    Idle,
    Error { title: String, hint: String },
}

/// The sounds voice typing plays, when Sounds is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sound {
    Start,
    Stop,
}

/// Esc, and Esc with each shortcut's modifiers so it works while a shortcut is held; never one of the shortcuts itself.
fn esc_keys(v: &voice::Voice) -> Vec<String> {
    let keys: Vec<String> = v.keys().into_iter().map(|(k, _)| k).collect();
    let mods = keys.iter().filter_map(|k| k.rsplit_once('+').map(|(m, _)| format!("{m}+Escape")));
    let mut esc: Vec<String> = mods.chain(["Escape".to_string()]).filter(|e| !keys.contains(e)).collect();
    esc.sort();
    esc.dedup();
    esc
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
    fn paste(&self, text: &str, typed: bool) -> Result<(), String>;
    fn show(&self, p: Pill);
    /// The mic loudness for the level bars, 0 to 1.
    fn level(&self, level: f32);
    /// Moves the pill to "bottom", "top-right", ... on the screen the user is typing on.
    fn place(&self, place: String);
    fn sound(&self, s: Sound);
    /// Takes `keys` (Esc combinations) from every app while `on`, so Esc cancels the recording; gives them back when not.
    fn catch_esc(&self, keys: &[String], on: bool);
    /// The app in front: its name and bundle id.
    fn front(&self) -> (Option<String>, Option<String>);
    fn transcriber(&self, id: &str, models_dir: &Path) -> Result<Box<dyn Transcriber>, String>;
}

type Loaded = Option<(String, Box<dyn Transcriber>)>;

/// The recording in progress, and the token its level loop holds: the loop runs while the token is
/// still the one in the slot, so it stops when this recording ends, even if a new one starts at once.
struct Active {
    rec: Box<dyn Recording>,
    token: Arc<()>,
    /// What its trace starts with: the start time, hold or tap, the app in front.
    trace: Trace,
    /// Started by Dictate with cleanup.
    clean: bool,
    /// Sounds was on when it started, so its stop sound matches its start sound.
    sounds: bool,
    /// The Esc shortcuts caught while it records.
    esc: Vec<String>,
}

pub struct Session {
    world: Box<dyn World>,
    db: Arc<Mutex<Connection>>,
    /// API keys for the AI provider behind the Fast model.
    keys: Arc<dyn ai::Keys>,
    models_dir: PathBuf,
    macos: u32,
    recording: Mutex<Option<Active>>,
    /// The loaded speech-to-text model and its id, kept between dictations (loading takes seconds).
    transcriber: Mutex<Loaded>,
    /// Meeting notes' model, when it isn't voice typing's (the same one shares `transcriber`, so it's in memory once).
    meeting_transcriber: Mutex<Loaded>,
    /// Bumped by every dictation and every new unload timer; a timer frees the model only if it's still the latest.
    unload_gen: AtomicU64,
    /// Tap mode stops by itself after this long.
    tap_limit: Duration,
    /// How long one second of the "unload after" setting lasts: a second in the app, a millisecond in tests.
    second: Duration,
}

impl Session {
    pub fn new(world: Box<dyn World>, db: Arc<Mutex<Connection>>, keys: Arc<dyn ai::Keys>, models_dir: PathBuf, macos: u32, tap_limit: Duration, second: Duration) -> Arc<Self> {
        let (recording, transcriber, meeting_transcriber, unload_gen) = Default::default();
        Arc::new(Session { world, db, keys, models_dir, macos, recording, transcriber, meeting_transcriber, unload_gen, tap_limit, second })
    }

    /// A global shortcut went down (`pressed`) or up; `clean`: it's the Dictate with cleanup one. Hold mode starts on
    /// press and stops on release; tap mode toggles on release, since a press may repeat while the key is down and a release comes once.
    pub fn key(self: &Arc<Self>, pressed: bool, clean: bool) {
        let tap = voice::get(&self.db.lock().unwrap()).is_ok_and(|v| v.mode == "tap");
        match (pressed, tap) {
            (true, false) => self.start(true, clean),
            (false, false) => self.stop(None),
            (false, true) => self.toggle(clean),
            (true, true) => {}
        }
    }

    /// The menu item, or a tap of the shortcut: start, or stop if already listening.
    pub fn toggle(self: &Arc<Self>, clean: bool) {
        if self.recording.lock().unwrap().is_some() {
            self.stop(None);
        } else {
            self.start(false, clean);
        }
    }

    /// Esc while recording: drops what was heard and pastes nothing.
    pub fn cancel(self: &Arc<Self>) {
        let Some(a) = self.recording.lock().unwrap().take() else { return };
        self.world.catch_esc(&a.esc, false);
        let s = self.clone();
        std::thread::spawn(move || {
            let _ = a.rec.finish();
            if a.sounds {
                s.world.sound(Sound::Stop);
            }
            s.world.show(Pill::Idle);
            s.schedule_unload();
        });
    }

    /// Recording, or still turning the last recording into text.
    pub fn busy(&self) -> bool {
        // The transcriber stays locked after the key is let go, until the words are pasted.
        self.recording.lock().unwrap().is_some() || self.transcriber.try_lock().is_err()
    }

    /// Frees the speech-to-text models (voice typing's and meeting notes') once they have sat unused for the chosen time.
    pub fn schedule_unload(self: &Arc<Self>) {
        let mine = self.unload_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let secs = models::unload_after(&self.db.lock().unwrap()).unwrap_or(-1);
        if secs < 0 {
            return;
        }
        let s = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(s.second * secs as u32);
            // Never mid-dictation: a recording bumps the generation, and a timer set during one finds it here.
            if s.unload_gen.load(Ordering::SeqCst) == mine && s.recording.lock().unwrap().is_none() {
                *s.transcriber.lock().unwrap() = None;
                *s.meeting_transcriber.lock().unwrap() = None;
            }
        });
    }

    /// Voice typing's speech-to-text model's id, if it's downloaded.
    fn active_stt(&self) -> Result<String, String> {
        self.active_for("stt")
    }

    /// The active model's id for `task` ("stt" or meeting notes' `MEETING_STT`), if it's downloaded.
    fn active_for(&self, task: &str) -> Result<String, String> {
        let tasks = models::list(&self.db.lock().unwrap(), &self.models_dir, self.macos, &self.world.speech())?;
        let stt = tasks.into_iter().find(|t| t.id == task).ok_or("No speech-to-text task.")?;
        let id = stt.active.ok_or("No speech-to-text model runs on this Mac")?;
        let page = if task == "stt" { "Understand" } else { "Meeting notes" };
        match stt.models.iter().find(|m| m.id == id) {
            Some(m) if m.installed => Ok(id),
            Some(m) if m.system => Err(format!("Download {}’s language in Settings › {page}", m.name)),
            Some(m) => Err(format!("Download {} in Settings › {page}", m.name)),
            None => Err(format!("Unknown model {id}")),
        }
    }

    /// Loads model `id` into `slot` unless it's already there.
    fn load(&self, slot: &mut Loaded, id: &str) -> Result<(), String> {
        if slot.as_ref().is_none_or(|(loaded, _)| loaded != id) {
            *slot = None; // free the old model before loading the new one
            *slot = Some((id.into(), self.world.transcriber(id, &self.models_dir)?));
        }
        Ok(())
    }

    /// Starts recording. `hold`: it stops when the shortcut is let go; otherwise on the next tap, or at the time limit.
    fn start(self: &Arc<Self>, hold: bool, clean: bool) {
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
        let token = Arc::new(());
        let (app, bundle) = self.world.front();
        let started = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let trace = Trace { started, mode: if hold { "hold" } else { "tap" }, app, bundle, ..Default::default() };
        if v.sounds {
            self.world.sound(Sound::Start); // before the mic opens, so the words don't start with it
        }
        let esc = esc_keys(&v);
        *rec = Some(Active { rec: self.world.record(), token: token.clone(), trace, clean, sounds: v.sounds, esc: esc.clone() });
        drop(rec);
        self.world.catch_esc(&esc, true);
        self.unload_gen.fetch_add(1, Ordering::SeqCst); // a pending unload must not fire mid-dictation
        self.world.show(Pill::Listening { hold, key: v.key, limit: self.tap_limit.as_secs() });
        let s = self.clone();
        std::thread::spawn(move || {
            // Load the model while the user talks; a failure shows when they let go.
            let _ = s.active_stt().and_then(|id| s.load(&mut s.transcriber.lock().unwrap(), &id));
        });
        let s = self.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            // Level bars, until this recording ends.
            loop {
                let slot = s.recording.lock().unwrap();
                let Some(a) = slot.as_ref().filter(|a| Arc::ptr_eq(&a.token, &token)) else { break };
                if !hold && started.elapsed() >= s.tap_limit {
                    drop(slot);
                    s.stop(Some(&token));
                    break;
                }
                s.world.level(a.rec.level());
                drop(slot);
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }

    /// Stops the recording and turns it into text. `only`: unless a newer recording took its place.
    fn stop(self: &Arc<Self>, only: Option<&Arc<()>>) {
        let mut slot = self.recording.lock().unwrap();
        if only.is_some_and(|t| slot.as_ref().is_none_or(|a| !Arc::ptr_eq(&a.token, t))) {
            return;
        }
        let Some(Active { rec, trace: t, clean, sounds, esc, .. }) = slot.take() else { return };
        drop(slot);
        self.world.catch_esc(&esc, false);
        let released = Instant::now();
        let s = self.clone();
        std::thread::spawn(move || {
            // The trace, once the dictation reaches speech-to-text.
            let mut trace = None;
            let run = || -> Result<Pill, String> {
                let audio = rec.finish();
                if sounds {
                    s.world.sound(Sound::Stop); // after the mic closes, so the words don't end with it
                }
                let audio = audio?;
                if audio.len() < runtime::RATE as usize / 4 {
                    return Ok(Pill::Idle); // a tap, not speech
                }
                let audio_ms = audio.len() as i64 * 1000 / runtime::RATE as i64;
                let t = trace.insert(Trace { audio_ms, outcome: "failed", ..t });
                s.world.show(Pill::Writing);
                let id = s.active_stt()?;
                t.model = Some(id.clone());
                let wait = Instant::now();
                let mut slot = s.transcriber.lock().unwrap(); // held by the load started on press, if still loading
                let loaded = s.load(&mut slot, &id);
                t.load_ms = Some(ms(wait.elapsed()));
                loaded?;
                let voice = voice::get(&s.db.lock().unwrap())?;
                let words = rules::load(&s.db.lock().unwrap())?;
                let model = slot.as_mut().unwrap().1.as_mut();
                model.bias(&rules::prompt(&words));
                let at = Instant::now();
                let heard = dictation::text(model, &audio)?;
                drop(slot);
                t.steps.push(Step { kind: "speech-to-text", text: heard.clone(), ms: ms(at.elapsed()), settings: None, edits: None });
                let at = Instant::now();
                let (mut text, edits) = rules::edits(&heard, &words);
                let settings = serde_json::json!({ "language": words.language, "tidy": words.tidy }).to_string();
                let edits = serde_json::to_string(&edits).ok();
                t.steps.push(Step { kind: "rules", text: text.clone(), ms: ms(at.elapsed()), settings: Some(settings), edits });
                let mut fallback = None;
                if clean && !text.is_empty() {
                    s.world.show(Pill::Cleaning);
                    let at = Instant::now();
                    let (model, done) = s.cleanup(&text, &words, t.app.as_deref());
                    let settings = serde_json::json!({ "model": model, "cost": done.cost, "fallback": done.fallback.as_ref().map(|f| f.reason), "detail": done.fallback.as_ref().and_then(|f| f.detail.as_ref()) });
                    t.steps.push(Step { kind: "cleanup", text: done.text.clone(), ms: ms(at.elapsed()), settings: Some(settings.to_string()), edits: None });
                    (text, fallback) = (done.text, done.fallback.map(|f| f.reason));
                }
                if text.is_empty() {
                    t.outcome = "nothing_heard";
                    let tap = voice.mode == "tap";
                    let hint = if tap { "Press the keys, talk, then press them again" } else { "Hold the keys while you talk" };
                    return Ok(Pill::Error { title: "Didn’t hear anything".into(), hint: hint.into() });
                }
                t.outcome = "not_pasted";
                s.world.paste(&text, voice.paste == "type")?;
                (t.outcome, t.paste_ms) = ("pasted", Some(ms(released.elapsed())));
                Ok(fallback.map_or(Pill::Added, |reason| Pill::Fallback { reason: reason.into() }))
            };
            let result = run();
            let mut saved = None;
            if let Some(mut t) = trace {
                t.reason = result.as_ref().err().cloned();
                match trace::save(&s.db.lock().unwrap(), &t) {
                    Ok(id) => saved = Some(id),
                    Err(e) => eprintln!("{e}"),
                }
            }
            let pasted = matches!(result, Ok(Pill::Added | Pill::Fallback { .. }));
            s.world.show(match result {
                Ok(p) => p,
                Err(e) if e == dictation::NO_AX => Pill::Error { title: "Needs Accessibility".into(), hint: pill::ALLOW.into() },
                Err(e) => Pill::Error { title: "Couldn’t add your words".into(), hint: e },
            });
            let ask = voice::get(&s.db.lock().unwrap()).is_ok_and(|v| v.rate);
            if let (true, true, Some(trace)) = (pasted, ask, saved) {
                s.world.show(Pill::Ask { trace });
            }
            s.schedule_unload();
        });
    }
}

/// A meeting's audio goes through meeting notes' model, one chunk at a time. When that's voice typing's model too, it
/// shares the dictation's slot: the lock is let go between chunks, so a dictation waits for at most the chunk in
/// progress. A different model has its own slot, so neither swaps the other out.
impl crate::meeting::Stt for Session {
    fn transcribe(&self, audio: &[f32]) -> Result<Vec<runtime::Segment>, String> {
        self.unload_gen.fetch_add(1, Ordering::SeqCst); // a pending unload must not fire between chunks
        let id = self.active_for(models::MEETING_STT)?;
        let shared = self.active_stt().is_ok_and(|v| v == id);
        let mut slot = if shared {
            *self.meeting_transcriber.lock().unwrap() = None; // the picks became the same: one copy is enough
            self.transcriber.lock().unwrap()
        } else {
            self.meeting_transcriber.lock().unwrap()
        };
        self.load(&mut slot, &id)?;
        let model = slot.as_mut().unwrap().1.as_mut();
        model.bias(""); // the last dictation's words are not this meeting's
        model.transcribe(audio)
    }
}

/// The Them track is told apart by the active diarization model, loaded for the meeting and dropped after.
impl crate::meeting::Voices for Session {
    fn diarize(&self, audio: &[f32]) -> Result<Vec<runtime::Turn>, String> {
        let tasks = models::list(&self.db.lock().unwrap(), &self.models_dir, self.macos, &self.world.speech())?;
        let diar = tasks.into_iter().find(|t| t.id == "diar").ok_or("No voice task.")?;
        let installed = |id: &str| diar.models.iter().any(|m| m.id == id && m.installed);
        let mut d = runtime::pick_diarizer(diar.active.as_deref().unwrap_or("sortformer"), &installed, &|id| runtime::diarizer(id, &self.models_dir))?;
        d.diarize(audio)
    }
}

impl Session {
    /// Dictate with cleanup's step: the Fast model's cleanup of `text`, or `text` itself and why. Also the model's name.
    fn cleanup(&self, text: &str, words: &rules::Settings, app: Option<&str>) -> (Option<String>, cleanup::Outcome) {
        let fail = |reason, detail: String| cleanup::Outcome { text: text.into(), cost: None, fallback: Some(cleanup::Fallback { reason, detail: Some(detail) }) };
        let db = self.db.lock().unwrap();
        let (record, model) = match ai::route(&db, "fast") {
            Ok(r) => r,
            Err(e) => return (None, fail("No Fast model", e)),
        };
        let backend = ai::provider(&db, &*self.keys, record.id);
        let instruction = cleanup::instruction(&db, app).unwrap_or_else(|_| cleanup::DEFAULT.into());
        drop(db);
        match backend {
            Ok(b) => (Some(model.clone()), cleanup::run(b, &model, &instruction, text, words, self.second * cleanup::BUDGET.as_secs() as u32)),
            Err(e) => (Some(model), fail("AI didn’t answer", e)),
        }
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
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
    fn paste(&self, text: &str, typed: bool) -> Result<(), String> {
        dictation::paste(text, typed)
    }
    fn show(&self, p: Pill) {
        match p {
            Pill::Listening { hold, key, limit } => pill::listening(&self.0, hold, &key, limit),
            Pill::Writing => pill::show(&self.0, "writing"),
            Pill::Cleaning => pill::show(&self.0, "cleaning"),
            Pill::Fallback { reason } => pill::fallback(&self.0, &reason),
            Pill::Added => pill::show(&self.0, "added"),
            Pill::Ask { trace } => pill::ask(&self.0, trace),
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
    fn sound(&self, s: Sound) {
        let name = match s {
            Sound::Start => "Pop",
            Sound::Stop => "Purr",
        };
        // NSSound isn't Send, so it's made and played on the main thread.
        let _ = self.0.run_on_main_thread(move || {
            if let Some(sound) = objc2_app_kit::NSSound::soundNamed(&objc2_foundation::NSString::from_str(name)) {
                sound.play();
            }
        });
    }
    fn catch_esc(&self, keys: &[String], on: bool) {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = self.0.global_shortcut();
        for k in keys {
            if let Err(e) = if on { gs.register(k.as_str()) } else { gs.unregister(k.as_str()) } {
                eprintln!("dictation: Esc {k}: {e}");
            }
        }
    }
    fn front(&self) -> (Option<String>, Option<String>) {
        let Some(app) = objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication() else { return (None, None) };
        (app.localizedName().map(|n| n.to_string()), app.bundleIdentifier().map(|b| b.to_string()))
    }
    fn transcriber(&self, id: &str, models_dir: &Path) -> Result<Box<dyn Transcriber>, String> {
        let t = runtime::transcriber(id, models_dir);
        // The pill says it too, but only for this dictation; the model's row in Settings keeps it.
        match &t {
            Ok(_) => crate::model_event(&self.0, id, "loaded", serde_json::json!({})),
            Err(e) => crate::model_event(&self.0, id, "failed", serde_json::json!({ "error": e })),
        }
        t
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
        levels: usize,
        shown: Vec<Pill>,
        pasted: Vec<String>,
        typed: Vec<bool>,
        sounds: Vec<Sound>,
        /// The Esc shortcuts caught right now.
        esc: Vec<String>,
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
        fn paste(&self, text: &str, typed: bool) -> Result<(), String> {
            let mut l = self.0.lock().unwrap();
            if !l.ax {
                return Err(dictation::NO_AX.into());
            }
            l.pasted.push(text.into());
            l.typed.push(typed);
            Ok(())
        }
        fn show(&self, p: Pill) {
            self.0.lock().unwrap().shown.push(p);
        }
        fn level(&self, _: f32) {
            self.0.lock().unwrap().levels += 1;
        }
        fn place(&self, _: String) {}
        fn sound(&self, s: Sound) {
            self.0.lock().unwrap().sounds.push(s);
        }
        fn catch_esc(&self, keys: &[String], on: bool) {
            let esc = &mut self.0.lock().unwrap().esc;
            if on { esc.extend_from_slice(keys) } else { esc.retain(|k| !keys.contains(k)) }
        }
        fn front(&self) -> (Option<String>, Option<String>) {
            (Some("Notes".into()), Some("com.apple.Notes".into()))
        }
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
        session_timed(mode, Duration::from_secs(voice::TAP_LIMIT_SECS), Duration::from_secs(1))
    }

    /// `second`: how long a second of the "unload after" setting lasts.
    fn session_timed(mode: &str, tap_limit: Duration, second: Duration) -> (Arc<Session>, FakeMac, tempfile::TempDir) {
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
        let s = Session::new(Box::new(mac.clone()), Arc::new(Mutex::new(db)), Arc::new(crate::ai::tests::Memory::default()), tmp.path().join("models"), 26, tap_limit, second);
        (s, mac, tmp)
    }

    fn listening(hold: bool) -> Pill {
        Pill::Listening { hold, key: "Alt+Space".into(), limit: voice::TAP_LIMIT_SECS }
    }

    fn error(title: &str, hint: &str) -> Pill {
        Pill::Error { title: title.into(), hint: hint.into() }
    }

    /// Each saved trace: how it ended, why, the model and each step's text.
    fn traces(s: &Session) -> Vec<serde_json::Value> {
        let db = s.db.lock().unwrap();
        let mut q = db.prepare("SELECT id, outcome, reason, model FROM trace ORDER BY id").unwrap();
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?)));
        let rows: Vec<_> = rows.unwrap().map(Result::unwrap).collect();
        let mut q = db.prepare("SELECT kind, text FROM step WHERE trace = ?1 ORDER BY rowid").unwrap();
        rows.into_iter()
            .map(|(id, outcome, reason, model)| {
                let steps: Vec<(String, String)> = q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
                serde_json::json!({ "outcome": outcome, "reason": reason, "model": model, "steps": steps })
            })
            .collect()
    }

    fn traced(outcome: &str, reason: Option<&str>, steps: &[(&str, &str)]) -> serde_json::Value {
        serde_json::json!({ "outcome": outcome, "reason": reason, "model": "apple-speech", "steps": steps })
    }

    #[test]
    fn hold_press_talk_and_let_go_pastes_the_words() {
        let (s, mac, _tmp) = session("hold");
        s.key(true, false);
        s.key(false, false);
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.shown(), [listening(true), Pill::Writing, Pill::Added]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
    }

    #[test]
    fn a_pasted_dictation_saves_its_trace() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().words = vec!["Uh, hello", "there."];
        let before = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        let steps = [("speech-to-text", "Uh, hello there."), ("rules", "Hello there.")];
        assert_eq!(traces(&s), [traced("pasted", None, &steps)]);

        let db = s.db.lock().unwrap();
        let q = "SELECT started, mode, app, bundle, audio_ms, load_ms, paste_ms FROM trace";
        let (started, mode, app, bundle, audio_ms, load_ms, paste_ms): (i64, String, String, String, i64, f64, f64) =
            db.query_row(q, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?))).unwrap();
        assert!(started >= before && started - before < 1000);
        assert_eq!((mode.as_str(), app.as_str(), bundle.as_str(), audio_ms), ("hold", "Notes", "com.apple.Notes", 1000));
        assert!(0.0 <= load_ms && load_ms <= paste_ms, "{load_ms} {paste_ms}");
        let q = "SELECT settings, edits FROM step WHERE kind = 'rules'";
        let (settings, edits): (String, String) = db.query_row(q, [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(settings, r#"{"language":"en","tidy":true}"#);
        assert_eq!(edits, r#"[{"kind":"filler","start":0,"end":4,"to":""}]"#);
        let q = "SELECT count(*) FROM step WHERE settings IS NULL AND edits IS NULL AND kind = 'speech-to-text'";
        assert_eq!(db.query_row(q, [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }

    #[test]
    fn a_tap_mode_dictation_says_so_in_its_trace() {
        let (s, mac, _tmp) = session("tap");
        dictate(&s, &mac, true);
        let mode: String = s.db.lock().unwrap().query_row("SELECT mode FROM trace", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "tap");
    }

    #[test]
    fn the_paste_method_setting_decides_between_pasting_and_typing() {
        let (s, mac, _tmp) = session("hold");
        for (method, n) in [("clipboard", 1), ("type", 2)] {
            let mut v = voice::get(&s.db.lock().unwrap()).unwrap();
            v.paste = method.into();
            voice::set(&s.db.lock().unwrap(), &v).unwrap();
            s.key(true, false);
            s.key(false, false);
            mac.wait(|l| l.pasted.len() == n);
        }
        assert_eq!(mac.0.lock().unwrap().typed, [false, true]);
    }

    #[test]
    fn tap_mode_and_the_menu_item_start_on_one_call_and_paste_on_the_next() {
        let (s, mac, _tmp) = session("tap");
        s.key(true, false); // presses do nothing in tap mode
        assert_eq!(mac.0.lock().unwrap().recorded, 0);
        s.key(false, false);
        assert_eq!(mac.shown(), [listening(false)]);
        s.key(true, false);
        s.key(false, false);
        mac.wait(|l| l.shown.len() == 3); // Added: the dictation is over

        s.toggle(false);
        s.toggle(false);
        mac.wait(|l| l.pasted.len() == 2);
        assert_eq!(mac.shown()[3], listening(false));
        assert_eq!(mac.0.lock().unwrap().recorded, 2);
    }

    #[test]
    fn a_press_while_recording_is_ignored() {
        let (s, mac, _tmp) = session("hold");
        s.key(true, false);
        s.key(true, false); // key repeat
        s.key(true, false);
        s.key(false, false);
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.0.lock().unwrap().recorded, 1);
        assert_eq!(mac.shown(), [listening(true), Pill::Writing, Pill::Added]);
    }

    #[test]
    fn checks_before_recording_run_mic_then_accessibility_then_model() {
        let (s, mac, _tmp) = session("hold");
        let no_model = "Download Parakeet TDT v3 in Settings › Understand";
        let press = |set: &dyn Fn(&mut Log)| {
            set(&mut mac.0.lock().unwrap());
            mac.0.lock().unwrap().shown.clear();
            s.key(true, false);
            s.key(false, false);
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
        assert!(traces(&s).is_empty(), "blocked before recording: no trace");
    }

    #[test]
    fn the_model_loads_while_the_user_talks_and_stays_loaded() {
        let (s, mac, _tmp) = session("hold");
        s.key(true, false);
        mac.wait(|l| l.loaded.len() == 1); // before the key is let go
        s.key(false, false);
        mac.wait(|l| l.pasted.len() == 1);
        s.key(true, false);
        s.key(false, false);
        mac.wait(|l| l.pasted.len() == 2);
        assert_eq!(mac.0.lock().unwrap().loaded, ["apple-speech"]);
    }

    /// One whole dictation in `s`'s mode; returns what the pill showed after Listening.
    fn dictate(s: &Arc<Session>, mac: &FakeMac, tap: bool) -> Vec<Pill> {
        press(s, mac, tap, false)
    }

    /// One dictation from the Dictate shortcut, or from Dictate with cleanup if `clean`.
    fn press(s: &Arc<Session>, mac: &FakeMac, tap: bool, clean: bool) -> Vec<Pill> {
        mac.0.lock().unwrap().shown.clear();
        if tap {
            s.toggle(clean);
            s.toggle(clean);
        } else {
            s.key(true, clean);
            s.key(false, clean);
        }
        // Ends on Added, a fallback, Idle or an error.
        mac.wait(|l| l.shown.last().is_some_and(|p| !matches!(p, Pill::Listening { .. } | Pill::Writing | Pill::Cleaning)));
        mac.shown().into_iter().filter(|p| !matches!(p, Pill::Listening { .. })).collect()
    }

    #[test]
    fn a_recording_under_a_quarter_second_shows_and_pastes_nothing() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().audio = Some(vec![0.1; runtime::RATE as usize / 4 - 1]);
        assert_eq!(dictate(&s, &mac, false), [Pill::Idle]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
        assert!(traces(&s).is_empty(), "a tap: no trace");
    }

    #[test]
    fn no_words_heard_says_so_with_a_hint_for_the_mode() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().words = vec!["Um,", " uh."]; // all filler: nothing left after rules cleanup
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, error("Didn’t hear anything", "Hold the keys while you talk")]);
        assert_eq!(traces(&s), [traced("nothing_heard", None, &[("speech-to-text", "Um, uh."), ("rules", "")])]);
        let (s, mac, _tmp) = session("tap");
        mac.0.lock().unwrap().words = vec![];
        assert_eq!(dictate(&s, &mac, true), [Pill::Writing, error("Didn’t hear anything", "Press the keys, talk, then press them again")]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
    }

    #[test]
    fn accessibility_lost_mid_dictation_pastes_nothing() {
        let (s, mac, _tmp) = session("hold");
        s.key(true, false);
        mac.0.lock().unwrap().ax = false;
        s.key(false, false);
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.shown()[1..], [Pill::Writing, error("Needs Accessibility", pill::ALLOW)]);
        assert!(mac.0.lock().unwrap().pasted.is_empty());
        let steps = [("speech-to-text", "Hello there."), ("rules", "Hello there.")];
        assert_eq!(traces(&s), [traced("not_pasted", Some(dictation::NO_AX), &steps)], "the text is kept");
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
        // The mic failing never reached speech-to-text: no trace.
        let failed = |why| traced("failed", Some(why), &[]);
        assert_eq!(traces(&s), [failed("Couldn't load Apple Speech: broken"), failed("The model choked.")]);
    }

    #[test]
    fn ask_how_it_went_follows_a_pasted_dictation_only_when_switched_on() {
        let (s, mac, _tmp) = session("hold");
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added], "off by default");
        let v = voice::get(&s.db.lock().unwrap()).unwrap();
        voice::set(&s.db.lock().unwrap(), &voice::Voice { rate: true, ..v }).unwrap();
        let id: i64 = s.db.lock().unwrap().query_row("SELECT max(id) FROM trace", [], |r| r.get(0)).unwrap();
        dictate(&s, &mac, false);
        // Ask comes just after Added, once the trace is saved.
        mac.wait(|l| l.shown.iter().any(|p| matches!(p, Pill::Ask { .. })));
        assert_eq!(mac.shown()[1..], [Pill::Writing, Pill::Added, Pill::Ask { trace: id + 1 }]);
        mac.0.lock().unwrap().words = vec![" um"];
        assert!(!dictate(&s, &mac, false).iter().any(|p| matches!(p, Pill::Ask { .. })), "nothing pasted, nothing to rate");
    }

    #[test]
    fn switching_models_frees_the_old_one_and_loads_the_new_one_on_the_next_dictation() {
        let (s, mac, tmp) = session("hold");
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        // Whisper downloaded (a sparse file at its catalog size) and picked on Speech models.
        let dir = tmp.path().join("models/whisper-large-v3-turbo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::File::create(dir.join("ggml-large-v3-turbo.bin")).unwrap().set_len(1624555275).unwrap();
        let speech = mac.speech();
        models::select(&s.db.lock().unwrap(), "stt", "whisper-large-v3-turbo", 26, &speech).unwrap();
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        let l = mac.0.lock().unwrap();
        assert_eq!((l.loaded.as_slice(), l.freed.as_slice()), (&["apple-speech".to_string(), "whisper-large-v3-turbo".into()][..], &["apple-speech".to_string()][..]));
    }

    fn unload_after(s: &Session, secs: i64) {
        models::set_unload_after(&s.db.lock().unwrap(), secs).unwrap();
    }

    fn freed(mac: &FakeMac) -> usize {
        mac.0.lock().unwrap().freed.len()
    }

    #[test]
    fn the_model_is_freed_after_the_idle_time_unless_never() {
        // A minute of idle time is 60 ms here.
        let (s, mac, _tmp) = session_timed("hold", Duration::from_secs(300), Duration::from_millis(1));
        unload_after(&s, 60);
        dictate(&s, &mac, false);
        assert_eq!(freed(&mac), 0, "not before the time");
        mac.wait(|l| l.freed == ["apple-speech"]);

        unload_after(&s, -1);
        dictate(&s, &mac, false);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(freed(&mac), 1, "never");
    }

    #[test]
    fn right_after_use_frees_the_model_once_the_dictation_ends() {
        let (s, mac, _tmp) = session("hold");
        unload_after(&s, 0);
        dictate(&s, &mac, false);
        mac.wait(|l| l.freed == ["apple-speech"]);
    }

    #[test]
    fn the_model_is_never_freed_mid_dictation() {
        let (s, mac, _tmp) = session_timed("hold", Duration::from_secs(300), Duration::from_millis(1));
        unload_after(&s, 60);
        dictate(&s, &mac, false); // its timer would run out during the next dictation
        s.key(true, false);
        s.schedule_unload(); // the idle time changed on Speech models mid-dictation
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(freed(&mac), 0);
        s.key(false, false);
        mac.wait(|l| l.pasted.len() == 2);
        assert_eq!(mac.0.lock().unwrap().loaded.len(), 1, "same model, still loaded");
        mac.wait(|l| l.freed.len() == 1); // and freed once idle again
    }

    #[test]
    fn a_dictation_during_the_idle_time_restarts_the_wait() {
        // A minute of idle time is 300 ms here.
        let (s, mac, _tmp) = session_timed("hold", Duration::from_secs(300), Duration::from_millis(5));
        unload_after(&s, 60);
        dictate(&s, &mac, false);
        std::thread::sleep(Duration::from_millis(200));
        dictate(&s, &mac, false);
        std::thread::sleep(Duration::from_millis(200)); // the first wait has run out
        assert_eq!(freed(&mac), 0);
        mac.wait(|l| l.freed.len() == 1);
    }

    #[test]
    fn tap_mode_stops_and_pastes_on_its_own_at_the_limit() {
        let (s, mac, _tmp) = session_timed("tap", Duration::from_millis(100), Duration::from_secs(1));
        s.key(false, false);
        std::thread::sleep(Duration::from_millis(40));
        assert!(mac.0.lock().unwrap().pasted.is_empty(), "not before the limit");
        mac.wait(|l| l.shown.len() == 3);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
        assert_eq!(mac.shown()[1..], [Pill::Writing, Pill::Added]);
    }

    #[test]
    fn level_bars_stop_when_the_recording_ends() {
        let levels = |mac: &FakeMac| mac.0.lock().unwrap().levels;
        let stays = |mac: &FakeMac| {
            let n = levels(mac);
            std::thread::sleep(Duration::from_millis(150));
            assert_eq!(levels(mac), n);
        };
        let (s, mac, _tmp) = session_timed("tap", Duration::from_millis(100), Duration::from_secs(1));
        s.key(false, false);
        mac.wait(|l| l.pasted.len() == 1); // at the limit
        assert!(levels(&mac) > 0);
        stays(&mac);
        for mode in ["hold", "tap"] {
            let (s, mac, _tmp) = session(mode);
            s.key(mode == "hold", false);
            mac.wait(|l| l.levels > 0);
            s.key(false, false);
            mac.wait(|l| l.pasted.len() == 1);
            stays(&mac);
        }
    }

    /// Sets the Fast model to "m" on a local server at `base`.
    fn fast_model(s: &Session, base: &str) {
        let db = s.db.lock().unwrap();
        ai::add(&db, &*s.keys, "openai", "Local", &format!("{base}/v1"), "").unwrap();
        let id = db.query_row("SELECT id FROM provider", [], |r| r.get(0)).unwrap();
        ai::set(&db, "fast", id, "m").unwrap();
    }

    fn answer(text: &str) -> &'static str {
        Box::leak(format!(r#"{{"choices":[{{"message":{{"content":"{text}"}}}}],"usage":{{"prompt_tokens":5,"completion_tokens":2}}}}"#).into_boxed_str())
    }

    /// The saved cleanup step: its text and its settings JSON.
    fn cleanup_step(s: &Session) -> Option<(String, serde_json::Value)> {
        let q = "SELECT text, settings FROM step WHERE kind = 'cleanup'";
        let (text, json): (String, String) = s.db.lock().unwrap().query_row(q, [], |r| Ok((r.get(0)?, r.get(1)?))).ok()?;
        Some((text, serde_json::from_str(&json).unwrap()))
    }

    #[test]
    fn dictate_with_cleanup_pastes_the_cleaned_text_and_traces_the_step() {
        let (s, mac, _tmp) = session("hold");
        fast_model(&s, &crate::ai::tests::stub(vec![("/v1/chat/completions", 200, answer("Hello there!"))]));
        assert_eq!(press(&s, &mac, false, true), [Pill::Writing, Pill::Cleaning, Pill::Added]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there!"]);
        let steps = [("speech-to-text", "Hello there."), ("rules", "Hello there."), ("cleanup", "Hello there!")];
        assert_eq!(traces(&s), [traced("pasted", None, &steps)]);
        let (_, settings) = cleanup_step(&s).unwrap();
        assert_eq!((&settings["model"], &settings["cost"]["kind"], &settings["fallback"]), (&"m".into(), &"free".into(), &serde_json::Value::Null));
    }

    #[test]
    fn the_plain_shortcut_never_runs_cleanup_even_with_a_fast_model() {
        let (s, mac, _tmp) = session("hold");
        fast_model(&s, &crate::ai::tests::stub(vec![("/v1/chat/completions", 200, answer("Hello there!"))]));
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
        assert!(cleanup_step(&s).is_none());
    }

    #[test]
    fn the_menu_item_and_tap_mode_can_start_cleanup_too() {
        let (s, mac, _tmp) = session("tap");
        fast_model(&s, &crate::ai::tests::stub(vec![("/v1/chat/completions", 200, answer("Hello there!"))]));
        assert_eq!(press(&s, &mac, true, true), [Pill::Writing, Pill::Cleaning, Pill::Added]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there!"]);
    }

    #[test]
    fn cleanup_that_fails_pastes_the_rules_text_and_says_why() {
        for (name, base, reason) in [
            ("an error", crate::ai::tests::stub(vec![("/v1/chat/completions", 500, "oops")]), "AI didn’t answer"),
            ("a reply", crate::ai::tests::stub(vec![("/v1/chat/completions", 200, answer("Sure, here is Hello there."))]), "AI replied instead"),
            ("a rewrite", crate::ai::tests::stub(vec![("/v1/chat/completions", 200, answer("Greetings to you, my dear friend, how are you doing today"))]), "AI changed too much"),
        ] {
            let (s, mac, _tmp) = session("hold");
            fast_model(&s, &base);
            assert_eq!(press(&s, &mac, false, true), [Pill::Writing, Pill::Cleaning, Pill::Fallback { reason: reason.into() }], "{name}");
            assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."], "{name}");
            let (text, settings) = cleanup_step(&s).unwrap();
            assert_eq!((text.as_str(), &settings["fallback"]), ("Hello there.", &reason.into()), "{name}");
            assert_eq!(traces(&s)[0]["outcome"], "pasted");
        }
    }

    #[test]
    fn cleanup_slower_than_its_budget_pastes_the_rules_text() {
        // A server that takes the request and never answers; a "second" of 100 ms makes the 3 s budget 300 ms.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let held: Vec<_> = l.incoming().flatten().collect();
            std::thread::sleep(Duration::from_secs(10));
            drop(held);
        });
        let (s, mac, _tmp) = session_timed("hold", Duration::from_secs(voice::TAP_LIMIT_SECS), Duration::from_millis(100));
        fast_model(&s, &base);
        assert_eq!(press(&s, &mac, false, true), [Pill::Writing, Pill::Cleaning, Pill::Fallback { reason: "AI took too long".into() }]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
    }

    #[test]
    fn cleanup_without_a_fast_model_pastes_the_rules_text() {
        let (s, mac, _tmp) = session("hold");
        assert_eq!(press(&s, &mac, false, true), [Pill::Writing, Pill::Cleaning, Pill::Fallback { reason: "No Fast model".into() }]);
        assert_eq!(mac.0.lock().unwrap().pasted, ["Hello there."]);
        let (_, settings) = cleanup_step(&s).unwrap();
        assert_eq!((&settings["model"], settings["detail"].as_str()), (&serde_json::Value::Null, Some("Fast model: Not set")));
    }

    fn sounds(mac: &FakeMac) -> Vec<Sound> {
        std::mem::take(&mut mac.0.lock().unwrap().sounds)
    }

    #[test]
    fn a_sound_plays_at_start_and_stop_with_both_shortcuts_in_both_modes() {
        for mode in ["hold", "tap"] {
            let (s, mac, _tmp) = session(mode);
            for clean in [false, true] {
                press(&s, &mac, mode == "tap", clean);
                assert_eq!(sounds(&mac), [Sound::Start, Sound::Stop], "{mode}, clean {clean}");
            }
        }
    }

    #[test]
    fn a_recording_too_short_to_keep_still_plays_the_stop_sound() {
        let (s, mac, _tmp) = session("hold");
        mac.0.lock().unwrap().audio = Some(vec![0.1; 10]);
        assert_eq!(dictate(&s, &mac, false), [Pill::Idle]);
        assert_eq!(sounds(&mac), [Sound::Start, Sound::Stop]);
    }

    #[test]
    fn with_sounds_off_nothing_plays() {
        let (s, mac, _tmp) = session("tap");
        let db = s.db.lock().unwrap();
        voice::set(&db, &voice::Voice { sounds: false, ..voice::get(&db).unwrap() }).unwrap();
        drop(db);
        dictate(&s, &mac, true);
        assert!(sounds(&mac).is_empty());
    }

    #[test]
    fn esc_while_recording_cancels_with_the_stop_sound_and_pastes_nothing() {
        for mode in ["hold", "tap"] {
            let (s, mac, _tmp) = session(mode);
            s.key(mode == "hold", false);
            assert_eq!(mac.0.lock().unwrap().esc, ["Alt+Escape", "Escape"], "caught only while recording");
            s.cancel();
            mac.wait(|l| l.shown.last() == Some(&Pill::Idle));
            assert!(mac.0.lock().unwrap().esc.is_empty());
            assert_eq!(sounds(&mac), [Sound::Start, Sound::Stop]);
            s.key(false, false); // hold: letting go after Esc; tap: the next tap starts afresh
            if mode == "tap" {
                s.cancel();
            }
            std::thread::sleep(Duration::from_millis(50));
            assert!(mac.0.lock().unwrap().pasted.is_empty(), "{mode}");
            assert!(traces(&s).is_empty());
        }
    }

    #[test]
    fn a_finished_dictation_lets_go_of_esc() {
        let (s, mac, _tmp) = session("hold");
        dictate(&s, &mac, false);
        assert!(mac.0.lock().unwrap().esc.is_empty());
        s.cancel(); // nothing recording: nothing happens
        assert_eq!(sounds(&mac), [Sound::Start, Sound::Stop]);
    }

    #[test]
    fn esc_is_caught_bare_and_with_each_shortcuts_modifiers_but_never_as_a_shortcut() {
        let v = |key: &str, clean: Option<&str>, key2: Option<&str>| voice::Voice {
            on: true, key: key.into(), clean: clean.map(Into::into), key2: key2.map(Into::into), clean2: None,
            mode: "hold".into(), paste: "clipboard".into(), rate: false, sounds: true,
        };
        assert_eq!(esc_keys(&v("Alt+Space", Some("Cmd+Shift+D"), None)), ["Alt+Escape", "Cmd+Shift+Escape", "Escape"]);
        assert_eq!(esc_keys(&v(voice::FN, None, None)), ["Escape"]);
        assert_eq!(esc_keys(&v("Alt+Space", None, Some("Alt+Escape"))), ["Escape"]);
    }

    /// Whisper downloaded: a sparse file at its catalog size.
    fn whisper(tmp: &tempfile::TempDir) {
        let dir = tmp.path().join("models/whisper-large-v3-turbo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::File::create(dir.join("ggml-large-v3-turbo.bin")).unwrap().set_len(1624555275).unwrap();
    }

    #[test]
    fn meetings_and_dictation_share_one_loaded_model_while_they_pick_the_same() {
        let (s, mac, _tmp) = session("hold");
        crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap();
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap();
        assert_eq!(mac.0.lock().unwrap().loaded, ["apple-speech"], "loaded once, for both");
    }

    #[test]
    fn meetings_load_their_own_model_and_dictation_keeps_its_own_then_both_idle_out() {
        let (s, mac, tmp) = session_timed("hold", Duration::from_secs(300), Duration::from_millis(1));
        whisper(&tmp);
        models::select(&s.db.lock().unwrap(), models::MEETING_STT, "whisper-large-v3-turbo", 26, &mac.speech()).unwrap();
        unload_after(&s, 60);
        crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap();
        assert_eq!(dictate(&s, &mac, false), [Pill::Writing, Pill::Added]);
        crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap();
        assert_eq!(mac.0.lock().unwrap().loaded, ["whisper-large-v3-turbo", "apple-speech"], "each loaded once, neither swapped out");
        assert_eq!(freed(&mac), 0);
        s.schedule_unload(); // as after a meeting is written up
        mac.wait(|l| l.freed.len() == 2);
    }

    #[test]
    fn a_meeting_model_not_downloaded_says_where_to_get_it() {
        let (s, mac, _tmp) = session("hold");
        models::select(&s.db.lock().unwrap(), models::MEETING_STT, "whisper-large-v3-turbo", 26, &mac.speech()).unwrap();
        let err = crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap_err();
        assert_eq!(err, "Download Whisper large-v3-turbo in Settings › Meeting notes");
    }

    #[test]
    fn meeting_notes_play_no_sounds() {
        let (s, mac, _tmp) = session("hold");
        crate::meeting::Stt::transcribe(&*s, &[0.1; 16_000]).unwrap();
        assert!(sounds(&mac).is_empty());
    }
}
