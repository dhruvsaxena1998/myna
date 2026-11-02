//! The dictation pill: a click-through window that never takes focus, in the style and screen spot the user picked.
//!
//! Showing a window through Tauri makes it key, which takes focus from the app being typed into and
//! breaks the paste. So the window is never shown that way: it stays ordered front, transparent and
//! ignoring the mouse, and the page draws the pill only while there's something to show.

use objc2_app_kit::{NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior as B};
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

fn ns(w: &WebviewWindow) -> Option<&NSWindow> {
    // SAFETY: Tauri hands back the window's live NSWindow; it outlives the call.
    w.ns_window().ok().map(|p| unsafe { &*p.cast::<NSWindow>() })
}

/// Puts the pill window on screen for good. Runs on the main thread, in setup.
pub fn setup(w: &WebviewWindow) {
    let Some(ns) = ns(w) else { return };
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

/// Shows "writing", "added" or "idle" (nothing) on the pill.
pub fn show(app: &AppHandle, state: &str) {
    let _ = app.emit_to("pill", "dictation", serde_json::json!({ "state": state }));
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
