//! The dictation pill: a click-through window that never takes focus, in the style and screen spot the user picked.
//!
//! Showing a window through Tauri makes it key, which takes focus from the app being typed into and
//! breaks the paste. So the window is never shown that way: it stays ordered front, transparent and
//! ignoring the mouse, and the page draws the pill only while there's something to show.

use objc2_app_kit::{NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior as B};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use rusqlite::{Connection, OptionalExtension};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

pub const STYLES: [&str; 4] = ["capsule", "orb", "wave", "card"];
pub const PLACES: [&str; 5] = ["bottom", "top", "bottom-right", "top-right", "caret"];
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
    ns.setIgnoresMouseEvents(true);
    ns.setLevel(NSStatusWindowLevel);
    ns.setCollectionBehavior(B::CanJoinAllSpaces | B::FullScreenAuxiliary | B::Stationary | B::IgnoresCycle);
    ns.setHidesOnDeactivate(false);
    ns.orderFrontRegardless();
}

/// Moves the pill window to `place` ("bottom", "top-right", "caret", ...) on the screen the user is typing on.
/// The page sets the pill inside the window against the same edges; for "caret" it's told where (`pill-caret`,
/// None to fall back to bottom centre), before the "listening" that shows it, since both go through the main thread in order.
pub fn place(app: &AppHandle, place: String) {
    let Some(w) = app.get_webview_window("pill") else { return };
    let caret = if place == "caret" { text_cursor() } else { None }; // off the main thread when the caller is
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let (Some(ns), Some(mtm)) = (ns(&w), MainThreadMarker::new()) else { return };
        let size = ns.frame().size;
        if place == "caret" {
            let screens = NSScreen::screens(mtm);
            let primary_h = screens.firstObject().map_or(0.0, |s| s.frame().size.height);
            let frames: Vec<NSRect> = screens.iter().map(|s| s.visibleFrame()).collect();
            let spot = caret.and_then(|c| under(c, primary_h, &frames, size));
            let _ = app.emit_to("pill", "pill-caret", spot.as_ref().map(|(_, c)| c));
            if let Some((at, _)) = spot {
                return ns.setFrameOrigin(at);
            }
        }
        let Some(screen) = NSScreen::mainScreen(mtm) else { return };
        let f = screen.visibleFrame(); // visible: below the menubar, above the Dock
        let x = if place.ends_with("right") { f.origin.x + f.size.width - size.width } else { f.origin.x + (f.size.width - size.width) / 2.0 };
        let y = if place.starts_with("top") { f.origin.y + f.size.height - size.height } else { f.origin.y };
        ns.setFrameOrigin(NSPoint::new(x, y));
    });
}

/// Where the pill is drawn in its window when it's under the text cursor: `x` from the window's left,
/// against the window's top, or its bottom when there's no room below the cursor (`above`).
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
pub struct Caret {
    pub x: f64,
    pub above: bool,
}

/// The pill's inset in its window (`--sp-3` in the pill page), and where it goes against the cursor (the design's 6 pt left, 10 pt below).
const PAD: f64 = 12.0;
const LEFT: f64 = 6.0;
const GAP: f64 = 10.0;

/// Where the pill window goes so the pill shows just under the text cursor, kept on the screen the cursor is on.
/// `caret` is as Accessibility gives it: from the top left of the main screen, y down. Cocoa's `screens` (their
/// visible frames) go from its bottom left, y up; `primary_h` is the main screen's height, to turn one into the other.
/// None when the cursor has no height or is on no screen: a spot that would be wrong.
pub fn under(caret: NSRect, primary_h: f64, screens: &[NSRect], win: NSSize) -> Option<(NSPoint, Caret)> {
    let (x, top) = (caret.origin.x, primary_h - caret.origin.y);
    let bottom = top - caret.size.height;
    let mid = (top + bottom) / 2.0;
    let s = screens.iter().find(|s| {
        (s.origin.x..=s.origin.x + s.size.width).contains(&x) && (s.origin.y..=s.origin.y + s.size.height).contains(&mid)
    });
    let s = s.filter(|_| caret.size.height > 0.0)?;
    let wx = (x - LEFT - PAD).min(s.origin.x + s.size.width - win.width).max(s.origin.x);
    let below = bottom - GAP + PAD - win.height;
    let above = below < s.origin.y;
    let wy = if above { top + GAP - PAD } else { below };
    let wy = wy.min(s.origin.y + s.size.height - win.height).max(s.origin.y);
    Some((NSPoint::new(wx, wy), Caret { x: (x - LEFT - wx).max(PAD), above }))
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementSetMessagingTimeout(el: CFTypeRef, secs: f32) -> i32;
    fn AXUIElementCopyAttributeValue(el: CFTypeRef, attr: CFStringRef, out: *mut CFTypeRef) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(el: CFTypeRef, attr: CFStringRef, param: CFTypeRef, out: *mut CFTypeRef) -> i32;
    fn AXValueGetValue(v: CFTypeRef, kind: u32, out: *mut std::ffi::c_void) -> bool;
}

