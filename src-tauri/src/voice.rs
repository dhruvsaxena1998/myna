//! Voice typing's own settings: on or off, its shortcut, and whether you hold the shortcut or tap it.

use rusqlite::{Connection, OptionalExtension};
use tauri_plugin_global_shortcut::Shortcut;

pub const MODES: [&str; 2] = ["hold", "tap"];
/// How words are added: through the clipboard, or typed out key by key.
pub const PASTES: [&str; 2] = ["clipboard", "type"];
/// Tap mode stops by itself after this long: one Parakeet call, no chunking.
pub const TAP_LIMIT_SECS: u64 = 300;
/// fn on its own: no accelerator can hold it, so an event tap watches for it (`fnkey`).
pub const FN: &str = "Fn";

fn valid(key: &str) -> bool {
    key == FN || key.parse::<Shortcut>().is_ok()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Voice {
    pub on: bool,
    /// An accelerator like "Alt+Space", the form both the global shortcut and the menu read, or `FN`.
    pub key: String,
    pub mode: String,
    pub paste: String,
}

fn read(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read voice typing's settings: {e}"))
}

pub fn get(conn: &Connection) -> Result<Voice, String> {
    Ok(Voice {
        on: read(conn, "voice_on")?.is_none_or(|v| v == "1"),
        key: read(conn, "voice_key")?.filter(|k| valid(k)).unwrap_or("Alt+Space".into()),
        mode: read(conn, "voice_mode")?.filter(|m| MODES.contains(&m.as_str())).unwrap_or(MODES[0].into()),
        paste: read(conn, "voice_paste")?.filter(|m| PASTES.contains(&m.as_str())).unwrap_or(PASTES[0].into()),
    })
}

pub fn set(conn: &Connection, v: &Voice) -> Result<(), String> {
    if !MODES.contains(&v.mode.as_str()) {
        return Err(format!("{} isn't one of the choices.", v.mode));
    }
    if !PASTES.contains(&v.paste.as_str()) {
        return Err(format!("{} isn't one of the choices.", v.paste));
    }
    if !valid(&v.key) {
        return Err(format!("{} can't be a shortcut.", v.key));
    }
    let on = if v.on { "1" } else { "0" };
    for (k, val) in [("voice_on", on), ("voice_key", &v.key), ("voice_mode", &v.mode), ("voice_paste", &v.paste)] {
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
        let v = |on, key: &str, mode: &str| Voice { on, key: key.into(), mode: mode.into(), paste: "clipboard".into() };
        assert_eq!(get(&conn), Ok(v(true, "Alt+Space", "hold")));
        set(&conn, &v(false, "Ctrl+Shift+KeyD", "tap")).unwrap();
        assert_eq!(get(&conn), Ok(v(false, "Ctrl+Shift+KeyD", "tap")));
        assert!(set(&conn, &v(true, "Alt+Nope", "tap")).is_err());
        assert!(set(&conn, &v(true, "Alt+Space", "toggle")).is_err());
        assert_eq!(get(&conn), Ok(v(false, "Ctrl+Shift+KeyD", "tap")));
        let typed = Voice { paste: "type".into(), ..v(true, "Alt+Space", "hold") };
        set(&conn, &typed).unwrap();
        assert_eq!(get(&conn), Ok(typed.clone()));
        assert!(set(&conn, &Voice { paste: "fax".into(), ..typed.clone() }).is_err());
        assert_eq!(get(&conn), Ok(typed));
    }

    #[test]
    fn fn_alone_is_a_shortcut() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let fn_key = Voice { on: true, key: "Fn".into(), mode: "tap".into(), paste: "clipboard".into() };
        set(&conn, &fn_key).unwrap();
        assert_eq!(get(&conn), Ok(fn_key));
    }
}
