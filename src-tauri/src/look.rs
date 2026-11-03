//! Look & appearance: light or dark, the highlight colour, and clear or tinted see-through panels.
//! The pages set them as `<html data-theme data-accent data-glass>`; design/tokens.css does the rest.

use objc2_app_kit::{NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
use objc2_foundation::MainThreadMarker;
use rusqlite::{Connection, OptionalExtension};

pub const THEMES: [&str; 3] = ["auto", "light", "dark"];
pub const ACCENTS: [&str; 8] = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];
pub const GLASSES: [&str; 2] = ["clear", "tinted"];

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Look {
    pub theme: String,
    pub accent: String,
    pub glass: String,
}

fn read(conn: &Connection, key: &str, choices: &[&str]) -> Result<String, String> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read Myna's look: {e}"))?;
    Ok(saved.filter(|v| choices.contains(&v.as_str())).unwrap_or(choices[0].into()))
}

pub fn get(conn: &Connection) -> Result<Look, String> {
    Ok(Look { theme: read(conn, "look_theme", &THEMES)?, accent: read(conn, "look_accent", &ACCENTS)?, glass: read(conn, "look_glass", &GLASSES)? })
}

pub fn set(conn: &Connection, l: &Look) -> Result<(), String> {
    if !THEMES.contains(&l.theme.as_str()) || !ACCENTS.contains(&l.accent.as_str()) || !GLASSES.contains(&l.glass.as_str()) {
        return Err(format!("{} {} {} isn't one of the choices.", l.theme, l.accent, l.glass));
    }
    for (k, v) in [("look_theme", &l.theme), ("look_accent", &l.accent), ("look_glass", &l.glass)] {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn is(t: &str, a: &str, g: &str) -> Look {
        Look { theme: t.into(), accent: a.into(), glass: g.into() }
    }

    #[test]
    fn defaults_to_auto_blue_clear_and_saves_only_listed_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(get(&conn), Ok(is("auto", "blue", "clear")));
        set(&conn, &is("dark", "graphite", "tinted")).unwrap();
        assert_eq!(get(&conn), Ok(is("dark", "graphite", "tinted")));
        assert!(set(&conn, &is("dim", "blue", "clear")).is_err());
        assert!(set(&conn, &is("light", "teal", "clear")).is_err());
        assert!(set(&conn, &is("light", "blue", "frosted")).is_err());
        assert_eq!(get(&conn), Ok(is("dark", "graphite", "tinted")));
    }

    #[test]
    fn unknown_saved_values_fall_back_to_the_default() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        conn.execute_batch("INSERT INTO setting VALUES ('look_theme', 'dim'), ('look_accent', 'teal'), ('look_glass', 'tinted')").unwrap();
        assert_eq!(get(&conn), Ok(is("auto", "blue", "tinted")));
    }
}
