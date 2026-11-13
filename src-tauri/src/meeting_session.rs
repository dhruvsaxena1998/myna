//! The Meeting session: one meeting recording from shortcut to write-up, and what the add-on reports during it.
//! It owns the app's `Meetings` and reaches the Mac only through `World`, so tests drive it with a fake.

use crate::addon::{AddonEvent, MuteState};
use crate::meeting::{self, Meetings, Stt, Voices};
use crate::permissions::Mic;
use crate::{notify, now_ms, permissions, pill};
use rusqlite::Connection;
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
    /// The recording pill with its start time and mic word, or hidden when `started` is none.
    fn pill(&self, started: Option<i64>, mic: Option<&str>);
    /// A warning over the pill.
    fn pill_warning(&self, title: &str, hint: &str);
    /// Moves the pill to "bottom", "top-right", ... where the user set it.
    fn place(&self, place: String);
    /// The menu item and the tray title for a running meeting.
    fn menu_running(&self, clock: &str, note: &str);
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
        match self.world.mic() {
            Mic::Allowed => {}
            Mic::Ask => return self.world.ask_mic(), // macOS's pop-up; the next press records
            Mic::Denied => return self.world.notify("Needs the Microphone", pill::ALLOW),
        }
        let id = match self.meetings.start(now_ms()) {
            Ok(id) => id,
            Err(e) => return self.world.notify("Couldn’t start meeting notes", &e),
        };
        self.show_running();
        if let Ok(look) = pill::look(&self.db.lock().unwrap()) {
            self.world.place(look.place);
        }
        let this = self.clone();
        std::thread::spawn(move || this.clock(id));
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
            Ok(Some(id)) => self.write_up(id),
            Ok(None) => {}
            Err(e) => self.world.notify("Couldn’t save the recording", &e),
        }
        self.world.menu_idle();
        self.world.running(None);
        self.world.pill(None, None);
    }

    /// Try again on a meeting whose notes failed.
    pub fn retry(self: &Arc<Self>, id: i64) -> Result<(), String> {
        if !self.meetings.failed(id) {
            return Err("That meeting isn't waiting to be tried again.".into());
        }
        self.write_up(id);
        Ok(())
    }

    /// At launch: finishes and writes up the meetings the app died or quit in the middle of.
    pub fn resume(self: &Arc<Self>) {
        for id in self.meetings.unfinished() {
            self.write_up(id);
        }
    }

    /// The live card: `{ id, started, source, mic }`, or none when not recording.
    pub fn running(&self) -> Option<serde_json::Value> {
        let (id, started) = self.meetings.running()?;
        let source: Option<String> = self.db.lock().unwrap().query_row("SELECT source_app FROM meeting WHERE id = ?1", [id], |r| r.get(0)).ok().flatten();
        Some(serde_json::json!({ "id": id, "started": started, "source": source, "mic": mic_word(self.meetings.muted()) }))
    }

    /// The Teams mute button changed: the timeline, then the menu, pill and live card.
    pub fn mute(&self, state: MuteState, at: i64) {
        self.meetings.mute(state, at, now_ms());
        self.show_running();
    }

    /// The add-on dropped: Myna can't see the button anymore.
    pub fn dropped(&self) {
        self.mute(MuteState::Unknown, now_ms());
    }

    /// What the add-on reports. Mute and a drop go to the timeline, then the menu, pill and live card; captions are kept
    /// on the meeting for naming its voices; the Teams tab's sound starts a meeting if none is recording.
    pub fn addon(self: &Arc<Self>, e: AddonEvent) {
        use AddonEvent as E;
        match e {
            E::Connected | E::SelectorsChanged => {}
            E::Disconnected => self.dropped(),
            E::Mute { state, at } => self.mute(state, at),
            E::Caption { name, at, text } => self.meetings.caption(&name, at, text.as_deref(), now_ms()),
            E::CaptionsUnavailable => self.meetings.captions_unavailable(),
            E::TabAudio { at, ts, samples } => self.meetings.tab_audio(at, ts, &samples),
            E::TabStop => self.meetings.tab_stop(),
            E::TabStart => {
                if self.meetings.running().is_none() {
                    self.start();
                }
                if self.meetings.tab_start("Teams (Chrome tab)") {
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
        self.world.pill(Some(started), mic_word(self.meetings.muted()));
    }

    fn show_menu(&self, started: i64) {
        // The note says how the mic is while recording; the idle menu puts the usual words back.
        let note = match self.meetings.muted() {
            Some(true) => "Muted · mirrored from Teams",
            Some(false) => "Mic live · mirrored from Teams",
            None => "Mic live",
        };
        self.world.menu_running(&meeting::clock(now_ms() - started), note);
    }

    /// Writes up meeting `id` on its own thread, saying how it goes with `meeting-notes`: "transcribing" with progress
    /// 0 to 1, then "done" or "failed" with the reason.
    fn write_up(self: &Arc<Self>, id: i64) {
        let this = self.clone();
        std::thread::spawn(move || {
            let w = &*this.world;
            w.notes(id, "transcribing", 0.0, None);
            let done = this.meetings.transcribe(id, w, w, &mut |p| w.notes(id, "transcribing", p, None));
            w.idle(); // the model idles out like after a dictation
            match done {
                Ok(()) => w.notes(id, "done", 1.0, None),
                Err(e) => w.notes(id, "failed", 0.0, Some(&e)),
            }
            w.recent();
        });
    }
}

/// `mic`: "live" or "muted" while the add-on mirrors the mute button, else null.
fn mic_word(muted: Option<bool>) -> Option<&'static str> {
    muted.map(|m| if m { "muted" } else { "live" })
}

/// The real Mac: the app's permissions, notifications, pill, menu and windows.
pub struct Mac(pub AppHandle);

impl Stt for Mac {
    fn transcribe(&self, audio: &[f32]) -> Result<Vec<crate::runtime::Segment>, String> {
        self.0.state::<Arc<crate::session::Session>>().transcribe(audio)
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
    fn pill(&self, started: Option<i64>, mic: Option<&str>) {
        pill::meeting(&self.0, started, mic)
    }
    fn pill_warning(&self, title: &str, hint: &str) {
        pill::error(&self.0, title, hint)
    }
    fn place(&self, place: String) {
        pill::place(&self.0, place)
    }
    fn menu_running(&self, clock: &str, note: &str) {
        if let Some(m) = self.0.try_state::<crate::MeetItem>() {
            let _ = m.item.set_text(format!("Stop recording  {clock}"));
            let _ = m.note.set_text(note);
        }
        // shortcut: a tray title is plain text, so the dot is the red emoji; draw a red icon if that looks off.
        if let Some(t) = self.0.tray_by_id("myna") {
            let _ = t.set_title(Some(format!("🔴 {clock}")));
        }
    }
    fn menu_idle(&self) {
        if let Some(m) = self.0.try_state::<crate::MeetItem>() {
            let _ = m.item.set_text("Start meeting notes");
            let _ = crate::sync_meeting(&self.0); // puts the usual note back
        }
        if let Some(t) = self.0.tray_by_id("myna") {
            let _ = t.set_title(None::<String>);
        }
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
        Pill(Option<i64>, Option<String>),
        Warning(String),
        Place(String),
        MenuRunning(String),
        MenuIdle,
        Running(Option<serde_json::Value>),
        Notes(i64, String),
        StopTab,
        Recent,
        Idle,
    }

    struct Scripted {
        mic: Mic,
        denied: bool,
        saw: Arc<Mutex<Vec<Saw>>>,
    }

    impl Scripted {
        fn log(&self, s: Saw) {
            self.saw.lock().unwrap().push(s);
        }
    }

    impl Stt for Scripted {
        fn transcribe(&self, _: &[f32]) -> Result<Vec<Segment>, String> {
            Ok(vec![])
        }
    }
    impl Voices for Scripted {
        fn diarize(&self, _: &[f32]) -> Result<Vec<Turn>, String> {
            Ok(vec![])
        }
    }
    impl World for Scripted {
        fn mic(&self) -> Mic {
            self.mic
        }
        fn ask_mic(&self) {
            self.log(Saw::AskMic)
        }
        fn audio_denied(&self) -> bool {
            self.denied
        }
        fn notify(&self, title: &str, body: &str) {
            self.log(Saw::Notify(title.into(), body.into()))
        }
        fn pill(&self, started: Option<i64>, mic: Option<&str>) {
            self.log(Saw::Pill(started, mic.map(Into::into)))
        }
        fn pill_warning(&self, title: &str, _: &str) {
            self.log(Saw::Warning(title.into()))
        }
        fn place(&self, place: String) {
            self.log(Saw::Place(place))
        }
        fn menu_running(&self, clock: &str, note: &str) {
            self.log(Saw::MenuRunning(format!("{clock} | {note}")))
        }
        fn menu_idle(&self) {
            self.log(Saw::MenuIdle)
        }
        fn running(&self, card: Option<serde_json::Value>) {
            self.log(Saw::Running(card))
        }
        fn notes(&self, id: i64, state: &str, _: f32, _: Option<&str>) {
            self.log(Saw::Notes(id, state.into()))
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
    }

    fn session(notes_on: bool, mic: Mic, denied: bool) -> (Arc<MeetingSession>, Arc<Mutex<Vec<Saw>>>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Mutex::new(crate::storage::open(tmp.path()).unwrap()));
        meeting::set(&db.lock().unwrap(), &meeting::Notes { on: notes_on, key: meeting::KEY.into(), keep_teams: false }).unwrap();
        let meetings = Meetings::new(db.clone(), tmp.path().into(), Box::new(Fake(crate::runtime::RATE as usize)), Box::new(Call(vec![])));
        let saw = Arc::new(Mutex::new(vec![]));
        let world = Scripted { mic, denied, saw: saw.clone() };
        (MeetingSession::new(Box::new(world), db, meetings, Duration::from_secs(3600)), saw, tmp)
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
        v.into_iter().filter(|s| !matches!(s, Saw::Notes(..) | Saw::Recent | Saw::Idle | Saw::MenuRunning(_))).collect()
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
    fn start_shows_menu_card_pill_then_places_it() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        s.start();
        let card = s.running().unwrap();
        let (id, started) = (card["id"].as_i64().unwrap(), card["started"].as_i64().unwrap());
        assert_eq!((card["source"].clone(), card["mic"].clone()), (serde_json::Value::Null, serde_json::Value::Null));
        let seen = saw(&saw_);
        assert!(matches!(&seen[0], Saw::MenuRunning(m) if m.ends_with("| Mic live")));
        assert_eq!(seen[1..4], [Saw::Running(Some(serde_json::json!({ "id": id, "started": started, "source": null, "mic": null }))), Saw::Pill(Some(started), None), Saw::Place("bottom".into())]);
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
        assert_eq!(seen.len(), 7, "{seen:?}");
        assert_eq!(seen[3..], [Saw::StopTab, Saw::MenuIdle, Saw::Running(None), Saw::Pill(None, None)]);
        let seen = settled(&saw_);
        assert!(matches!(seen.iter().find(|s| matches!(s, Saw::Notes(..))), Some(Saw::Notes(_, state)) if state == "transcribing"));
    }

    #[test]
    fn stop_with_nothing_recording_still_sends_the_idle_state() {
        let (s, saw_, _tmp) = session(true, Mic::Allowed, false);
        s.stop();
        assert_eq!(saw(&saw_), vec![Saw::StopTab, Saw::MenuIdle, Saw::Running(None), Saw::Pill(None, None)]);
    }
}
