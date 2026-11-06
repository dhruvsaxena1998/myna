//! Look & appearance: light or dark, the highlight colour, and clear or tinted see-through panels.
//! The pages set them as `<html data-theme data-accent data-glass>`; design/tokens.css does the rest.
//! Plus Liquid Glass (experimental): the pill on macOS's own glass, macOS 26 or later.

use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
    NSApplication, NSAutoresizingMaskOptions as M, NSColor, NSGlassEffectView, NSGlassEffectViewStyle, NSVisualEffectView,
    NSWindowOrderingMode,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use rusqlite::{Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

pub const THEMES: [&str; 3] = ["auto", "light", "dark"];
pub const ACCENTS: [&str; 8] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];
pub const GLASSES: [&str; 2] = ["clear", "tinted"];

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Look {
    pub theme: String,
    pub accent: String,
    pub glass: String,
    /// None below macOS 26, where there's no Liquid Glass and the switch isn't shown.
    pub liquid: Option<bool>,
    /// Experimental features' master switch: off turns every experimental feature off, keeping their own switches.
    // shortcut: experimental switches travel with the look; give them their own module when the main window lands and Stats for nerds moves there.
    pub experimental: bool,
    /// Stats for nerds' own switch, off by default; its page shows only while this and the master switch are on.
    pub nerds: bool,
    /// Liquid Glass opacity: how strongly the CSS fill frosts the glass, 0 clear to 100 most frosted.
    #[serde(default = "default_frost")]
    pub frost: u8,
}

fn default_frost() -> u8 {
    50
}

/// Liquid Glass shows only while both it and experimental features are on.
pub fn glass_on(l: &Look) -> bool {
    l.experimental && l.liquid == Some(true)
}

/// Each accent's fill, the same as `--accent` in design/tokens.css, in ACCENTS order.
const ACCENT_RGB: [u32; 8] = [0x0A64D8, 0x8E44AD, 0xC2185B, 0xC42B1C, 0xB35000, 0xFFCC00, 0x1E7B34, 0x5E5E63];

pub type Rgba = (u8, u8, u8, u8);

/// What Liquid Glass is tinted with: the highlight colour when See-through panels is Tinted, else nothing.
pub fn tint(l: &Look) -> Option<Rgba> {
    let i = ACCENTS.iter().position(|a| *a == l.accent).filter(|_| l.glass == "tinted")?;
    let [_, r, g, b] = ACCENT_RGB[i].to_be_bytes();
    Some((r, g, b, 51)) // shortcut: 20% picked by eye on a dark and a light desktop; tune with more wallpapers
}

/// NSGlassEffectView arrived in macOS 26.
pub fn liquid_ok(macos: u32) -> bool {
    macos >= 26
}

fn read(conn: &Connection, key: &str, choices: &[&str]) -> Result<String, String> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read Myna's look: {e}"))?;
    Ok(saved.filter(|v| choices.contains(&v.as_str())).unwrap_or(choices[0].into()))
}

fn saved_frost(conn: &Connection) -> Result<u8, String> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = 'look_frost'", [], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read Myna's look: {e}"))?;
    Ok(saved.and_then(|v| v.parse().ok()).filter(|v| *v <= 100).unwrap_or_else(default_frost))
}

pub fn get(conn: &Connection, macos: u32) -> Result<Look, String> {
    Ok(Look {
        theme: read(conn, "look_theme", &THEMES)?,
        accent: read(conn, "look_accent", &ACCENTS)?,
        glass: read(conn, "look_glass", &GLASSES)?,
        liquid: liquid_ok(macos).then(|| read(conn, "look_liquid", &["off", "on"])).transpose()?.map(|v| v == "on"),
        experimental: read(conn, "experimental", &["off", "on"])? == "on",
        nerds: read(conn, "exp_nerds", &["off", "on"])? == "on",
        frost: saved_frost(conn)?,
    })
}

pub fn set(conn: &Connection, l: &Look, macos: u32) -> Result<(), String> {
    if !THEMES.contains(&l.theme.as_str()) || !ACCENTS.contains(&l.accent.as_str()) || !GLASSES.contains(&l.glass.as_str()) {
        return Err(format!("{} {} {} isn't one of the choices.", l.theme, l.accent, l.glass));
    }
    if l.frost > 100 {
        return Err("Opacity must be between 0 and 100.".into());
    }
    if l.liquid == Some(true) && !liquid_ok(macos) {
        return Err("Liquid Glass needs macOS 26 or later.".into());
    }
    let onoff = |on: bool| if on { "on" } else { "off" }.to_string();
    let (liquid, exp, nerds, frost) = (l.liquid.map(onoff), onoff(l.experimental), onoff(l.nerds), l.frost.to_string());
    let saved = [("look_theme", &l.theme), ("look_accent", &l.accent), ("look_glass", &l.glass), ("experimental", &exp), ("exp_nerds", &nerds), ("look_frost", &frost)];
    for (k, v) in saved.into_iter().chain(liquid.as_ref().map(|v| ("look_liquid", v))) {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v])
            .map_err(|e| format!("Couldn't save Myna's look: {e}"))?;
    }
    Ok(())
}

