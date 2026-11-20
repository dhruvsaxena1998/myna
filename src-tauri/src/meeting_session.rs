//! The Meeting session: one meeting recording from shortcut to write-up, and what the add-on reports during it.
//! It owns the app's `Meetings` and reaches the Mac only through `World`, so tests drive it with a fake.

use crate::addon::{AddonEvent, MuteState, Site};
use crate::meeting::{self, Meetings, Stt, Voices};
use crate::stt::{self, Transcriber};
use crate::permissions::Mic;
use crate::{notify, now_ms, permissions, pill};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Everything the session needs from the Mac. It is also meeting notes' speech and voices.
pub trait World: Stt + Voices + 'static {
    fn mic(&self) -> Mic;
    /// Shows macOS's mic pop-up and waits for the answer.
    fn ask_mic(&self);
    /// System Audio is denied, so Myna can't hear the other people.
    fn audio_denied(&self) -> bool;
    fn notify(&self, title: &str, body: &str);
    /// The recording pill with its start time, mic word and the words for it, or hidden when `started` is none.
    fn pill(&self, started: Option<i64>, mic: Option<&str>, words: Option<&str>);
    /// A warning over the pill.
    fn pill_warning(&self, title: &str, hint: &str);
    /// Moves the pill to "bottom", "top-right", ... where the user set it.
    fn place(&self, place: String);
    /// The menu item and the tray title for a running meeting.
    fn menu_running(&self, clock: &str, note: &str, paused: bool);
    /// The menu bar icon while recording, and the hidden recording pill: their wave follows the mic's loudness, 0 to 1.
    fn menu_wave(&self, level: f32);
    /// The menu item, its note and the tray title for no meeting.
    fn menu_idle(&self);
    /// `meeting-running`: the live card, or none.
    fn running(&self, card: Option<serde_json::Value>);
    /// `meeting-notes`: how a write-up is going.
    fn notes(&self, id: i64, state: &str, progress: f32, error: Option<&str>);
    /// Tells the browser add-on to stop capturing the tab, if it's there.
    fn stop_tab(&self);
    /// Refreshes "Recent" in the menu.
    fn recent(&self);
    /// The speech model is idle now: it may be freed after the idle time.
    fn idle(&self);
    /// Meeting `id` is transcribed: its follow-up (cleanup, title, Summary) runs now, in the background.
    fn follow_up(&self, id: i64);
    /// `meeting-echo`: checking meeting `id`, recorded before echo handling, left lines out.
    fn echo_checked(&self, id: i64);
    /// Where AI providers' keys are, for meeting speech-to-text on an Endpoint.
    fn keys(&self) -> &dyn crate::ai::Keys;
}

pub struct MeetingSession {
    world: Box<dyn World>,
    db: Arc<Mutex<Connection>>,
    meetings: Meetings,
    /// The menu clock refreshes this often while a meeting runs.
    tick: Duration,
}

impl MeetingSession {
    pub fn new(world: Box<dyn World>, db: Arc<Mutex<Connection>>, meetings: Meetings, tick: Duration) -> Arc<Self> {
        Arc::new(MeetingSession { world, db, meetings, tick })
    }

    /// The shortcut, the menu item or the Meetings list: start, or stop if recording.
    pub fn toggle(self: &Arc<Self>) {
        if self.meetings.running().is_some() {
            self.stop();
        } else {
            self.start();
        }
    }

    pub fn start(self: &Arc<Self>) {
        if !meeting::get(&self.db.lock().unwrap()).is_ok_and(|n| n.on) {
            return;
        }
        if self.world.mic() == Mic::Ask {
            self.world.ask_mic(); // macOS's pop-up, answered before this goes on
        }
        match self.world.mic() {
            Mic::Allowed => {}
            Mic::Ask => return,
            Mic::Denied => return self.world.notify("Needs the Microphone", pill::ALLOW),
        }
        // The pill first: opening the mic and the call's audio can take seconds.
        if let Ok(look) = pill::look(&self.db.lock().unwrap()) {
            self.world.place(look.place);
        }
        let now = now_ms();
        self.world.pill(Some(now), mic_word(self.meetings.muted()), Some(&self.mic_words()));
        let id = match self.meetings.start(now) {
            Ok(id) => id,
            Err(e) => {
                self.world.pill(None, None, None);
                return self.world.notify("Couldn’t start meeting notes", &e);
            }
        };
        self.show_running();
        let (this, that) = (self.clone(), self.clone());
        std::thread::spawn(move || this.clock(id));
        std::thread::spawn(move || that.wave(id));
    }

    /// Until meeting `id` ends: the menu bar icon's wave, about 8 times a second.
    fn wave(&self, id: i64) {
        while self.meetings.running().is_some_and(|(i, _)| i == id) {
            self.world.menu_wave(self.meetings.level());
            std::thread::sleep(Duration::from_millis(125));
        }
    }

    /// Start notes on a detected call: starts, and names the meeting after the call's app.
    pub fn start_from(self: &Arc<Self>, source: &str) {
        if self.meetings.running().is_none() {
            self.start();
        }
        let Some((id, _)) = self.meetings.running() else {
            // The offer is gone by now, and a notification may not show, so the pill says it.
            let hint = if self.world.mic() == Mic::Allowed { "Try the Recording shortcut, or Start in the menu." } else { "Allow the Microphone in Settings › Meeting notes." };
            return self.world.pill_warning("Couldn’t start meeting notes", hint);
        };
        let _ = self.db.lock().unwrap().execute("UPDATE meeting SET source_app = ?1 WHERE id = ?2", (source, id));
        self.show_running();
    }

    pub fn recording(&self) -> bool {
        self.meetings.running().is_some()
    }

    /// Until meeting `id` ends: the menu clock, and the warning, said once per meeting over the pill.
    fn clock(&self, id: i64) {
        let mut warned = false;
        let mut warn = || {
            if !std::mem::replace(&mut warned, true) {
                self.world.pill_warning("Myna can’t hear the other people on the call", "Your mic is still recording. Allow System Audio in Settings › Meeting notes.");
            }
        };
        if self.world.audio_denied() {
            warn();
        }
        loop {
            std::thread::sleep(self.tick);
            let running = self.meetings.running();
            if running.is_none_or(|(i, _)| i != id) {
                break;
            }
            self.show_menu(running.unwrap().1);
            if self.meetings.deaf() {
                warn();
            }
        }
    }

    /// Stops the recording, keeps it, and starts writing it up.
    pub fn stop(self: &Arc<Self>) {
        self.world.stop_tab();
        match self.meetings.stop() {
            Ok(Some(id)) => self.write_up(id, None, true),
            Ok(None) => {}
            Err(e) => self.world.notify("Couldn’t save the recording", &e),
        }
        self.world.menu_idle();
        self.world.running(None);
        self.world.pill(None, None, None);
    }

    /// Try again on a meeting whose notes failed.
    pub fn retry(self: &Arc<Self>, id: i64) -> Result<(), String> {
        if !self.meetings.failed(id) {
            return Err("That meeting isn't waiting to be tried again.".into());
        }
        self.write_up(id, None, true);
        Ok(())
    }

