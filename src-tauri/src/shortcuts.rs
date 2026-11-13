//! Shortcuts: the global keys and what each one does. It registers keys through a `Registrar`, keeps the
//! key-to-action map in memory, and answers "what does this key do" without the database or parsing.
//! Voice typing's keys and meeting notes' key live here.

use crate::meeting::Notes;
use crate::voice::{self, Voice};
use std::sync::Mutex;
use tauri_plugin_global_shortcut::Shortcut;

/// What a key press asks the app to do.
#[derive(Debug, PartialEq)]
pub enum Do {
    /// Run the Dictation session's `key(pressed, clean)`.
    Dictate { pressed: bool, clean: bool },
    /// fn let go while Settings records a key: tell the Settings page.
    TellSettings,
    /// The meeting key was pressed: start meeting notes, or stop if recording.
    ToggleMeeting,
}

const BUSY: &str = "Another app is using that shortcut. Pick a different one.";

/// What the module needs from the Mac.
pub trait Registrar: Send + Sync + 'static {
    /// Takes a key from macOS, or the plugin's error text.
    fn register(&self, key: &str) -> Result<(), String>;
    fn unregister(&self, key: &str);
    /// Starts the fn tap. It starts once; later calls do nothing.
    fn watch_fn(&self);
}

/// One feature's keys: what's registered now, with whether each runs Dictate with cleanup (always false for meeting notes).
#[derive(Default)]
struct Group {
    keys: Vec<(String, bool)>,
    /// `keys` parsed once, without fn.
    parsed: Vec<(Shortcut, bool)>,
}

#[derive(Default)]
struct State {
    voice: Group,
    meeting: Group,
    paused: bool,
}

pub struct Shortcuts {
    reg: Box<dyn Registrar>,
    state: Mutex<State>,
}

impl Shortcuts {
    pub fn new(reg: Box<dyn Registrar>) -> Self {
        Self { reg, state: Default::default() }
    }

    /// Makes `g` hold exactly `want`. The new keys are taken before the old ones go, so a refusal leaves the old ones working.
    /// `who` prefixes the log line.
    fn swap(&self, g: &mut Group, want: Vec<(String, bool)>, who: &str) -> Result<(), String> {
        let have = |k: &str| g.keys.iter().any(|(h, _)| h == k);
        let mut taken: Vec<&str> = vec![];
        for (k, _) in want.iter().filter(|(k, _)| k != voice::FN && !have(k)) {
            if let Err(e) = self.reg.register(k) {
                eprintln!("{who}: couldn't use {k}: {e}");
                taken.iter().for_each(|k| self.reg.unregister(k));
                return Err(BUSY.into());
            }
            taken.push(k);
        }
        for (k, _) in g.keys.iter().filter(|(k, _)| k != voice::FN && !want.iter().any(|(w, _)| w == k)) {
            self.reg.unregister(k);
        }
        g.parsed = want.iter().filter_map(|(k, c)| Some((k.parse().ok()?, *c))).collect();
        g.keys = want;
        Ok(())
    }

    /// Registers voice typing's keys while it's on and not paused, and drops them otherwise.
    pub fn sync_voice(&self, v: &Voice) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        let want = if v.on && !s.paused { v.keys() } else { vec![] };
        self.swap(&mut s.voice, want, "dictation")?;
        // The recorder hears fn from the tap too, so it watches while paused.
        if v.on && (v.keys().iter().any(|(k, _)| k == voice::FN) || s.paused) {
            self.reg.watch_fn();
        }
        Ok(())
    }

    /// Registers meeting notes' key while they're on, the key isn't "" and not paused, and drops it otherwise.
    /// A refusal here leaves voice typing's keys alone, and the other way round.
    pub fn sync_meeting(&self, n: &Notes) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        let want = if n.on && !n.key.is_empty() && !s.paused { vec![(n.key.clone(), false)] } else { vec![] };
        self.swap(&mut s.meeting, want, "meeting notes")
    }

    /// Lets go of every key while Settings records a new one, and takes them back after. Each group syncs on its
    /// own: voice typing's result, then meeting notes'.
    pub fn pause(&self, paused: bool, v: &Voice, n: &Notes) -> (Result<(), String>, Result<(), String>) {
        self.state.lock().unwrap().paused = paused;
        (self.sync_voice(v), self.sync_meeting(n))
    }

