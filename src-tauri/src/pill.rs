//! The dictation pill: a click-through window that never takes focus, in the style and screen spot the user picked.
//!
//! Showing a window through Tauri makes it key, which takes focus from the app being typed into and
//! breaks the paste. So the window is never shown that way: it stays ordered front, transparent and
//! ignoring the mouse, and the page draws the pill only while there's something to show.

use objc2::ClassType;
use objc2_app_kit::{NSEvent, NSPanel, NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior as B, NSWindowStyleMask};
use std::sync::Mutex;
use objc2_foundation::{MainThreadMarker, NSPoint};
use rusqlite::{Connection, OptionalExtension};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

pub const STYLES: [&str; 4] = ["capsule", "orb", "wave", "card"];
pub const PLACES: [&str; 4] = ["bottom", "top", "bottom-right", "top-right"];
/// The hint on every "permission missing" error.
pub const ALLOW: &str = "Allow it in Settings › Voice typing";

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct Look {
    pub style: String,
    pub place: String,
}

fn read(conn: &Connection, key: &str, choices: &[&str]) -> Result<String, String> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read the pill's look: {e}"))?;
    Ok(saved.filter(|v| choices.contains(&v.as_str())).unwrap_or(choices[0].into()))
}

pub fn look(conn: &Connection) -> Result<Look, String> {
    Ok(Look { style: read(conn, "pill_style", &STYLES)?, place: read(conn, "pill_place", &PLACES)? })
}

pub fn set_look(conn: &Connection, style: &str, place: &str) -> Result<(), String> {
    if !STYLES.contains(&style) || !PLACES.contains(&place) {
        return Err(format!("{style} at {place} isn't one of the choices."));
    }
    for (k, v) in [("pill_style", style), ("pill_place", place)] {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v])
            .map_err(|e| format!("Couldn't save the pill's look: {e}"))?;
    }
    Ok(())
}

pub fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
    // SAFETY: Tauri hands back the window's live NSWindow; it outlives the call.
    w.ns_window().ok().map(|p| unsafe { &*p.cast::<NSWindow>() })
}

/// Puts the pill window on screen for good. Runs on the main thread, in setup.
pub fn setup(w: &WebviewWindow) {
    let Some(ns) = ns(w) else { return };
    // A non-activating panel: a click on 👍/👎 leaves the app being typed into frontmost and focused.
    // Same as tauri-nspanel: swap the class, since only an NSPanel honours NonactivatingPanel.
    // SAFETY: raw object_setClass, as tauri-nspanel does: tao's window class is 8 bytes bigger (its own ivar),
    // which objc2's checked set_class refuses. The memory stays allocated; NSPanel just doesn't use it, and the
    // pill is never closed or released.
    unsafe { objc2::ffi::object_setClass((ns as *const NSWindow).cast_mut().cast(), NSPanel::class()) };
    ns.setStyleMask(ns.styleMask() | NSWindowStyleMask::NonactivatingPanel);
    // The swap skips NSPanel's init, and setStyleMask: alone doesn't tell the WindowServer: without this a click
    // still activated Myna (seen: 👎 took focus from the app being typed into). Private, so only if it's there.
    let sel = objc2::sel!(_setPreventsActivation:);
    // SAFETY: plain message sends to the live pill window, on the main thread (setup runs there).
    unsafe {
        let responds: bool = objc2::msg_send![ns, respondsToSelector: sel];
        if responds {
            let _: () = objc2::msg_send![ns, _setPreventsActivation: true];
        } else {
            eprintln!("dictation: the pill can't stop a click activating Myna on this macOS");
        }
    }
    ns.setIgnoresMouseEvents(true);
    ns.setLevel(NSStatusWindowLevel);
    ns.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
    ns.setHidesOnDeactivate(false);
    ns.orderFrontRegardless();
}

/// Moves the pill window to `place` ("bottom", "top-right", ...) on the screen the user is typing on.
/// The page sets the pill inside the window against the same edges.
pub fn place(app: &AppHandle, place: String) {
    let Some(w) = app.get_webview_window("pill") else { return };
    let _ = app.run_on_main_thread(move || {
        let (Some(ns), Some(mtm)) = (ns(&w), MainThreadMarker::new()) else { return };
        let Some(screen) = NSScreen::mainScreen(mtm) else { return };
        let (f, size) = (screen.visibleFrame(), ns.frame().size); // visible: below the menubar, above the Dock
        let x = if place.ends_with("right") { f.origin.x + f.size.width - size.width } else { f.origin.x + (f.size.width - size.width) / 2.0 };
        let y = if place.starts_with("top") { f.origin.y + f.size.height - size.height } else { f.origin.y };
        ns.setFrameOrigin(NSPoint::new(x, y));
    });
}