    /// Transcribe locally, on a meeting an Endpoint left incomplete: keeps the chunks it did.
    pub fn retry_local(self: &Arc<Self>, id: i64) -> Result<(), String> {
        if !self.meetings.failed(id) {
            return Err("That meeting isn't waiting to be tried again.".into());
        }
        self.write_up(id, Some(Transcriber::Local(None)), true);
        Ok(())
    }

    /// Transcribe again, on a written-up meeting: through `pick` this once, from scratch; the setting for new meetings
    /// stays. The lines are replaced, so speaker names and cleanup are redone; Summary versions are kept.
    pub fn again(self: &Arc<Self>, id: i64, pick: &stt::Pick) -> Result<(), String> {
        self.meetings.settled(id)?;
        let t = stt::resolve(&self.db.lock().unwrap(), self.world.keys(), Some(pick))?;
        self.write_up(id, Some(t), false);
        Ok(())
    }

    /// Play on a meeting's page: where its mixed audio is.
    pub fn audio(&self, id: i64) -> Result<std::path::PathBuf, String> {
        self.meetings.audio(id)
    }

    /// Rename on the Meetings list; "Recent" in the menu shows the new name.
    pub fn rename(&self, id: i64, title: &str) -> Result<(), String> {
        self.meetings.rename(id, title)?;
        self.world.recent();
        Ok(())
    }

    /// Delete (one meeting) and Clear all (`id` none) on the Meetings list.
    pub fn delete(&self, id: Option<i64>) -> Result<(), String> {
        self.meetings.delete(id)?;
        self.world.recent();
        Ok(())
    }

    /// At launch: finishes and writes up the meetings the app died or quit in the middle of, then checks the ones
    /// written up before echo handling, newest first, one at a time (a write-up goes first).
    pub fn resume(self: &Arc<Self>) {
        for id in self.meetings.unfinished() {
            self.write_up(id, None, true);
        }
        let this = self.clone();
        std::thread::spawn(move || this.check_echo());
    }

    fn check_echo(&self) {
        for id in self.meetings.unchecked() {
            match self.meetings.check_echo(id) {
                Ok(true) => self.world.echo_checked(id),
                Ok(false) => {}
                Err(e) => eprintln!("meeting {id}: {e}"),
            }
        }
    }

    /// The live card: `{ id, started, source, mic, words }`, or none when not recording.
    pub fn running(&self) -> Option<serde_json::Value> {
        let (id, started) = self.meetings.running()?;
        let source: Option<String> = self.db.lock().unwrap().query_row("SELECT source_app FROM meeting WHERE id = ?1", [id], |r| r.get(0)).ok().flatten();
        Some(serde_json::json!({ "id": id, "started": started, "source": source, "mic": self.mic_word(), "words": self.mic_words() }))
    }

    /// The call page's mute button changed: the timeline, then the menu, pill and live card.
    pub fn mute(&self, state: MuteState, site: Site, at: i64) {
        self.meetings.mute(state, site, at, now_ms());
        self.show_running();
    }

    /// The add-on dropped: Myna can't see the button anymore.
    pub fn dropped(&self) {
        self.mute(MuteState::Unknown, self.meetings.site(), now_ms());
    }

    fn mic_words(&self) -> String {
        if self.meetings.paused() { "Paused".into() } else { mic_words(self.meetings.muted(), self.meetings.site()) }
    }

    /// The mic for the pill and the live card: "paused" from the pill, else "live" or "muted" while the add-on mirrors it.
    fn mic_word(&self) -> Option<&'static str> {
        if self.meetings.paused() { Some("paused") } else { mic_word(self.meetings.muted()) }
    }

    /// Pause or Resume in the menu bar menu: whichever it isn't.
    pub fn toggle_pause(&self) -> Result<(), String> {
        self.pause(!self.meetings.paused())
    }

    /// Pause and Resume on the recording pill, the menu and the live card: they all say so.
    pub fn pause(&self, on: bool) -> Result<(), String> {
        if !self.meetings.pause(on) {
            return Err("Nothing is recording.".into());
        }
        self.show_running();
        Ok(())
    }

    /// What the add-on reports. Mute and a drop go to the timeline, then the menu, pill and live card; captions are kept
    /// on the meeting for naming its voices; the call tab's sound starts a meeting if none is recording.
    pub fn addon(self: &Arc<Self>, e: AddonEvent) {
        use AddonEvent as E;
        match e {
            E::Connected | E::SelectorsChanged => {}
            E::Disconnected => self.dropped(),
            E::Mute { state, at, site } => self.mute(state, site, at),
            E::Caption { name, at, text } => self.meetings.caption(&name, at, text.as_deref(), now_ms()),
            E::CaptionsUnavailable => self.meetings.captions_unavailable(),
            E::TabAudio { at, ts, samples } => self.meetings.tab_audio(at, ts, &samples),
            E::TabStop => self.meetings.tab_stop(),
            E::TabStart { site } => {
                if self.meetings.running().is_none() {
                    self.start();
                }
                let source = if site == Site::Other { "Chrome tab".into() } else { format!("{} (Chrome tab)", site.name()) };
                if self.meetings.tab_start(&source) {
                    self.show_running();
                }
            }
        }
    }

    /// Makes the menu, the live card and the pill agree with the meeting that's recording; nothing when none is.
    fn show_running(&self) {
        let Some((_, started)) = self.meetings.running() else { return };
        self.show_menu(started);
        self.world.running(self.running());
        self.world.pill(Some(started), self.mic_word(), Some(&self.mic_words()));
    }

    fn show_menu(&self, started: i64) {
        // The note says how the mic is while recording; the idle menu puts the usual words back.
        self.world.menu_running(&meeting::clock(now_ms() - started), &self.mic_words(), self.meetings.paused());
    }

    /// Writes up meeting `id` on its own thread, saying how it goes with `meeting-notes`: "transcribing" with progress
    /// 0 to 1, then "done" or "failed" with the reason. `with` none is meeting speech-to-text's setting, resolved on
    /// that thread (it reads the Keychain). `keep_done`: chunks an Endpoint already did are kept, not redone.
    fn write_up(self: &Arc<Self>, id: i64, with: Option<Transcriber>, keep_done: bool) {
        let this = self.clone();
        std::thread::spawn(move || {
            let w = &*this.world;
            let with = with.unwrap_or_else(|| stt::resolve(&this.db.lock().unwrap(), w.keys(), None).unwrap_or_else(Transcriber::Unavailable));
            if let Transcriber::Endpoint(e) = &with {
                // Before "transcribing" is said, so the meeting page shows where its audio goes while it runs.
                let json = serde_json::json!({ "provider": e.provider, "model": e.model, "off": e.off }).to_string();
                let _ = this.db.lock().unwrap().execute("UPDATE meeting SET stt = ?1 WHERE id = ?2", (json, id));
            }
            w.notes(id, "transcribing", 0.0, None);
            let done = this.meetings.transcribe(id, w, w, &with, keep_done, &mut |p| w.notes(id, "transcribing", p, None));
            w.idle(); // the model idles out like after a dictation
            match done {
                Ok(()) => {
                    w.notes(id, "done", 1.0, None);
                    w.recent(); // before cleanup, which can take minutes on a local model
                    w.follow_up(id);
                }
                Err(e) => {
                    w.notes(id, "failed", 0.0, Some(&e));
                    w.recent();
                }
            }
            // Play made from the 48 kHz copy while it's still whole, then the copy kept as AAC.
            if let Err(e) = this.meetings.audio(id).and_then(|_| this.meetings.compact(id)) {
                eprintln!("meeting {id}: {e}");
            }
        });
    }
}