/// A key from the plugin: what it does, or nothing if it isn't one of ours.
    pub fn dispatch(&self, key: &Shortcut, pressed: bool) -> Option<Do> {
        let s = self.state.lock().unwrap();
        if s.meeting.parsed.iter().any(|(k, _)| k == key) {
            return pressed.then_some(Do::ToggleMeeting);
        }
        let (_, clean) = s.voice.parsed.iter().find(|(k, _)| k == key)?;
        Some(Do::Dictate { pressed, clean: *clean })
    }

    /// fn going down or up.
    pub fn dispatch_fn(&self, down: bool) -> Option<Do> {
        let s = self.state.lock().unwrap();
        if s.paused {
            // On release, so the release of this press doesn't reach the new shortcut.
            return (!down).then_some(Do::TellSettings);
        }
        let (_, clean) = s.voice.keys.iter().find(|(k, _)| k == voice::FN)?;
        Some(Do::Dictate { pressed: down, clean: *clean })
    }
}

/// The real registrar: the global-shortcut plugin and the fn tap.
pub struct Mac(pub tauri::AppHandle);

impl Registrar for Mac {
    fn register(&self, key: &str) -> Result<(), String> {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        self.0.global_shortcut().register(key).map_err(|e| e.to_string())
    }

    fn unregister(&self, key: &str) {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let _ = self.0.global_shortcut().unregister(key);
    }

