//! The menubar popover: a borderless window under the tray icon, in place of a native tray menu.
//! The backend decides what it shows (`view`); ui/popover.js only renders that.
use crate::{
    meetings::{
        self,
        mute::Mic,
        shots,
        store::{self, Meeting},
    },
    open_window, open_window_at, Db, HotkeyError,
};
use serde::Serialize;
use std::{sync::Mutex, time::{Duration, Instant}};
use tauri::{
    window::{Effect, EffectState, EffectsBuilder},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, Rect, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

const LABEL: &str = "popover";
/// Logical px. The window is the popover: macOS draws its material, corners and shadow.
const WIN_W: f64 = 320.0;
const RADIUS: f64 = 10.0;
/// Between the menubar and the popover, as under Control Center.
const GAP: f64 = 5.0;
const SCREEN_MARGIN: f64 = 8.0;
const RECENT: usize = 3;

#[derive(Default)]
pub struct Popover {
    /// Where the next `popover_fit` shows the window, set by the tray click that asked for it.
    at: Mutex<Option<PhysicalPosition<i32>>>,
    /// When focus last left the popover, so the tray click that caused it is not read as "open again".
    blurred: Mutex<Option<Instant>>,
}

#[derive(Serialize, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum State {
    Idle,
    /// `shots` is filled in by `popover_view`, from the live capture; None while screenshots are off.
    Recording { started_at: i64, app: Option<String>, note: Option<String>, shots: Option<shots::Status> },
    Transcribing { title: String, progress: f64 },
}

#[derive(Serialize, PartialEq, Debug)]
struct Dictation {
    hotkey: String,
    /// Press to start and again to stop, instead of holding.
    toggle: bool,
    /// The hotkey could not be registered.
    error: bool,
}

#[derive(Serialize, PartialEq, Debug)]
struct Recent {
    id: i64,
    title: String,
    started_at: i64,
    status: String,
}

/// What the popover renders; the shape is the contract with ui/popover.js.
#[derive(Serialize, PartialEq, Debug)]
pub struct View {
    state: State,
    /// Offer "Start meeting recording".
    can_start: bool,
    /// Offer the Meetings entry and the recent list.
    meetings: bool,
    dictation: Option<Dictation>,
    /// The mic's state, while a recording is running. Set by `popover_view`, which reads the mute timeline.
    mic: Option<Mic>,
    recent: Vec<Recent>,
}

/// The Dictation line, only when the module is on.
fn dictation(on: bool, hotkey: String, toggle: bool, registered: bool) -> Option<Dictation> {
    on.then_some(Dictation { hotkey, toggle, error: !registered })
}

/// `list` is newest first.
fn view(list: &[Meeting], meetings_on: bool, dictation: Option<Dictation>) -> View {
    let with = |status: &str| list.iter().find(|m| m.status == status);
    // A live recording is always shown and stoppable, even if Meetings was switched off mid-call:
    // hiding Stop would leave the mic open with no way to end it.
    let state = match (with("recording"), with("transcribing")) {
        (Some(m), _) => State::Recording { started_at: m.started_at, app: m.app.clone(), note: m.note.clone(), shots: None },
        (None, Some(m)) if meetings_on => State::Transcribing { title: m.title.clone(), progress: m.progress },
        _ => State::Idle,
    };
    let recent = list
        .iter()
        .filter(|m| meetings_on && m.status != "recording")
        .take(RECENT)
        .map(|m| Recent { id: m.id, title: m.title.clone(), started_at: m.started_at, status: m.status.clone() })
        .collect();
    View {
        can_start: meetings_on && !matches!(state, State::Recording { .. }),
        state,
        meetings: meetings_on,
        dictation,
        mic: None,
        recent,
    }
}