/// `mic`: "live" or "muted" while the add-on mirrors the mute button, else null.
fn mic_word(muted: Option<bool>) -> Option<&'static str> {
    muted.map(|m| if m { "muted" } else { "live" })
}

/// The words for the mic while recording, on the menu, the pill and the live card.
fn mic_words(muted: Option<bool>, site: Site) -> String {
    match muted {
        Some(true) => format!("Muted · mirrored from {}", site.name()),
        Some(false) => format!("Mic live · mirrored from {}", site.name()),
        None => "Mic live".into(),
    }
}

/// The real Mac: the app's permissions, notifications, pill, menu and windows.
pub struct Mac(pub AppHandle);

/// Whether the menu bar icon is in its recording look; only touched on the main thread.
static RECORDING: AtomicBool = AtomicBool::new(false);
/// The wave frame in the menu bar (dark * 5 + bar), so an unchanged level isn't redrawn; NO_FRAME once the icon is put back.
static SHOWN: AtomicUsize = AtomicUsize::new(NO_FRAME);
const NO_FRAME: usize = usize::MAX;

/// Whether the menu bar the icon sits in is dark (it follows the wallpaper, not just the system appearance).
fn menu_bar_dark(item: Option<objc2::rc::Retained<objc2_app_kit::NSStatusItem>>) -> bool {
    use objc2_app_kit::NSAppearanceCustomization;
    let Some(mtm) = objc2::MainThreadMarker::new() else { return false };
    item.and_then(|i| i.button(mtm)).is_some_and(|b| b.effectiveAppearance().name().to_string().contains("Dark"))
}

impl Stt for Mac {
    fn transcribe(&self, audio: &[f32], model: Option<&str>) -> Result<Vec<crate::runtime::Segment>, String> {
        self.0.state::<Arc<crate::session::Session>>().transcribe(audio, model)
    }
}

impl Voices for Mac {
    fn diarize(&self, audio: &[f32]) -> Result<Vec<crate::runtime::Turn>, String> {
        self.0.state::<Arc<crate::session::Session>>().diarize(audio)
    }
}