/// Shows "writing", "cleaning", "added" or "idle" (nothing) on the pill.
pub fn show(app: &AppHandle, state: &str) {
    let _ = app.emit_to("pill", "dictation", serde_json::json!({ "state": state }));
}

/// Shows the meeting recording pill, for a meeting that started at `started` (unix ms), or hides it. `mic` is
/// "live" or "muted" while the browser add-on mirrors the mute button.
pub fn meeting(app: &AppHandle, started: Option<i64>, mic: Option<&str>) {
    let _ = app.emit_to("pill", "meeting", serde_json::json!({ "started": started, "mic": mic }));
}

/// 👍/👎 on the pill for the dictation just saved.
pub fn ask(app: &AppHandle, trace: i64) {
    let _ = app.emit_to("pill", "dictation-ask", trace);
}

/// Where the pill's buttons are, in the window's CSS pixels from its top left: x, y, width, height.
static HIT: Mutex<Option<[f64; 4]>> = Mutex::new(None);

/// The page says where 👍/👎 are (`None` once they're gone). The window takes the mouse only while the
/// pointer is over them, so the rest of it never blocks a click on the app underneath.
#[tauri::command]
pub fn pill_hit(app: AppHandle, rect: Option<[f64; 4]>) {
    let start = std::mem::replace(&mut *HIT.lock().unwrap(), rect).is_none() && rect.is_some();
    if !start {
        return;
    }
    std::thread::spawn(move || loop {
        let r = *HIT.lock().unwrap();
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(w) = a.get_webview_window("pill") else { return };
            let Some(ns) = ns(&w) else { return };
            let ignore = !r.is_some_and(|r| inside(r, NSEvent::mouseLocation(), ns.frame()));
            if ns.ignoresMouseEvents() != ignore {
                ns.setIgnoresMouseEvents(ignore);
                // The page can't see the pointer leave: the window stops taking the mouse first. So this tells it.
                let _ = a.emit_to("pill", "pill-hover", !ignore);
            }
        });
        if r.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    });
}

/// Whether the pointer (screen points, from the bottom left) is over `r` (CSS pixels from the window's top left).
fn inside(r: [f64; 4], at: NSPoint, frame: objc2_foundation::NSRect) -> bool {
    let (x, y) = (at.x - frame.origin.x, frame.size.height - (at.y - frame.origin.y));
    x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
}

/// Shows "Added without cleanup": the words went in as the rules left them, and `reason` says why.
pub fn fallback(app: &AppHandle, reason: &str) {
    let _ = app.emit_to("pill", "dictation", serde_json::json!({ "state": "fallback", "reason": reason }));
}

/// Shows "listening": `hold` says let go of `key` to paste; otherwise press it again, before `limit` seconds run out.
pub fn listening(app: &AppHandle, hold: bool, key: &str, limit: u64) {
    let _ = app.emit_to("pill", "dictation", serde_json::json!({ "state": "listening", "hold": hold, "key": key, "limit": limit }));
}

/// Shows an error: what went wrong, and what to do about it.
pub fn error(app: &AppHandle, title: &str, hint: &str) {
    let _ = app.emit_to("pill", "dictation", serde_json::json!({ "state": "error", "title": title, "hint": hint }));
}

/// The mic loudness for the level bars, 0 to 1.
pub fn level(app: &AppHandle, level: f32) {
    let _ = app.emit_to("pill", "dictation-level", level);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_is_inside_only_over_the_buttons() {
        use objc2_foundation::{NSRect, NSSize};
        // A 720×100 window at (100, 50); the buttons 40×20 at 10 px from its top left.
        let frame = NSRect::new(NSPoint::new(100.0, 50.0), NSSize::new(720.0, 100.0));
        let r = [10.0, 10.0, 40.0, 20.0];
        assert!(inside(r, NSPoint::new(120.0, 130.0), frame)); // 20 in, 20 down
        assert!(!inside(r, NSPoint::new(120.0, 60.0), frame)); // near the bottom
        assert!(!inside(r, NSPoint::new(200.0, 130.0), frame)); // right of them
    }

    #[test]
    fn look_defaults_to_capsule_at_the_bottom_and_saves_only_listed_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let is = |s: &str, p: &str| Look { style: s.into(), place: p.into() };
        assert_eq!(look(&conn), Ok(is("capsule", "bottom")));
        set_look(&conn, "card", "top-right").unwrap();
        assert_eq!(look(&conn), Ok(is("card", "top-right")));
        assert!(set_look(&conn, "huge", "top").is_err());
        assert!(set_look(&conn, "orb", "middle").is_err());
        assert_eq!(look(&conn), Ok(is("card", "top-right")));
    }
}