/// The text cursor's rect in the focused app, read through Accessibility (from the main screen's top left, y down).
/// None when the app doesn't report one: Terminal, many Electron apps, no text field focused.
fn text_cursor() -> Option<NSRect> {
    // Owned CF objects, released on drop; None when the call fails.
    let copy = |el: CFTypeRef, attr: &str, param: Option<CFTypeRef>| -> Option<CFType> {
        let (attr, mut out) = (CFString::new(attr), std::ptr::null());
        // SAFETY: `el` and `param` are live AX objects; on success `out` is a +1 reference we take ownership of.
        let err = unsafe {
            match param {
                Some(p) => AXUIElementCopyParameterizedAttributeValue(el, attr.as_concrete_TypeRef(), p, &mut out),
                None => AXUIElementCopyAttributeValue(el, attr.as_concrete_TypeRef(), &mut out),
            }
        };
        (err == 0 && !out.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(out) })
    };
    // SAFETY: a +1 reference, owned from here.
    let system = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateSystemWide()) };
    // A hung app would otherwise hold up the dictation for AX's default 6 s.
    unsafe { AXUIElementSetMessagingTimeout(system.as_CFTypeRef(), 0.25) };
    let focused = copy(system.as_CFTypeRef(), "AXFocusedUIElement", None)?;
    let range = copy(focused.as_CFTypeRef(), "AXSelectedTextRange", None)?;
    let bounds = copy(focused.as_CFTypeRef(), "AXBoundsForRange", Some(range.as_CFTypeRef()))?;
    let mut r = NSRect::ZERO;
    // SAFETY: kAXValueCGRectType (3) writes a CGRect, which NSRect is.
    unsafe { AXValueGetValue(bounds.as_CFTypeRef(), 3, &mut r as *mut NSRect as *mut _) }.then_some(r)
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

    fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
    }
    const WIN: NSSize = NSSize::new(720.0, 100.0);
    // A 1440×900 laptop screen, menubar 25 pt.
    const H: f64 = 900.0;
    fn laptop() -> NSRect {
        rect(0.0, 0.0, 1440.0, 875.0)
    }

    #[test]
    fn the_pill_sits_just_under_the_text_cursor() {
        // AX: cursor 300 pt down from the top, so its bottom is 900 - 319 = 581 pt up in Cocoa's terms.
        let (at, spot) = under(rect(400.0, 300.0, 2.0, 19.0), H, &[laptop()], WIN).unwrap();
        // The pill's top left goes 6 pt left of the cursor and 10 pt below it, 12 pt inside the window.
        assert_eq!(spot, Caret { x: 12.0, above: false });
        assert_eq!((at.x, at.y), (382.0, 581.0 - 10.0 + 12.0 - 100.0));
    }

    #[test]
    fn near_a_screen_edge_the_window_stays_on_screen() {
        // Right edge: the window stops at the edge; the page keeps the pill inside it, so the pill stays on screen.
        let (at, spot) = under(rect(1430.0, 300.0, 2.0, 19.0), H, &[laptop()], WIN).unwrap();
        assert_eq!((at.x, spot.x), (720.0, 704.0));
        // Left edge: the pill keeps its inset rather than going off screen.
        let (at, spot) = under(rect(2.0, 300.0, 2.0, 19.0), H, &[laptop()], WIN).unwrap();
        assert_eq!((at.x, spot.x), (0.0, 12.0));
        // Bottom edge: no room below, so the pill goes 10 pt above the cursor (top at 900 - 860 = 40 pt).
        let (at, spot) = under(rect(400.0, 860.0, 2.0, 19.0), H, &[laptop()], WIN).unwrap();
        assert!(spot.above);
        assert_eq!(at.y + 12.0, 40.0 + 10.0, "the pill's bottom, 12 pt above the window's");
    }

    #[test]
    fn the_pill_goes_to_the_screen_the_cursor_is_on() {
        // A display to the left of the laptop, and one above it: AX x and y go negative there.
        let left = rect(-1920.0, 100.0, 1920.0, 1055.0);
        let up = rect(0.0, 900.0, 1920.0, 1055.0);
        let (at, _) = under(rect(-1000.0, 300.0, 2.0, 19.0), H, &[laptop(), left, up], WIN).unwrap();
        assert_eq!((at.x, at.y), (-1018.0, 483.0));
        let (at, _) = under(rect(500.0, -500.0, 2.0, 19.0), H, &[laptop(), left, up], WIN).unwrap();
        assert_eq!((at.x, at.y), (482.0, 1400.0 - 19.0 - 10.0 + 12.0 - 100.0));
    }

    #[test]
    fn no_spot_when_the_cursor_is_missing_or_off_every_screen() {
        assert_eq!(under(rect(0.0, 0.0, 0.0, 0.0), H, &[laptop()], WIN), None, "an app that reports an empty rect");
        assert_eq!(under(rect(5000.0, 300.0, 2.0, 19.0), H, &[laptop()], WIN), None);
        assert_eq!(under(rect(400.0, 300.0, 2.0, 19.0), H, &[], WIN), None);
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
        set_look(&conn, "orb", "caret").unwrap();
        assert_eq!(look(&conn), Ok(is("orb", "caret")));
    }
}
