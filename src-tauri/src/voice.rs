//! Voice typing's own settings: on or off, its shortcut, and whether you hold the shortcut or tap it.

use rusqlite::{Connection, OptionalExtension};
use tauri_plugin_global_shortcut::Shortcut;

pub const MODES: [&str; 2] = ["hold", "tap"];
/// Tap mode stops by itself after this long: one Parakeet call, no chunking.
pub const TAP_LIMIT_SECS: u64 = 300;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Voice {
    pub on: bool,
    /// An accelerator like "Alt+Space", the form both the global shortcut and the menu read.
    pub key: String,
    pub mode: String,
}

fn read(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read voice typing's settings: {e}"))
}

pub fn get(conn: &Connection) -> Result<Voice, String> {
    Ok(Voice {
        on: read(conn, "voice_on")?.is_none_or(|v| v == "1"),
        key: read(conn, "voice_key")?.filter(|k| k.parse::<Shortcut>().is_ok()).unwrap_or("Alt+Space".into()),
        mode: read(conn, "voice_mode")?.filter(|m| MODES.contains(&m.as_str())).unwrap_or(MODES[0].into()),
    })
}

pub fn set(conn: &Connection, v: &Voice) -> Result<(), String> {
    if !MODES.contains(&v.mode.as_str()) {
        return Err(format!("{} isn't one of the choices.", v.mode));
    }
    v.key.parse::<Shortcut>().map_err(|_| format!("{} can't be a shortcut.", v.key))?;
    let on = if v.on { "1" } else { "0" };
    for (k, val) in [("voice_on", on), ("voice_key", &v.key), ("voice_mode", &v.mode)] {
        conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, val])
            .map_err(|e| format!("Couldn't save voice typing's settings: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_on_holding_option_space_and_saves_only_valid_choices() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let v = |on, key: &str, mode: &str| Voice { on, key: key.into(), mode: mode.into() };
        assert_eq!(get(&conn), Ok(v(true, "Alt+Space", "hold")));
        set(&conn, &v(false, "Ctrl+Shift+KeyD", "tap")).unwrap();
        assert_eq!(get(&conn), Ok(v(false, "Ctrl+Shift+KeyD", "tap")));
        assert!(set(&conn, &v(true, "Alt+Nope", "tap")).is_err());
        assert!(set(&conn, &v(true, "Alt+Space", "toggle")).is_err());
        assert_eq!(get(&conn), Ok(v(false, "Ctrl+Shift+KeyD", "tap")));
    }
}