    fn watch_fn(&self) {
        let app = self.0.clone();
        crate::fnkey::watch(move |down| crate::on_fn(&app, down));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Default)]
    struct Record {
        calls: Vec<String>,
        refuse: Vec<String>,
        watches: usize,
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Record>>);

    impl Registrar for Fake {
        fn register(&self, key: &str) -> Result<(), String> {
            let mut r = self.0.lock().unwrap();
            if r.refuse.iter().any(|k| k == key) {
                return Err("taken".into());
            }
            r.calls.push(format!("+{key}"));
            Ok(())
        }
        fn unregister(&self, key: &str) {
            self.0.lock().unwrap().calls.push(format!("-{key}"));
        }
        fn watch_fn(&self) {
            self.0.lock().unwrap().watches += 1;
        }
    }

    impl Fake {
        fn calls(&self) -> Vec<String> {
            std::mem::take(&mut self.0.lock().unwrap().calls)
        }
        fn refuse(&self, k: &str) {
            self.0.lock().unwrap().refuse.push(k.into());
        }
        fn watches(&self) -> usize {
            self.0.lock().unwrap().watches
        }
    }

    fn setup() -> (Shortcuts, Fake) {
        let f = Fake::default();
        (Shortcuts::new(Box::new(f.clone())), f)
    }

    fn voice(on: bool, key: &str, key2: Option<&str>, clean: Option<&str>, clean2: Option<&str>) -> Voice {
        let o = |k: Option<&str>| k.map(String::from);
        Voice { on, key: key.into(), clean: o(clean), key2: o(key2), clean2: o(clean2), mode: "hold".into(), paste: "clipboard".into(), rate: false, sounds: true }
    }

    fn notes(on: bool, key: &str) -> Notes {
        Notes { on, key: key.into(), keep_teams: false }
    }

    fn all() -> Voice {
        voice(true, "Alt+Space", Some("Alt+D"), Some("Alt+C"), Some("Alt+E"))
    }

    fn key(k: &str) -> Shortcut {
        k.parse().unwrap()
    }

    #[test]
    fn voice_typing_on_registers_every_key_and_never_fn() {
        let (s, f) = setup();
        s.sync_voice(&voice(true, "Fn", Some("Alt+D"), Some("Alt+C"), Some("Alt+E"))).unwrap();
        assert_eq!(f.calls(), ["+Alt+D", "+Alt+C", "+Alt+E"]);
        let (s, f) = setup();
        s.sync_voice(&all()).unwrap();
        assert_eq!(f.calls(), ["+Alt+Space", "+Alt+D", "+Alt+C", "+Alt+E"]);
    }

    #[test]
    fn voice_typing_off_registers_nothing_and_drops_what_was_held() {
        let (s, f) = setup();
        s.sync_voice(&voice(false, "Alt+Space", None, None, None)).unwrap();
        assert!(f.calls().is_empty());
        s.sync_voice(&all()).unwrap();
        f.calls();
        s.sync_voice(&voice(false, "Alt+Space", None, None, None)).unwrap();
        assert_eq!(f.calls(), ["-Alt+Space", "-Alt+D", "-Alt+C", "-Alt+E"]);
        assert_eq!(s.dispatch(&key("Alt+Space"), true), None);
    }

    #[test]
    fn a_refused_key_says_so_gives_back_what_it_took_and_keeps_the_old_keys() {
        let (s, f) = setup();
        s.sync_voice(&voice(true, "Alt+Space", None, None, None)).unwrap();
        f.calls();
        f.refuse("Alt+C");
        let err = s.sync_voice(&voice(true, "Alt+F", Some("Alt+D"), Some("Alt+C"), None)).unwrap_err();
        assert_eq!(err, BUSY);
        assert_eq!(f.calls(), ["+Alt+F", "+Alt+D", "-Alt+F", "-Alt+D"]);
        assert_eq!(s.dispatch(&key("Alt+Space"), true), Some(Do::Dictate { pressed: true, clean: false }));
        assert_eq!(s.dispatch(&key("Alt+F"), true), None);
    }

    #[test]
    fn changing_only_the_action_of_a_held_key_leaves_it_registered() {
        let (s, f) = setup();
        s.sync_voice(&voice(true, "Alt+Space", None, None, None)).unwrap();
        f.calls();
        s.sync_voice(&voice(true, "Alt+D", None, Some("Alt+Space"), None)).unwrap();
        assert_eq!(f.calls(), ["+Alt+D"]);
        assert_eq!(s.dispatch(&key("Alt+Space"), true), Some(Do::Dictate { pressed: true, clean: true }));
    }

    #[test]
    fn pausing_drops_every_key_and_resuming_takes_them_back() {
        let (s, f) = setup();
        s.sync_voice(&all()).unwrap();
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        f.calls();
        let (v, m) = s.pause(true, &all(), &notes(true, "Alt+M"));
        assert!(v.is_ok() && m.is_ok());
        assert_eq!(f.calls(), ["-Alt+Space", "-Alt+D", "-Alt+C", "-Alt+E", "-Alt+M"]);
        assert_eq!(s.dispatch(&key("Alt+M"), true), None, "nothing while paused");
        s.pause(false, &all(), &notes(true, "Alt+M")).0.unwrap();
        assert_eq!(f.calls(), ["+Alt+Space", "+Alt+D", "+Alt+C", "+Alt+E", "+Alt+M"]);
        assert_eq!(s.dispatch(&key("Alt+M"), true), Some(Do::ToggleMeeting));
    }

    #[test]
    fn meeting_notes_on_registers_the_key_off_or_no_key_registers_nothing_and_drops_a_held_one() {
        let (s, f) = setup();
        s.sync_meeting(&notes(false, "Alt+M")).unwrap();
        s.sync_meeting(&notes(true, "")).unwrap();
        assert!(f.calls().is_empty());
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        assert_eq!(f.calls(), ["+Alt+M"]);
        s.sync_meeting(&notes(false, "Alt+M")).unwrap();
        assert_eq!(f.calls(), ["-Alt+M"]);
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        s.sync_meeting(&notes(true, "")).unwrap();
        assert_eq!(f.calls(), ["+Alt+M", "-Alt+M"]);
        assert_eq!(s.dispatch(&key("Alt+M"), true), None);
    }

    #[test]
    fn changing_the_meeting_key_takes_the_new_one_before_dropping_the_old() {
        let (s, f) = setup();
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        f.calls();
        s.sync_meeting(&notes(true, "Alt+N")).unwrap();
        assert_eq!(f.calls(), ["+Alt+N", "-Alt+M"]);
        assert_eq!(s.dispatch(&key("Alt+M"), true), None);
        assert_eq!(s.dispatch(&key("Alt+N"), true), Some(Do::ToggleMeeting));
    }

    #[test]
    fn a_refused_meeting_key_says_so_and_keeps_the_old_one() {
        let (s, f) = setup();
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        f.calls();
        f.refuse("Alt+N");
        assert_eq!(s.sync_meeting(&notes(true, "Alt+N")).unwrap_err(), BUSY);
        assert!(f.calls().is_empty());
        assert_eq!(s.dispatch(&key("Alt+M"), true), Some(Do::ToggleMeeting));
        assert_eq!(s.dispatch(&key("Alt+N"), true), None);
    }

    #[test]
    fn a_refusal_in_one_group_leaves_the_other_alone() {
        let (s, f) = setup();
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        s.sync_voice(&voice(true, "Alt+Space", None, None, None)).unwrap();
        f.calls();
        f.refuse("Alt+D");
        f.refuse("Alt+N");
        assert!(s.sync_voice(&voice(true, "Alt+D", None, None, None)).is_err());
        assert_eq!(s.dispatch(&key("Alt+M"), true), Some(Do::ToggleMeeting));
        assert!(s.sync_meeting(&notes(true, "Alt+N")).is_err());
        assert_eq!(s.dispatch(&key("Alt+Space"), true), Some(Do::Dictate { pressed: true, clean: false }));
        assert!(f.calls().is_empty());
    }

    #[test]
    fn the_meeting_key_toggles_on_press_and_does_nothing_on_release() {
        let (s, _) = setup();
        s.sync_meeting(&notes(true, "Alt+M")).unwrap();
        assert_eq!(s.dispatch(&key("Alt+M"), true), Some(Do::ToggleMeeting));
        assert_eq!(s.dispatch(&key("Alt+M"), false), None);
    }

    #[test]
    fn a_key_press_and_release_run_dictate_or_cleanup() {
        let (s, _) = setup();
        s.sync_voice(&all()).unwrap();
        for k in ["Alt+Space", "Alt+D"] {
            assert_eq!(s.dispatch(&key(k), true), Some(Do::Dictate { pressed: true, clean: false }));
            assert_eq!(s.dispatch(&key(k), false), Some(Do::Dictate { pressed: false, clean: false }));
        }
        for k in ["Alt+C", "Alt+E"] {
            assert_eq!(s.dispatch(&key(k), true), Some(Do::Dictate { pressed: true, clean: true }));
            assert_eq!(s.dispatch(&key(k), false), Some(Do::Dictate { pressed: false, clean: true }));
        }
        assert_eq!(s.dispatch(&key("Alt+Q"), true), None);
    }

    #[test]
    fn fn_as_a_dictate_key_or_a_cleanup_key() {
        let (s, _) = setup();
        s.sync_voice(&voice(true, "Fn", None, None, None)).unwrap();
        assert_eq!(s.dispatch_fn(true), Some(Do::Dictate { pressed: true, clean: false }));
        assert_eq!(s.dispatch_fn(false), Some(Do::Dictate { pressed: false, clean: false }));
        s.sync_voice(&voice(true, "Alt+Space", None, Some("Fn"), None)).unwrap();
        assert_eq!(s.dispatch_fn(true), Some(Do::Dictate { pressed: true, clean: true }));
        assert_eq!(s.dispatch_fn(false), Some(Do::Dictate { pressed: false, clean: true }));
    }

    #[test]
    fn fn_while_paused_tells_settings_on_release_only() {
        let (s, _) = setup();
        s.pause(true, &voice(true, "Fn", None, None, None), &notes(false, "")).0.unwrap();
        assert_eq!(s.dispatch_fn(true), None);
        assert_eq!(s.dispatch_fn(false), Some(Do::TellSettings));
    }

    #[test]
    fn fn_that_is_not_a_key_does_nothing() {
        let (s, _) = setup();
        s.sync_voice(&all()).unwrap();
        assert_eq!((s.dispatch_fn(true), s.dispatch_fn(false)), (None, None));
    }

    #[test]
    fn the_fn_tap_starts_for_fn_keys_and_while_paused_but_not_when_off() {
        let (s, f) = setup();
        s.sync_voice(&all()).unwrap();
        assert_eq!(f.watches(), 0);
        s.sync_voice(&voice(false, "Fn", None, None, None)).unwrap();
        assert_eq!(f.watches(), 0);
        s.sync_voice(&voice(true, "Fn", None, None, None)).unwrap();
        assert_eq!(f.watches(), 1);
        let (s, f) = setup();
        s.pause(true, &all(), &notes(true, "Alt+M")).0.unwrap();
        assert_eq!(f.watches(), 1);
        let (s, f) = setup();
        s.pause(true, &voice(false, "Fn", None, None, None), &notes(false, "")).0.unwrap();
        assert_eq!(f.watches(), 0);
    }
}