/// Light or dark for everything native (window vibrancy, the menubar menu); "auto" follows the Mac. Main thread only.
pub fn apply_native(theme: &str) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    // SAFETY: AppKit's appearance name constants are immutable statics.
    let name = match theme {
        "light" => Some(unsafe { NSAppearanceNameAqua }),
        "dark" => Some(unsafe { NSAppearanceNameDarkAqua }),
        _ => None,
    };
    let appearance = name.and_then(NSAppearance::appearanceNamed);
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

/// A rect a page wants glass behind (the pill; the Settings sidebar and main panel), in points from
/// the window's top left.
#[derive(serde::Deserialize, Debug, PartialEq)]
pub struct Glass {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub radius: f64,
    /// False while the pill is hidden, so the glass fades out with it.
    pub shown: bool,
    /// Grows with the window's width (Settings' whole-window sheet); otherwise keeps its width.
    #[serde(default)]
    pub grow: bool,
}

/// Puts macOS's own glass behind each rect in window `label` ("pill" or "settings"); none takes it all away.
/// The pill's glass is sized to the pill (its window is wider). Settings sends its whole window: one sheet of
/// glass in place of the window's vibrancy, which is hidden meanwhile so the glass shows the desktop, not it.
/// Callers gate on macOS 26 (`glass_on`); NSGlassEffectView doesn't exist before it.
pub fn glass(app: &AppHandle, label: &str, rects: Vec<Glass>, tint: Option<Rgba>) {
    let Some(w) = app.get_webview_window(label) else { return };
    let pill = label == "pill";
    let _ = app.run_on_main_thread(move || {
        let (Some(view), Some(mtm)) = (crate::pill::ns(&w).and_then(|ns| ns.contentView()), MainThreadMarker::new()) else { return };
        let fx = view.subviews().iter().find(|s| s.downcast_ref::<NSVisualEffectView>().is_some());
        if let Some(fx) = &fx {
            fx.setHidden(!rects.is_empty());
        }
        let mut have: Vec<_> = view.subviews().iter().filter_map(|v| v.downcast::<NSGlassEffectView>().ok()).collect();
        if have.len() != rects.len() {
            have.drain(..).for_each(|v| v.removeFromSuperview());
        }
        let new = have.is_empty();
        if new {
            for _ in &rects {
                let v = NSGlassEffectView::initWithFrame(mtm.alloc(), NSRect::ZERO);
                // Clear on the pill, like a HUD; Regular on Settings, so its words read on any desktop.
                v.setStyle(if pill { NSGlassEffectViewStyle::Clear } else { NSGlassEffectViewStyle::Regular });
                if pill {
                    // Dark like the HUD it replaces, so its white text reads on any desktop.
                    // SAFETY: AppKit's appearance name constants are immutable statics.
                    v.setAppearance(NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }).as_deref());
                }
                v.setAlphaValue(0.0); // fades in below
                view.addSubview_positioned_relativeTo(&v, NSWindowOrderingMode::Below, None); // behind the page
                have.push(v);
            }
        }
        let c = |v: u8| v as f64 / 255.0;
        let tint = tint.map(|(r, g, b, a)| NSColor::colorWithRed_green_blue_alpha(c(r), c(g), c(b), c(a)));
        for (glass, g) in have.iter().zip(&rects) {
            let top = if view.isFlipped() { g.y } else { view.bounds().size.height - g.y - g.h };
            glass.setFrame(NSRect::new(NSPoint::new(g.x, top), NSSize::new(g.w, g.h)));
            // Pinned to the window's edges so a live resize follows at once; the pill window never resizes.
            let side = if g.grow { M::ViewWidthSizable } else { M::ViewMaxXMargin };
            glass.setAutoresizingMask(if pill { M::empty() } else { M::ViewHeightSizable | side });
            glass.setCornerRadius(g.radius);
            glass.setTintColor(tint.as_deref());
            let alpha = if g.shown { 1.0 } else { 0.0 };
            if new || (glass.alphaValue() - alpha).abs() > 0.01 {
                // Same timing as the page's fade (--dur in, --dur-fast out).
                NSAnimationContext::beginGrouping();
                NSAnimationContext::currentContext().setDuration(if g.shown { 0.18 } else { 0.12 });
                glass.animator().setAlphaValue(alpha);
                NSAnimationContext::endGrouping();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is(t: &str, a: &str, g: &str) -> Look {
        Look { theme: t.into(), accent: a.into(), glass: g.into(), liquid: None, experimental: false, nerds: false, frost: 50 }
    }

    #[test]
    fn defaults_to_auto_blue_clear_and_saves_only_listed_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(get(&conn, 15), Ok(is("auto", "blue", "clear")));
        set(&conn, &is("dark", "graphite", "tinted"), 15).unwrap();
        assert_eq!(get(&conn, 15), Ok(is("dark", "graphite", "tinted")));
        assert!(set(&conn, &is("dim", "blue", "clear"), 15).is_err());
        assert!(set(&conn, &is("light", "teal", "clear"), 15).is_err());
        assert!(set(&conn, &is("light", "blue", "frosted"), 15).is_err());
        assert_eq!(get(&conn, 15), Ok(is("dark", "graphite", "tinted")));
    }

    #[test]
    fn unknown_saved_values_fall_back_to_the_default() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        conn.execute_batch("INSERT INTO setting VALUES ('look_theme', 'dim'), ('look_accent', 'teal'), ('look_glass', 'tinted')").unwrap();
        assert_eq!(get(&conn, 15), Ok(is("auto", "blue", "tinted")));
    }

    #[test]
    fn frost_defaults_to_50_saves_and_rejects_over_100() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(get(&conn, 15).unwrap().frost, 50);
        set(&conn, &Look { frost: 80, ..is("auto", "blue", "clear") }, 15).unwrap();
        assert_eq!(get(&conn, 15).unwrap().frost, 80);
        conn.execute("UPDATE setting SET value = '300' WHERE key = 'look_frost'", []).unwrap();
        assert_eq!(get(&conn, 15).unwrap().frost, 50);
        assert!(set(&conn, &Look { frost: 101, ..is("auto", "blue", "clear") }, 15).is_err());
    }

    #[test]
    fn tint_is_the_highlight_colour_only_when_tinted() {
        assert_eq!(ACCENTS.len(), ACCENT_RGB.len());
        assert_eq!(tint(&is("auto", "green", "tinted")), Some((0x1E, 0x7B, 0x34, 51)));
        assert_eq!(tint(&is("auto", "graphite", "tinted")), Some((0x5E, 0x5E, 0x63, 51)));
        assert_eq!(tint(&is("auto", "green", "clear")), None);
    }

    #[test]
    fn liquid_glass_is_off_by_default_saved_on_26_and_hidden_below() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert!(!liquid_ok(15) && liquid_ok(26) && liquid_ok(27));
        assert_eq!(get(&conn, 26).unwrap().liquid, Some(false));
        set(&conn, &Look { liquid: Some(true), ..is("auto", "blue", "clear") }, 26).unwrap();
        assert_eq!(get(&conn, 27).unwrap().liquid, Some(true));
        assert_eq!(get(&conn, 15).unwrap().liquid, None, "a Mac moved to an older macOS never gets glass");
        // Saving from a Mac without it leaves the switch alone, and can't turn it on.
        set(&conn, &is("dark", "blue", "clear"), 15).unwrap();
        assert_eq!(get(&conn, 26).unwrap(), Look { liquid: Some(true), ..is("dark", "blue", "clear") });
        assert!(set(&conn, &Look { liquid: Some(true), ..is("auto", "blue", "clear") }, 15).is_err());
    }

    #[test]
    fn experimental_features_are_off_by_default_and_gate_liquid_glass() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert!(!get(&conn, 26).unwrap().experimental);
        set(&conn, &Look { liquid: Some(true), ..is("auto", "blue", "clear") }, 26).unwrap();
        assert!(!glass_on(&get(&conn, 26).unwrap()), "Liquid Glass waits for the master switch");
        set(&conn, &Look { liquid: Some(true), experimental: true, ..is("auto", "blue", "clear") }, 26).unwrap();
        assert!(glass_on(&get(&conn, 26).unwrap()));
        set(&conn, &Look { liquid: Some(true), experimental: false, ..is("auto", "blue", "clear") }, 26).unwrap();
        let l = get(&conn, 26).unwrap();
        assert!(!glass_on(&l) && l.liquid == Some(true), "off keeps Liquid Glass's own switch for next time");
        assert!(!glass_on(&Look { experimental: true, ..get(&conn, 15).unwrap() }), "never below macOS 26");
    }

    #[test]
    fn stats_for_nerds_has_its_own_switch_off_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert!(!get(&conn, 15).unwrap().nerds);
        set(&conn, &Look { nerds: true, ..is("auto", "blue", "clear") }, 15).unwrap();
        assert!(get(&conn, 15).unwrap().nerds);
    }
}