// shortcut: lists every meeting to show three. Add a LIMIT query when histories get long enough to notice.
#[tauri::command]
pub fn popover_view(app: AppHandle, db: tauri::State<Db>, failed: tauri::State<HotkeyError>) -> Result<View, String> {
    // Each helper takes the Db lock itself, so none is held across them.
    let on = |key| crate::setting(&app, key).as_deref() == Some("true");
    let list = store::list(&db.0.lock().unwrap()).map_err(|e| e.to_string())?;
    let registered = failed.0.lock().unwrap().is_none();
    let dictation = dictation(on("dictation_enabled"), crate::hotkey(&app), crate::toggle_mode(&app), registered);
    let mut v = view(&list, on("meetings_enabled"), dictation);
    if let State::Recording { shots, .. } = &mut v.state {
        *shots = crate::meetings::shots::status();
    }
    v.mic = meetings::mic(&db.0.lock().unwrap());
    Ok(v)
}

/// Resizes the window to the page. The first call after a tray click also shows it, so it never
/// appears with last time's content or the wrong height.
#[tauri::command]
pub fn popover_fit(app: AppHandle, height: f64) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let at = app.state::<Popover>().at.lock().unwrap().take();
    if let Some(at) = at {
        let _ = w.set_position(at); // before the size, so the logical size uses the target display's scale
    }
    let _ = w.set_size(LogicalSize::new(WIN_W, height));
    if at.is_some() {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn popover_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// Closes the popover and opens `to`: "meetings" (optionally one meeting), "settings" or "quit".
#[tauri::command]
pub fn popover_go(app: AppHandle, to: String, id: Option<i64>) {
    popover_hide(app.clone());
    match (to.as_str(), id) {
        ("meetings", Some(id)) => {
            let open = app.get_webview_window("meetings").is_some();
            open_window_at(&app, "meetings", &format!("meetings/{id}"), "Meetings", MEETINGS_SIZE);
            if open {
                let _ = app.emit_to("meetings", "open-meeting", id);
            }
        }
        ("meetings", None) => open_window(&app, "meetings", "Meetings", MEETINGS_SIZE),
        ("settings", _) => open_window(&app, "settings", "Myna Settings", (800.0, 560.0)),
        ("quit", _) => app.exit(0),
        _ => {}
    }
}

const MEETINGS_SIZE: (f64, f64) = (1000.0, 680.0);

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    app.manage(Popover::default());
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#popover".into()))
        .inner_size(WIN_W, 200.0)
        .decorations(false)
        .transparent(true)
        // Liquid Glass where macOS has it (26+), the menu material before that, like the system's menubar menus.
        .effects(
            EffectsBuilder::new()
                .effects([Effect::LiquidGlassRegular, Effect::Menu])
                .state(EffectState::Active)
                .radius(RADIUS)
                .build(),
        )
        .shadow(true)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    let handle = app.clone();
    w.on_window_event(move |event| {
        if let WindowEvent::Focused(false) = event {
            *handle.state::<Popover>().blurred.lock().unwrap() = Some(Instant::now());
            popover_hide(handle.clone());
        }
    });
    Ok(())
}

/// A tray click: close the popover if it is open, otherwise ask the page to refresh and show itself.
pub fn toggle(app: &AppHandle, icon: Rect) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let state = app.state::<Popover>();
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
        return;
    }
    if state.blurred.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(250)) {
        return;
    }
    // The tray reports physical pixels, so the scale factor given here is unused.
    let (pos, size): (PhysicalPosition<f64>, PhysicalSize<f64>) = (icon.position.to_physical(1.0), icon.size.to_physical(1.0));
    let centre = pos.x + size.width / 2.0;
    let on_icon = |m: &tauri::Monitor| {
        let (p, s) = (m.position(), m.size());
        (p.x as f64..p.x as f64 + s.width as f64).contains(&centre)
    };
    let mon = app.available_monitors().ok().and_then(|ms| ms.into_iter().find(on_icon));
    let Some(mon) = mon.or_else(|| app.primary_monitor().ok().flatten()) else { return };
    let scale = mon.scale_factor();
    let x = place(centre, WIN_W * scale, mon.position().x as f64, mon.size().width as f64, SCREEN_MARGIN * scale);
    *state.at.lock().unwrap() = Some(PhysicalPosition::new(x as i32, (pos.y + size.height + GAP * scale) as i32));
    let _ = app.emit_to(LABEL, "popover-open", ());
}