impl World for Mac {
    fn mic(&self) -> Mic {
        permissions::mic()
    }
    fn ask_mic(&self) {
        permissions::ask_mic()
    }
    fn audio_denied(&self) -> bool {
        permissions::audio() == permissions::Audio::Denied
    }
    fn notify(&self, title: &str, body: &str) {
        notify::send(title, body)
    }
    fn pill(&self, started: Option<i64>, mic: Option<&str>, words: Option<&str>) {
        pill::meeting(&self.0, started, mic, words)
    }
    fn pill_warning(&self, title: &str, hint: &str) {
        pill::error(&self.0, title, hint)
    }
    fn place(&self, place: String) {
        pill::place(&self.0, place)
    }
    fn menu_running(&self, clock: &str, note: &str, paused: bool) {
        if let Some(m) = self.0.try_state::<crate::MeetItem>() {
            let _ = m.item.set_text(format!("Stop recording  {clock}"));
            let _ = m.note.set_text(note);
            let _ = m.pause.set_text(if paused { "Resume recording" } else { "Pause recording" });
            // Right under Stop recording, once.
            if m.menu.get("meeting-pause").is_none() {
                let at = m.menu.items().ok().and_then(|items| items.iter().position(|i| i.id() == "meeting"));
                if let Some(at) = at {
                    let _ = m.menu.insert(&m.pause, at + 1);
                }
            }
        }
        // The icon says it's recording (menu_wave); the title is just the clock.
        let (app, clock) = (self.0.clone(), clock.to_string());
        let _ = self.0.run_on_main_thread(move || {
            RECORDING.store(true, Ordering::Relaxed);
            if let Some(t) = app.tray_by_id("myna") {
                let _ = t.set_title(Some(clock));
            }
        });
    }
    fn menu_wave(&self, level: f32) {
        let _ = self.0.emit_to("pill", "meeting-level", level);
        // A coloured icon can't be a template, so there's a frame for a light menu bar and one for a dark one.
        const FRAMES: [[&[u8]; 5]; 2] = [
            [include_bytes!("../icons/tray-rec/light-0.png"), include_bytes!("../icons/tray-rec/light-1.png"), include_bytes!("../icons/tray-rec/light-2.png"), include_bytes!("../icons/tray-rec/light-3.png"), include_bytes!("../icons/tray-rec/light-4.png")],
            [include_bytes!("../icons/tray-rec/dark-0.png"), include_bytes!("../icons/tray-rec/dark-1.png"), include_bytes!("../icons/tray-rec/dark-2.png"), include_bytes!("../icons/tray-rec/dark-3.png"), include_bytes!("../icons/tray-rec/dark-4.png")],
        ];
        // Decoded once: decoding a PNG 8 times a second on the main thread made the app stutter while recording.
        static DECODED: std::sync::OnceLock<Vec<Option<tauri::image::Image<'static>>>> = std::sync::OnceLock::new();
        let app = self.0.clone();
        // On the main thread, like menu_idle, so a frame can never land after the icon was put back.
        let _ = self.0.run_on_main_thread(move || {
            let Some(t) = app.tray_by_id("myna").filter(|_| RECORDING.load(Ordering::Relaxed)) else { return };
            let dark = t.with_inner_tray_icon(|i| menu_bar_dark(i.ns_status_item())).unwrap_or(false);
            let n = dark as usize * 5 + ((level * 4.0).round() as usize).min(4);
            if SHOWN.swap(n, Ordering::Relaxed) == n {
                return; // same frame as now: no redraw
            }
            let frames = DECODED.get_or_init(|| FRAMES.iter().flatten().map(|b| tauri::image::Image::from_bytes(b).ok()).collect());
            if let Some(icon) = frames[n].clone() {
                let _ = t.set_icon_with_as_template(Some(icon), false);
            }
        });
    }
    fn menu_idle(&self) {
        if let Some(m) = self.0.try_state::<crate::MeetItem>() {
            let _ = m.item.set_text("Start meeting notes");
            let _ = m.menu.remove(&m.pause);
            let _ = crate::sync_meeting(&self.0); // puts the usual note back
        }
        let app = self.0.clone();
        let _ = self.0.run_on_main_thread(move || {
            RECORDING.store(false, Ordering::Relaxed);
            SHOWN.store(NO_FRAME, Ordering::Relaxed);
            if let Some(t) = app.tray_by_id("myna") {
                let _ = t.set_title(None::<String>);
                if let Ok(icon) = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png")) {
                    let _ = t.set_icon_with_as_template(Some(icon), true);
                }
            }
        });
    }
    fn running(&self, card: Option<serde_json::Value>) {
        let _ = self.0.emit("meeting-running", card);
    }
    fn notes(&self, id: i64, state: &str, progress: f32, error: Option<&str>) {
        let _ = self.0.emit("meeting-notes", serde_json::json!({ "id": id, "state": state, "progress": progress, "error": error }));
    }
    fn stop_tab(&self) {
        if let Some(a) = self.0.try_state::<crate::addon::Addon>() {
            a.stop_tab();
        }
    }
    fn recent(&self) {
        crate::sync_recent(&self.0)
    }
    fn idle(&self) {
        self.0.state::<Arc<crate::session::Session>>().schedule_unload()
    }
    fn follow_up(&self, id: i64) {
        let app = self.0.clone();
        std::thread::spawn(move || app.state::<crate::follow_up::FollowUp>().run(id));
    }
    fn echo_checked(&self, id: i64) {
        let _ = self.0.emit("meeting-echo", id);
    }
    fn keys(&self) -> &dyn crate::ai::Keys {
        &*self.0.state::<crate::Data>().inner().keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::tests::{Call, Fake};
    use crate::runtime::{Segment, Turn};

    /// What the fake saw, in order.
    #[derive(Debug, Clone, PartialEq)]
    enum Saw {
        AskMic,
        Notify(String, String),
        Pill(Option<i64>, Option<String>, Option<String>),
        Warning(String),
        Place(String),
        MenuRunning(String),
        MenuIdle,
        Running(Option<serde_json::Value>),
        Notes(i64, String, f32, Option<String>),
        StopTab,
        Recent,
        Idle,
        FollowUp(i64),
        Echo(i64),
        Model(String),
    }

    struct Scripted {
        mic: Mutex<Mic>,
        /// What the mic is once macOS's pop-up is answered.
        on_ask: Mic,
        denied: bool,
        /// While set, the speech model fails.
        fail: Arc<std::sync::atomic::AtomicBool>,
        saw: Arc<Mutex<Vec<Saw>>>,
        keys: crate::ai::tests::Memory,
    }

    impl Scripted {
        fn log(&self, s: Saw) {
            self.saw.lock().unwrap().push(s);
        }
    }

    impl Stt for Scripted {
        fn transcribe(&self, _: &[f32], model: Option<&str>) -> Result<Vec<Segment>, String> {
            if let Some(m) = model {
                self.log(Saw::Model(m.into()));
            }
            if self.fail.load(std::sync::atomic::Ordering::SeqCst) { Err("The model fell over.".into()) } else { Ok(vec![]) }
        }
    }
    impl Voices for Scripted {
        fn diarize(&self, _: &[f32]) -> Result<Vec<Turn>, String> {
            Ok(vec![])
        }
    }
    impl World for Scripted {
        fn mic(&self) -> Mic {
            *self.mic.lock().unwrap()
        }
        fn ask_mic(&self) {
            *self.mic.lock().unwrap() = self.on_ask;
            self.log(Saw::AskMic)
        }
        fn audio_denied(&self) -> bool {
            self.denied
        }
        fn notify(&self, title: &str, body: &str) {
            self.log(Saw::Notify(title.into(), body.into()))
        }
        fn pill(&self, started: Option<i64>, mic: Option<&str>, words: Option<&str>) {
            self.log(Saw::Pill(started, mic.map(Into::into), words.map(Into::into)))
        }
        fn pill_warning(&self, title: &str, _: &str) {
            self.log(Saw::Warning(title.into()))
        }
        fn place(&self, place: String) {
            self.log(Saw::Place(place))
        }
        fn menu_running(&self, clock: &str, note: &str, _: bool) {
            self.log(Saw::MenuRunning(format!("{clock} | {note}")))
        }
        fn menu_wave(&self, _: f32) {}
        fn menu_idle(&self) {
            self.log(Saw::MenuIdle)
        }
        fn running(&self, card: Option<serde_json::Value>) {
            self.log(Saw::Running(card))
        }
        fn notes(&self, id: i64, state: &str, progress: f32, error: Option<&str>) {
            self.log(Saw::Notes(id, state.into(), progress, error.map(Into::into)))
        }
        fn stop_tab(&self) {
            self.log(Saw::StopTab)
        }
        fn recent(&self) {
            self.log(Saw::Recent)
        }
        fn idle(&self) {
            self.log(Saw::Idle)
        }
        fn follow_up(&self, id: i64) {
            self.log(Saw::FollowUp(id))
        }
        fn echo_checked(&self, id: i64) {
            self.log(Saw::Echo(id))
        }
        fn keys(&self) -> &dyn crate::ai::Keys {
            &self.keys
        }
    }

    fn session(notes_on: bool, mic: Mic, denied: bool) -> (Arc<MeetingSession>, Arc<Mutex<Vec<Saw>>>, tempfile::TempDir) {
        let (s, saw, _, tmp) = rig(notes_on, mic, denied, Call(vec![]), Duration::from_secs(3600));
        (s, saw, tmp)
    }

    type Rig = (Arc<MeetingSession>, Arc<Mutex<Vec<Saw>>>, Arc<std::sync::atomic::AtomicBool>, tempfile::TempDir);

    /// A session with a call and a clock tick of its choosing; the flag makes the speech model fail.
    fn rig(notes_on: bool, mic: Mic, denied: bool, them: impl crate::meeting::Them + 'static, tick: Duration) -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        meeting::set(&db.lock().unwrap(), &meeting::Notes { on: notes_on, key: meeting::KEY.into(), keep_teams: false, keep_meet: false, offer_calls: true, stop_on_end: true, show_pill: true }).unwrap();
        let meetings = Meetings::new(db.clone(), tmp.path().into(), Box::new(Fake(crate::runtime::RATE as usize)), Box::new(them));
        let saw = Arc::new(Mutex::new(vec![]));
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let world = Scripted { mic: Mutex::new(mic), on_ask: mic, denied, fail: fail.clone(), saw: saw.clone(), keys: Default::default() };
        (MeetingSession::new(Box::new(world), db, meetings, tick), saw, fail, tmp)
    }

    fn saw(s: &Arc<Mutex<Vec<Saw>>>) -> Vec<Saw> {
        s.lock().unwrap().clone()
    }

    /// Waits for the write-up's last call, so nothing from it lands after the test.
    fn settled(s: &Arc<Mutex<Vec<Saw>>>) -> Vec<Saw> {
        for _ in 0..400 {
            if saw(s).contains(&Saw::Recent) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        saw(s)
    }

    /// What the write-up thread said and the menu clock, left out: they land whenever.
    fn without_write_up(v: Vec<Saw>) -> Vec<Saw> {
        v.into_iter().filter(|s| !matches!(s, Saw::Notes(..) | Saw::Recent | Saw::Idle | Saw::MenuRunning(_) | Saw::FollowUp(_))).collect()
    }

    #[test]
    fn notes_off_start_does_nothing() {
        let (s, saw_, _tmp) = session(false, Mic::Allowed, false);
        s.start();
        assert_eq!((saw(&saw_), s.running()), (vec![], None));
    }

    #[test]
    fn mic_not_asked_asks_and_does_not_record() {
        let (s, saw_, _tmp) = session(true, Mic::Ask, false);
        s.start();
        assert_eq!((saw(&saw_), s.running()), (vec![Saw::AskMic], None));
    }

    #[test]
    fn mic_denied_says_so_and_does_not_record() {
        let (s, saw_, _tmp) = session(true, Mic::Denied, false);
        s.start();
        assert_eq!(saw(&saw_), vec![Saw::Notify("Needs the Microphone".into(), pill::ALLOW.into())]);
        assert_eq!(s.running(), None);
    }

    #[test]
    fn start_places_and_shows_the_pill_before_the_mic_opens_then_the_menu_card_and_pill() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        s.start();
        let card = s.running().unwrap();
        let (id, started) = (card["id"].as_i64().unwrap(), card["started"].as_i64().unwrap());
        assert_eq!((card["source"].clone(), card["mic"].clone()), (serde_json::Value::Null, serde_json::Value::Null));
        let seen = saw(&saw_);
        assert_eq!(seen[..2], [Saw::Place("bottom".into()), Saw::Pill(Some(started), None, Some("Mic live".into()))]);
        assert!(matches!(&seen[2], Saw::MenuRunning(m) if m.ends_with("| Mic live")));
        assert_eq!(seen[3..5], [Saw::Running(Some(serde_json::json!({ "id": id, "started": started, "source": null, "mic": null, "words": "Mic live" }))), Saw::Pill(Some(started), None, Some("Mic live".into()))]);
        s.stop();
        settled(&saw_);
    }

    #[test]
    fn pause_and_resume_say_so_on_the_pill_menu_and_live_card() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        assert!(s.pause(true).is_err(), "nothing is recording");
        s.start();
        let started = s.running().unwrap()["started"].as_i64().unwrap();
        s.pause(true).unwrap();
        assert_eq!((s.running().unwrap()["mic"].clone(), s.running().unwrap()["words"].clone()), (serde_json::json!("paused"), serde_json::json!("Paused")));
        let seen = saw(&saw_);
        assert!(seen.iter().any(|x| matches!(x, Saw::MenuRunning(m) if m.ends_with("| Paused"))), "{seen:?}");
        assert_eq!(seen.last(), Some(&Saw::Pill(Some(started), Some("paused".into()), Some("Paused".into()))));
        s.pause(false).unwrap();
        assert_eq!(saw(&saw_).last(), Some(&Saw::Pill(Some(started), None, Some("Mic live".into()))));
        s.stop();
        settled(&saw_);
    }

    #[test]
    fn toggle_starts_then_stops_and_stop_clears_everything() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        s.toggle();
        assert!(s.running().is_some());
        s.toggle();
        assert_eq!(s.running(), None);
        let seen = without_write_up(settled(&saw_));
        assert_eq!(seen.len(), 8, "{seen:?}");
        assert_eq!(seen[4..], [Saw::StopTab, Saw::MenuIdle, Saw::Running(None), Saw::Pill(None, None, None)]);
        let seen = settled(&saw_);
        assert!(matches!(seen.iter().find(|s| matches!(s, Saw::Notes(..))), Some(Saw::Notes(_, state, ..)) if state == "transcribing"));
    }

    #[test]
    fn stop_with_nothing_recording_still_sends_the_idle_state() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        s.stop();
        assert_eq!(saw(&saw_), vec![Saw::StopTab, Saw::MenuIdle, Saw::Running(None), Saw::Pill(None, None, None)]);
    }

    const TICK: Duration = Duration::from_millis(5);

    fn count(v: &[Saw], f: impl Fn(&Saw) -> bool) -> usize {
        v.iter().filter(|s| f(s)).count()
    }
    fn menus(v: &[Saw]) -> usize {
        count(v, |s| matches!(s, Saw::MenuRunning(_)))
    }
    fn warnings(v: &[Saw]) -> usize {
        count(v, |s| matches!(s, Saw::Warning(_)))
    }
    fn recents(v: &[Saw]) -> usize {
        count(v, |s| *s == Saw::Recent)
    }

    /// Waits for what the fake has recorded to satisfy `done`.
    fn until(s: &Arc<Mutex<Vec<Saw>>>, done: impl Fn(&[Saw]) -> bool) -> Vec<Saw> {
        for _ in 0..3000 {
            let v = saw(s);
            if done(&v) {
                return v;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("timed out: {:?}", saw(s));
    }

    fn id_of(s: &MeetingSession) -> i64 {
        s.running().unwrap()["id"].as_i64().unwrap()
    }

    /// What the write-up said, in order.
    fn write_up_of(v: &[Saw]) -> Vec<Saw> {
        v.iter().filter(|s| matches!(s, Saw::Notes(..) | Saw::Idle | Saw::Recent | Saw::FollowUp(_))).cloned().collect()
    }

    /// A call whose listening fails when the test says so.
    struct Later(Arc<Mutex<Option<crate::meeting::Ear>>>);
    impl crate::meeting::Them for Later {
        fn open(&self, on: crate::meeting::Ear) -> Result<crate::meeting::Stop, String> {
            *self.0.lock().unwrap() = Some(on);
            Ok(Box::new(|| {}))
        }
    }
    fn goes_deaf(ear: &Arc<Mutex<Option<crate::meeting::Ear>>>) {
        (ear.lock().unwrap().as_mut().unwrap())(crate::meeting::Heard::Failed("denied".into()));
    }

    #[test]
    fn the_menu_clock_ticks_while_recording_and_stops_with_the_meeting() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), TICK);
        s.start();
        until(&saw_, |v| menus(v) >= 4);
        s.stop();
        std::thread::sleep(TICK * 3); // a refresh already on its way lands
        let n = menus(&saw(&saw_));
        std::thread::sleep(TICK * 5);
        assert_eq!(menus(&saw(&saw_)), n);
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn a_stopped_meetings_clock_does_not_refresh_the_next_meeting() {
        let tick = Duration::from_millis(40);
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), tick);
        s.start();
        s.stop();
        s.start(); // inside the first meeting's first tick
        assert_eq!(menus(&saw(&saw_)), 2);
        until(&saw_, |v| menus(v) >= 3);
        std::thread::sleep(tick / 2);
        assert_eq!(menus(&saw(&saw_)), 3, "one clock refreshes, not two");
        s.stop();
        until(&saw_, |v| recents(v) >= 2);
    }

    #[test]
    fn system_audio_denied_warns_once_at_start_and_a_deaf_call_afterwards_stays_quiet() {
        let ear = Arc::new(Mutex::new(None));
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, true, Later(ear.clone()), TICK);
        s.start();
        until(&saw_, |v| warnings(v) >= 1);
        goes_deaf(&ear);
        let n = menus(&until(&saw_, |v| menus(v) >= 2));
        until(&saw_, |v| menus(v) >= n + 4);
        assert_eq!(warnings(&saw(&saw_)), 1);
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn a_call_that_goes_deaf_mid_meeting_warns_once_however_long_it_stays_deaf() {
        let ear = Arc::new(Mutex::new(None));
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Later(ear.clone()), TICK);
        s.start();
        let n = menus(&until(&saw_, |v| menus(v) >= 3));
        assert_eq!(warnings(&saw(&saw_)), 0);
        goes_deaf(&ear);
        until(&saw_, |v| warnings(v) >= 1);
        until(&saw_, |v| menus(v) >= n + 6);
        assert_eq!(warnings(&saw(&saw_)), 1);
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn each_new_meeting_can_warn_again() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, true, Call(vec![]), TICK);
        s.start();
        until(&saw_, |v| warnings(v) >= 1);
        s.stop();
        s.start();
        until(&saw_, |v| warnings(v) >= 2);
        s.stop();
        until(&saw_, |v| recents(v) >= 2);
    }

    #[test]
    fn stop_writes_the_meeting_up_transcribing_then_idle_then_done_then_recent_then_cleanup_then_summary() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.start();
        let id = id_of(&s);
        s.stop();
        let up = write_up_of(&until(&saw_, |v| v.contains(&Saw::FollowUp(id))));
        assert_eq!(up[0], Saw::Notes(id, "transcribing".into(), 0.0, None));
        assert!(up[1..up.len() - 5].iter().all(|s| matches!(s, Saw::Notes(i, st, p, None) if *i == id && st == "transcribing" && (0.0..1.0).contains(p))), "{up:?}");
        assert_eq!(up[up.len() - 4..], [Saw::Idle, Saw::Notes(id, "done".into(), 1.0, None), Saw::Recent, Saw::FollowUp(id)]);
    }

    #[test]
    fn a_failed_write_up_says_why_and_retry_writes_it_up() {
        let (s, saw_, fail, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        fail.store(true, std::sync::atomic::Ordering::SeqCst);
        s.start();
        let id = id_of(&s);
        s.stop();
        let up = write_up_of(&until(&saw_, |v| recents(v) >= 1));
        assert_eq!(up[up.len() - 3..], [Saw::Idle, Saw::Notes(id, "failed".into(), 0.0, Some("The model fell over.".into())), Saw::Recent], "no Summary of a failed write-up");
        fail.store(false, std::sync::atomic::Ordering::SeqCst);
        s.retry(id).unwrap();
        let up = write_up_of(&until(&saw_, |v| v.contains(&Saw::FollowUp(id))));
        assert_eq!(up[up.len() - 4..], [Saw::Idle, Saw::Notes(id, "done".into(), 1.0, None), Saw::Recent, Saw::FollowUp(id)]);
        let again = up.iter().rposition(|s| *s == Saw::Notes(id, "transcribing".into(), 0.0, None)).unwrap();
        assert!(again > 3, "the retry starts from transcribing at 0: {up:?}");
    }

    #[test]
    fn retry_on_a_meeting_that_is_not_failed_says_so_and_sends_nothing() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let no = Err("That meeting isn't waiting to be tried again.".to_string());
        assert_eq!(s.retry(7), no);
        s.start();
        let id = id_of(&s);
        s.stop();
        let n = until(&saw_, |v| v.contains(&Saw::FollowUp(id))).len();
        assert_eq!(s.retry(id), no);
        std::thread::sleep(TICK * 3);
        assert_eq!(saw(&saw_).len(), n);
    }

    #[test]
    fn resume_writes_up_every_unfinished_meeting() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let mut ids = vec![];
        for _ in 0..2 {
            s.start();
            ids.push(id_of(&s));
            s.meetings.stop().unwrap(); // recorded, never written up: the app quit here
        }
        assert_eq!(count(&saw(&saw_), |s| matches!(s, Saw::Notes(..))), 0);
        s.resume();
        let v = until(&saw_, |v| recents(v) >= 2);
        for id in ids {
            assert!(v.contains(&Saw::Notes(id, "done".into(), 1.0, None)), "{v:?}");
        }
    }

    #[test]
    fn resume_checks_meetings_written_up_before_echo_handling_and_says_so_only_when_lines_changed() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let id: i64 = s.db.lock().unwrap().query_row("INSERT INTO meeting(started, state) VALUES (1, 'done') RETURNING id", [], |r| r.get(0)).unwrap();
        s.resume();
        let checked = || s.db.lock().unwrap().query_row("SELECT echo FROM meeting WHERE id = ?1", [id], |r| r.get::<_, Option<String>>(0)).unwrap();
        let t = std::time::Instant::now();
        while checked().is_none() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(checked().unwrap().contains("no audio"));
        assert_eq!(count(&saw(&saw_), |s| matches!(s, Saw::Echo(_))), 0, "nothing changed, so the page isn't told");
    }

    use crate::addon::AddonEvent as E;

    fn muted_at_now(state: MuteState) -> E {
        E::Mute { state, at: now_ms(), site: Site::Teams }
    }

    /// The session, already recording, with what start showed cleared from the record.
    fn recording() -> (Arc<MeetingSession>, Arc<Mutex<Vec<Saw>>>, tempfile::TempDir) {
        let (s, saw_, _, tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.start();
        saw_.lock().unwrap().clear();
        (s, saw_, tmp)
    }

    /// The last menu note, pill mic and live-card mic the session showed.
    fn shown(v: &[Saw]) -> (String, Option<String>, serde_json::Value) {
        let note = v.iter().rev().find_map(|s| if let Saw::MenuRunning(m) = s { Some(m.split(" | ").nth(1).unwrap().to_string()) } else { None }).unwrap();
        let pill = v.iter().rev().find_map(|s| if let Saw::Pill(Some(_), m, _) = s { Some(m.clone()) } else { None }).unwrap();
        let card = v.iter().rev().find_map(|s| if let Saw::Running(Some(c)) = s { Some(c["mic"].clone()) } else { None }).unwrap();
        (note, pill, card)
    }

    fn marks(s: &MeetingSession) -> Vec<String> {
        let db = s.db.lock().unwrap();
        let mut q = db.prepare("SELECT state FROM mute_event ORDER BY id").unwrap();
        q.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn a_mute_report_while_recording_updates_the_menu_the_pill_and_the_card() {
        let (s, saw_, _tmp) = recording();
        s.addon(muted_at_now(MuteState::Muted));
        let seen = saw(&saw_);
        // The order today: menu, then the card, then the pill.
        assert!(matches!(&seen[0], Saw::MenuRunning(_)) && matches!(&seen[1], Saw::Running(Some(_))) && matches!(&seen[2], Saw::Pill(Some(_), ..)), "{seen:?}");
        assert_eq!(seen.len(), 3);
        assert_eq!(shown(&seen), ("Muted · mirrored from Teams".into(), Some("muted".into()), "muted".into()));
        s.addon(muted_at_now(MuteState::Unmuted));
        assert_eq!(shown(&saw(&saw_)), ("Mic live · mirrored from Teams".into(), Some("live".into()), "live".into()));
        assert_eq!(marks(&s), ["muted", "unmuted"]);
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn a_mute_report_while_not_recording_shows_nothing_and_a_later_meeting_knows_it() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.addon(muted_at_now(MuteState::Muted));
        assert_eq!((saw(&saw_), marks(&s)), (vec![], vec![]));
        s.start();
        assert_eq!(shown(&saw(&saw_)), ("Muted · mirrored from Teams".into(), Some("muted".into()), "muted".into()));
        assert_eq!(s.running().unwrap()["mic"], "muted");
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn the_add_on_dropping_mid_meeting_puts_the_words_back_to_mic_live() {
        let (s, saw_, _tmp) = recording();
        s.addon(muted_at_now(MuteState::Muted));
        for gone in [|s: &Arc<MeetingSession>| s.dropped(), |s: &Arc<MeetingSession>| s.addon(E::Disconnected)] {
            s.addon(muted_at_now(MuteState::Muted));
            gone(&s);
            assert_eq!(shown(&saw(&saw_)), ("Mic live".into(), None, serde_json::Value::Null));
        }
        assert_eq!(marks(&s), ["muted", "muted", "unknown", "muted", "unknown"]);
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn tab_start_with_nothing_recording_starts_a_meeting_sourced_from_the_tab() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.addon(E::TabStart { site: Site::Teams });
        assert!(s.running().is_some());
        assert_eq!(s.running().unwrap()["source"], "Teams (Chrome tab)");
        let card = saw(&saw_).into_iter().rev().find_map(|s| if let Saw::Running(Some(c)) = s { Some(c) } else { None }).unwrap();
        assert_eq!(card["source"], "Teams (Chrome tab)");
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn tab_start_with_notes_off_starts_nothing_and_shows_nothing() {
        let (s, saw_, _, _tmp) = rig(false, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.addon(E::TabStart { site: Site::Teams });
        assert_eq!((saw(&saw_), s.running()), (vec![], None));
    }

    #[test]
    fn tab_start_during_a_recording_keeps_the_meeting_and_sets_its_source() {
        let (s, saw_, _tmp) = recording();
        let id = id_of(&s);
        s.addon(E::TabStart { site: Site::Meet });
        let card = s.running().unwrap();
        assert_eq!((card["id"].as_i64(), card["source"].clone()), (Some(id), "Meet (Chrome tab)".into()));
        assert_eq!(count(&saw(&saw_), |s| matches!(s, Saw::Place(_))), 0, "no second start");
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    #[test]
    fn captions_are_kept_on_the_recording_meeting_and_unavailable_marks_it() {
        let (s, saw_, _tmp) = recording();
        let caption = |name: &str, text: Option<&str>| E::Caption { name: name.into(), at: now_ms(), text: text.map(Into::into) };
        s.addon(caption("  Ana ", Some(" hello ")));
        s.addon(E::Connected); // ignored
        s.addon(E::CaptionsUnavailable);
        let db = s.db.lock().unwrap();
        let rows: Vec<(String, Option<String>)> = db.prepare("SELECT name, text FROM caption_event").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        assert_eq!(rows, [("Ana".to_string(), Some("hello".to_string()))]);
        let captions: Option<String> = db.query_row("SELECT captions FROM meeting", [], |r| r.get(0)).unwrap();
        assert_eq!(captions.as_deref(), Some("unavailable"));
        drop(db);
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    /// Mic not asked yet; the pop-up is answered `answer`.
    fn asking(answer: Mic) -> (Arc<MeetingSession>, Arc<Mutex<Vec<Saw>>>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        meeting::set(&db.lock().unwrap(), &meeting::Notes { on: true, key: meeting::KEY.into(), keep_teams: false, keep_meet: false, offer_calls: true, stop_on_end: true, show_pill: true }).unwrap();
        let meetings = Meetings::new(db.clone(), tmp.path().into(), Box::new(Fake(crate::runtime::RATE as usize)), Box::new(Call(vec![])));
        let saw = Arc::new(Mutex::new(vec![]));
        let world = Scripted { mic: Mutex::new(Mic::Ask), on_ask: answer, denied: false, fail: Default::default(), saw: saw.clone(), keys: Default::default() };
        (MeetingSession::new(Box::new(world), db, meetings, Duration::from_secs(3600)), saw, tmp)
    }

    #[test]
    fn a_mic_allowed_at_the_pop_up_records_at_once() {
        let (s, saw_, _tmp) = asking(Mic::Allowed);
        s.start_from("Slack");
        assert!(s.recording(), "{:?}", saw(&saw_));
        s.stop();
    }

    #[test]
    fn a_detected_call_that_cant_start_says_why_on_the_pill() {
        let (s, saw_, _tmp) = asking(Mic::Denied);
        s.start_from("Slack");
        assert!(!s.recording());
        assert!(saw(&saw_).contains(&Saw::Warning("Couldn’t start meeting notes".into())), "{:?}", saw(&saw_));
    }

    #[test]
    fn starting_from_a_detected_call_names_the_meeting_after_its_app() {
        let (s, _, _tmp) = session(true, Mic::Allowed, false);
        s.start_from("Slack");
        assert!(s.recording());
        assert_eq!(s.running().unwrap()["source"], "Slack");
        s.stop();
        assert!(!s.recording());
        let (s, _, _tmp) = session(false, Mic::Allowed, false);
        s.start_from("Slack");
        assert!(!s.recording(), "notes off: nothing starts");
    }

    #[test]
    fn the_mic_words_are_the_same_on_the_menu_the_pill_and_the_card() {
        assert_eq!([Some(true), Some(false), None].map(|m| mic_words(m, Site::Teams)), ["Muted · mirrored from Teams", "Mic live · mirrored from Teams", "Mic live"]);
        assert_eq!([Some(true), Some(false), None].map(|m| mic_words(m, Site::Meet)), ["Muted · mirrored from Meet", "Mic live · mirrored from Meet", "Mic live"]);
        assert_eq!(mic_words(Some(true), Site::Other), "Muted · mirrored from the browser");
        let (s, saw_, _tmp) = recording();
        for (state, mic, words) in [(MuteState::Muted, "muted", "Muted · mirrored from Teams"), (MuteState::Unmuted, "live", "Mic live · mirrored from Teams")] {
            s.addon(muted_at_now(state));
            let seen = saw(&saw_);
            assert!(matches!(seen.last(), Some(Saw::Pill(Some(_), Some(m), Some(w))) if m == mic && w == words));
            assert!(matches!(seen.last_chunk::<3>().unwrap(), [Saw::MenuRunning(m), Saw::Running(Some(c)), Saw::Pill(..)] if m.ends_with(&format!("| {words}")) && c["mic"] == mic && c["words"] == words));
        }
        s.addon(E::Mute { state: MuteState::Muted, at: now_ms(), site: Site::Meet });
        assert!(matches!(saw(&saw_).last(), Some(Saw::Pill(Some(_), Some(m), Some(w))) if m == "muted" && w == "Muted · mirrored from Meet"));
        s.addon(E::Disconnected);
        assert!(matches!(saw(&saw_).last(), Some(Saw::Pill(Some(_), None, Some(w))) if w == "Mic live"));
        assert_eq!(s.running().unwrap()["words"], "Mic live");
        s.stop();
        until(&saw_, |v| recents(v) >= 1);
    }

    /// A provider on a local server that answers any transcription and keeps what it was sent, with Main and Fast on it.
    fn cloud(s: &MeetingSession) -> (i64, Arc<Mutex<Vec<Vec<u8>>>>) {
        let (base, bodies) = crate::ai::tests::server(|_, _, _| (200, r#"{"segments":[{"start":0.1,"end":0.5,"text":"hi"}],"usage":{"cost":0.001}}"#.into()));
        let db = s.db.lock().unwrap();
        let keys = crate::ai::tests::Memory::default();
        let id = serde_json::to_value(crate::ai::add(&db, &keys, "openai", "Cloud", &base, "").unwrap()).unwrap()["id"].as_i64().unwrap();
        crate::ai::set(&db, "main", id, "m").unwrap();
        crate::ai::set(&db, "fast", id, "m").unwrap();
        (id, bodies)
    }

    fn one_meeting(s: &Arc<MeetingSession>, saw_: &Arc<Mutex<Vec<Saw>>>) -> i64 {
        s.start();
        let id = id_of(s);
        s.stop();
        until(saw_, |v| recents(v) >= 1);
        id
    }

    #[test]
    fn transcribe_again_runs_a_written_up_meeting_through_the_picked_model_and_leaves_the_setting_alone() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let (p, bodies) = cloud(&s);
        let id = one_meeting(&s, &saw_);
        assert!(bodies.lock().unwrap().is_empty(), "written up on the Local model first");
        saw_.lock().unwrap().clear();
        let play = _tmp.path().join(format!("meetings/{id}/play.m4a"));
        for _ in 0..400 {
            if play.exists() {
                break; // made after the first write-up
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::write(&play, b"made from the old lines").unwrap();
        s.again(id, &stt::Pick::Endpoint { provider: p, model: "whisper".into() }).unwrap();
        let up = write_up_of(&until(&saw_, |v| v.contains(&Saw::FollowUp(id))));
        assert_eq!(up[0], Saw::Notes(id, "transcribing".into(), 0.0, None));
        assert_eq!(bodies.lock().unwrap().len(), 1);
        let page = meeting::page(&s.db.lock().unwrap(), id).unwrap();
        assert_eq!((page.stt.as_ref().unwrap()["model"].as_str(), page.segments.len()), (Some("whisper"), 1));
        assert_eq!(stt::saved(&s.db.lock().unwrap()).unwrap(), stt::Pick::Local { model: None }, "the setting for new meetings stays");
        assert!(std::fs::read(&play).map_or(true, |b| b != b"made from the old lines"), "playback is made again from the new lines' echo");

        saw_.lock().unwrap().clear();
        s.again(id, &stt::Pick::Local { model: None }).unwrap();
        until(&saw_, |v| v.contains(&Saw::FollowUp(id)));
        assert!(meeting::page(&s.db.lock().unwrap(), id).unwrap().stt.is_none(), "says it's the Local model's now");
        assert!(!saw_.lock().unwrap().iter().any(|v| matches!(v, Saw::Model(_))), "plain local is meeting notes' own model");

        saw_.lock().unwrap().clear();
        s.again(id, &stt::Pick::Local { model: Some("whisper-turbo".into()) }).unwrap();
        until(&saw_, |v| v.contains(&Saw::FollowUp(id)));
        assert!(saw_.lock().unwrap().contains(&Saw::Model("whisper-turbo".into())), "another downloaded model, picked for this once");

        assert_eq!(s.again(99, &stt::Pick::Local { model: None }), Err("That meeting isn't there anymore.".into()));
        // The resolving itself (gone provider, blank model) is stt.rs's to test.
        assert!(s.again(id, &stt::Pick::NotSet).is_err(), "a pick that can't run writes nothing up");
    }

    #[test]
    fn after_the_write_up_play_is_ready_and_the_listening_copy_is_kept_small() {
        let (s, saw_, _, tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        s.start();
        let id = id_of(&s);
        s.stop();
        until(&saw_, |v| v.contains(&Saw::FollowUp(id)));
        let folder = tmp.path().join(format!("meetings/{id}"));
        for _ in 0..400 {
            if folder.join("them.m4a").exists() && !folder.join("them.hq.wav").exists() {
                break; // compacting's last step
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let has = |f: &str| folder.join(f).exists();
        assert_eq!((has("play.m4a"), has("mic.m4a"), has("them.m4a")), (true, true, true));
        assert_eq!((has("mic.hq.wav"), has("them.hq.wav")), (false, false));
        assert!(has("mic.wav") && has("them.wav"), "the 16 kHz tracks stay, for speech-to-text and older builds");
    }

    #[test]
    fn with_the_local_model_picked_nothing_leaves_the_mac() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let (_, bodies) = cloud(&s);
        let id = one_meeting(&s, &saw_);
        assert!(saw(&saw_).contains(&Saw::Notes(id, "done".into(), 1.0, None)));
        assert!(bodies.lock().unwrap().is_empty(), "a provider with Main and Fast on it still got nothing");
        assert!(meeting::page(&s.db.lock().unwrap(), id).unwrap().stt.is_none());
    }

    #[test]
    fn a_picked_endpoint_transcribes_and_once_its_provider_is_removed_new_meetings_say_not_set() {
        let (s, saw_, _, _tmp) = rig(true, Mic::Allowed, false, Call(vec![]), Duration::from_secs(3600));
        let (p, bodies) = cloud(&s);
        crate::ai::feature_set(&s.db.lock().unwrap(), crate::ai::STT, &format!("{p}|whisper")).unwrap();
        let id = one_meeting(&s, &saw_);
        assert_eq!(bodies.lock().unwrap().len(), 1, "the mic's second of tone; the silent call isn't sent");
        assert_eq!(meeting::page(&s.db.lock().unwrap(), id).unwrap().stt.unwrap()["provider"], "Cloud");
        crate::ai::remove(&mut s.db.lock().unwrap(), &crate::ai::tests::Memory::default(), p).unwrap();
        saw_.lock().unwrap().clear();
        let id = one_meeting(&s, &saw_);
        let failed = Saw::Notes(id, "failed".into(), 0.0, Some("Transcription incomplete (0 of 1): Meeting speech-to-text: Not set".into()));
        assert!(saw(&saw_).contains(&failed), "{:?}", saw(&saw_));
        assert_eq!(bodies.lock().unwrap().len(), 1);
        s.retry_local(id).unwrap();
        until(&saw_, |v| v.contains(&Saw::Notes(id, "done".into(), 1.0, None)));
    }
}

