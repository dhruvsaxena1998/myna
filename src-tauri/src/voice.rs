//! Voice typing's own settings: on or off, its two shortcuts (Dictate, and Dictate with cleanup), and whether you hold or tap them.

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
    /// The Dictate with cleanup shortcut, in the same form; none until the user sets one.
    pub clean: Option<String>,
    /// A second key for Dictate, e.g. one on another keyboard; same form. Older builds never read it.
    #[serde(default)]
    pub key2: Option<String>,
    /// A second key for Dictate with cleanup.
    #[serde(default)]
    pub clean2: Option<String>,
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
        clean: read(conn, "voice_key_clean")?.filter(|k| valid(k)),
        key2: read(conn, "voice_key_2")?.filter(|k| valid(k)),
        clean2: read(conn, "voice_key_clean_2")?.filter(|k| valid(k)),
        mode: read(conn, "voice_mode")?.filter(|m| MODES.contains(&m.as_str())).unwrap_or(MODES[0].into()),
        paste: read(conn, "voice_paste")?.filter(|m| PASTES.contains(&m.as_str())).unwrap_or(PASTES[0].into()),
    })
}

impl Voice {
    /// Every key that's set, with whether it starts Dictate with cleanup: Dictate's first.
    pub fn keys(&self) -> Vec<(String, bool)> {
        [(Some(&self.key), false), (self.key2.as_ref(), false), (self.clean.as_ref(), true), (self.clean2.as_ref(), true)]
            .into_iter()
            .filter_map(|(k, c)| k.map(|k| (k.clone(), c)))
            .collect()
    }

    /// Whether `key` starts Dictate with cleanup (and not plain Dictate).
    pub fn is_clean(&self, key: &str) -> bool {
        self.keys().iter().any(|(k, c)| k == key && *c)
    }
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
    let keys = v.keys();
    for (i, (k, _)) in keys.iter().enumerate() {
        if !valid(k) {
            return Err(format!("{k} can't be a shortcut."));
        }
        // The earlier one names the action the key already belongs to.
        if let Some((_, was_clean)) = keys[..i].iter().find(|(o, _)| o == k) {
            let other = if *was_clean { "Dictate with cleanup" } else { "Dictate" };
            return Err(format!("{k} is already the {other} shortcut. Pick another key."));
        }
    }
    if v.clean2.is_some() && v.clean.is_none() {
        return Err("Set the first Dictate with cleanup key before a second one.".into());
    }
    let on = if v.on { "1" } else { "0" };
    let opt = |o: &Option<String>| o.clone().unwrap_or_default();
    let (clean, key2, clean2) = (opt(&v.clean), opt(&v.key2), opt(&v.clean2));
    for (k, val) in [("voice_on", on), ("voice_key", &v.key), ("voice_key_clean", &clean), ("voice_key_2", &key2), ("voice_key_clean_2", &clean2), ("voice_mode", &v.mode), ("voice_paste", &v.paste)] {
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
        let v = |on, key: &str, mode: &str| Voice { on, key: key.into(), clean: None, key2: None, clean2: None, mode: mode.into(), paste: "clipboard".into() };
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
    fn the_cleanup_shortcut_starts_unassigned_and_never_equals_the_other() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let v = get(&conn).unwrap();
        assert_eq!(v.clean, None);
        let with = |c: Option<&str>| Voice { clean: c.map(Into::into), ..v.clone() };
        set(&conn, &with(Some("Alt+Shift+Space"))).unwrap();
        assert_eq!(get(&conn).unwrap().clean.as_deref(), Some("Alt+Shift+Space"));
        assert!(set(&conn, &with(Some("Alt+Space"))).unwrap_err().contains("already the Dictate shortcut"));
        assert!(set(&conn, &with(Some("Alt+Nope"))).is_err());
        assert_eq!(get(&conn).unwrap().clean.as_deref(), Some("Alt+Shift+Space"), "a refused save changes nothing");
        set(&conn, &with(None)).unwrap();
        assert_eq!(get(&conn).unwrap().clean, None);
    }

    #[test]
    fn fn_alone_is_a_shortcut() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let fn_key = Voice { on: true, key: "Fn".into(), clean: None, key2: None, clean2: None, mode: "tap".into(), paste: "clipboard".into() };
        set(&conn, &fn_key).unwrap();
        assert_eq!(get(&conn), Ok(fn_key));
    }

    #[test]
    fn each_action_takes_a_second_key_and_no_key_is_used_twice() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let v = Voice { key: "Fn".into(), key2: Some("PageDown".into()), clean: Some("Alt+Space".into()), clean2: Some("End".into()), ..get(&conn).unwrap() };
        set(&conn, &v).unwrap();
        assert_eq!(get(&conn), Ok(v.clone()));
        assert!(v.is_clean("End") && v.is_clean("Alt+Space") && !v.is_clean("PageDown") && !v.is_clean("Fn"));
        for (bad, says) in [
            (Voice { key2: Some("Fn".into()), ..v.clone() }, "Fn is already the Dictate shortcut"),
            (Voice { clean2: Some("PageDown".into()), ..v.clone() }, "PageDown is already the Dictate shortcut"),
            (Voice { clean2: Some("Alt+Space".into()), ..v.clone() }, "Alt+Space is already the Dictate with cleanup shortcut"),
            (Voice { key2: Some("Alt+Nope".into()), ..v.clone() }, "can't be a shortcut"),
            (Voice { clean: None, ..v.clone() }, "first Dictate with cleanup key"),
        ] {
            assert!(set(&conn, &bad).unwrap_err().contains(says), "{bad:?}");
        }
        assert_eq!(get(&conn), Ok(v), "a refused save changes nothing");
    }

    #[test]
    fn settings_from_a_build_without_second_keys_still_load() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        for (k, val) in [("voice_key", "Fn"), ("voice_key_clean", "Alt+Space")] {
            conn.execute("INSERT INTO setting(key, value) VALUES (?1, ?2)", [k, val]).unwrap();
        }
        let v = get(&conn).unwrap();
        assert_eq!((v.key2, v.clean2), (None, None));
        // and what the Settings page of that build sends
        let old: Voice = serde_json::from_str(r#"{"on":true,"key":"Fn","clean":null,"mode":"hold","paste":"clipboard"}"#).unwrap();
        assert_eq!((old.key2, old.clean2), (None, None));
    }
}