/// Centres a window of `win_w` under an icon at `centre`, keeping it `margin` inside the display.
/// Returns the window's left edge. Physical px.
fn place(centre: f64, win_w: f64, screen_x: f64, screen_w: f64, margin: f64) -> f64 {
    (centre - win_w / 2.0).min(screen_x + screen_w - win_w - margin).max(screen_x + margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: i64, status: &str) -> Meeting {
        Meeting {
            id,
            title: format!("Meeting {id}"),
            started_at: 1_000 + id,
            duration_ms: 0,
            app: Some("Google Chrome".into()),
            note: None,
            status: status.into(),
            progress: 0.4,
            error: None,
            segments: 0,
            has_them: true,
            folder_id: None,
        }
    }

    // Newest first, as `store::list` returns them.
    fn history(statuses: &[&str]) -> Vec<Meeting> {
        statuses.iter().enumerate().rev().map(|(i, s)| m(i as i64 + 1, s)).collect()
    }

    #[test]
    fn idle_offers_start_and_the_three_newest_meetings() {
        let v = view(&history(&["done", "done", "done", "failed"]), true, None);
        assert_eq!(v.state, State::Idle);
        assert!(v.can_start && v.meetings);
        assert_eq!(v.recent.iter().map(|r| r.id).collect::<Vec<_>>(), [4, 3, 2]);
        assert_eq!(v.recent[0].status, "failed");
    }

    #[test]
    fn recording_shows_its_clock_source_and_stop_not_start() {
        let v = view(&history(&["done", "recording"]), true, None);
        let State::Recording { started_at, app, .. } = v.state else { panic!("{:?}", v.state) };
        assert_eq!((started_at, app.as_deref()), (1_002, Some("Google Chrome")));
        assert!(!v.can_start);
        // The live meeting is the state block above, not a row in the recent list.
        assert_eq!(v.recent.iter().map(|r| r.id).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn transcribing_shows_progress_and_still_allows_a_new_recording() {
        let v = view(&history(&["transcribing", "done"]), true, None);
        assert_eq!(v.state, State::Transcribing { title: "Meeting 1".into(), progress: 0.4 });
        assert!(v.can_start);
    }

    #[test]
    fn recording_wins_over_transcribing() {
        let v = view(&history(&["transcribing", "recording"]), true, None);
        assert!(matches!(v.state, State::Recording { .. }));
    }

    #[test]
    fn meetings_off_hides_every_meeting_control() {
        let v = view(&history(&["transcribing", "done"]), false, None);
        assert_eq!(v, View { state: State::Idle, can_start: false, meetings: false, dictation: None, mic: None, recent: vec![] });
    }

    #[test]
    fn a_live_recording_stays_stoppable_when_meetings_is_switched_off() {
        let v = view(&history(&["recording"]), false, None);
        assert!(matches!(v.state, State::Recording { .. }));
        assert!(!v.can_start && !v.meetings && v.recent.is_empty());
    }

    #[test]
    fn dictation_shows_only_when_enabled_and_flags_a_dead_hotkey() {
        assert_eq!(dictation(false, "Alt+Space".into(), false, true), None);
        let on = || dictation(true, "Alt+Space".into(), true, true);
        assert_eq!(on(), Some(Dictation { hotkey: "Alt+Space".into(), toggle: true, error: false }));
        assert!(dictation(true, "Alt+Space".into(), false, false).unwrap().error);
        // Dictation has nothing to do with Meetings: its line stays when Meetings is off.
        assert_eq!(view(&[], false, on()).dictation, on());
    }

    #[test]
    fn popover_centres_under_the_icon() {
        // 1440 wide display, 640 px window, icon centre at 1000.
        assert_eq!(place(1000.0, 640.0, 0.0, 2880.0, 16.0), 680.0);
    }

    #[test]
    fn popover_stays_on_the_display() {
        // Icon near the right edge: window pushed left.
        assert_eq!(place(2860.0, 640.0, 0.0, 2880.0, 16.0), 2224.0);
        // Icon near the left edge of a second display starting at x = 2880.
        assert_eq!(place(2900.0, 640.0, 2880.0, 2880.0, 16.0), 2896.0);
    }
}
