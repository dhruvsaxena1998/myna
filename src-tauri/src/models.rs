//! The bundled catalog of on-device models (models/local.json), what's installed, and the active model per task.

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize, Clone)]
pub struct File {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub desc: String,
    pub acc: u8,
    pub spd: u8,
    #[serde(default)]
    pub default: bool,
    pub size: u64,
    /// 0 = any macOS Myna runs on.
    #[serde(default)]
    pub min_macos: u32,
    pub repo: String,
    pub revision: String,
    /// Empty for Desert Ant models: their SDK fetches and verifies its own weights (ticket 03.4).
    pub files: Vec<File>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub runs_here: bool,
}

#[derive(Deserialize)]
struct Catalog {
    stt: Vec<Model>,
    diar: Vec<Model>,
}

#[derive(Serialize)]
pub struct Task {
    pub id: &'static str,
    pub models: Vec<Model>,
    /// None only if no model in the task runs on this macOS.
    pub active: Option<String>,
}

fn catalog() -> Catalog {
    serde_json::from_str(include_str!("../../models/local.json")).expect("models/local.json is valid")
}

fn task_models(task: &str) -> Option<Vec<Model>> {
    let c = catalog();
    match task {
        "stt" => Some(c.stt),
        "diar" => Some(c.diar),
        _ => None,
    }
}

pub fn runs_here(m: &Model, macos: u32) -> bool {
    macos >= m.min_macos
}

pub fn find(id: &str) -> Option<Model> {
    let c = catalog();
    c.stt.into_iter().chain(c.diar).find(|m| m.id == id)
}

/// Every catalog file is in `models/<id>/` at its catalog size. The sha256 is checked when it's downloaded.
fn is_installed(m: &Model, models_dir: &Path) -> bool {
    !m.files.is_empty()
        && m.files.iter().all(|f| {
            std::fs::metadata(models_dir.join(&m.id).join(&f.path)).is_ok_and(|md| md.len() == f.size)
        })
}

pub fn list(conn: &Connection, models_dir: &Path, macos: u32) -> Result<Vec<Task>, String> {
    ["stt", "diar"]
        .into_iter()
        .map(|id| {
            let mut models = task_models(id).unwrap();
            for m in &mut models {
                m.installed = is_installed(m, models_dir);
                m.runs_here = runs_here(m, macos);
            }
            let saved: Option<String> = conn
                .query_row("SELECT model_id FROM active_model WHERE task = ?1", [id], |r| r.get(0))
                .optional()
                .map_err(|e| format!("Couldn't read the chosen models: {e}"))?;
            let usable = |want: &dyn Fn(&Model) -> bool| {
                models.iter().find(|m| m.runs_here && want(m)).map(|m| m.id.clone())
            };
            let active = usable(&|m| Some(&m.id) == saved.as_ref())
                .or_else(|| usable(&|m| m.default))
                .or_else(|| usable(&|_| true));
            Ok(Task { id, models, active })
        })
        .collect()
}

pub fn select(conn: &Connection, task: &str, id: &str, macos: u32) -> Result<(), String> {
    let models = task_models(task).ok_or_else(|| format!("Unknown task {task}"))?;
    let m = models.iter().find(|m| m.id == id).ok_or_else(|| format!("Unknown model {id}"))?;
    if !runs_here(m, macos) {
        return Err(format!("{} needs macOS {}.", m.name, m.min_macos));
    }
    conn.execute(
        "INSERT INTO active_model(task, model_id) VALUES (?1, ?2) ON CONFLICT(task) DO UPDATE SET model_id = ?2",
        [task, id],
    )
    .map_err(|e| format!("Couldn't save the chosen model: {e}"))?;
    Ok(())
}

/// Seconds the speech-to-text model stays loaded after its last use; 0 unloads right away, -1 never.
pub const UNLOAD_CHOICES: [i64; 6] = [0, 60, 300, 900, 3600, -1];
const UNLOAD_DEFAULT: i64 = 300;

pub fn unload_after(conn: &Connection) -> Result<i64, String> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM setting WHERE key = 'stt_unload_secs'", [], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read when to unload the model: {e}"))?;
    Ok(saved.and_then(|v| v.parse().ok()).filter(|v| UNLOAD_CHOICES.contains(v)).unwrap_or(UNLOAD_DEFAULT))
}

pub fn set_unload_after(conn: &Connection, secs: i64) -> Result<(), String> {
    if !UNLOAD_CHOICES.contains(&secs) {
        return Err(format!("{secs} s isn't one of the choices."));
    }
    conn.execute(
        "INSERT INTO setting(key, value) VALUES ('stt_unload_secs', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1",
        [secs.to_string()],
    )
    .map_err(|e| format!("Couldn't save when to unload the model: {e}"))?;
    Ok(())
}

/// Major macOS version, e.g. 15. 0 if unknown, which hides the macOS 15+ models rather than letting them fail later.
pub fn macos_major() -> u32 {
    std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok()?.split('.').next()?.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        (tmp, conn)
    }

    fn active(conn: &Connection, dir: &Path, macos: u32) -> Vec<Option<String>> {
        list(conn, dir, macos).unwrap().into_iter().map(|t| t.active).collect()
    }

    #[test]
    fn unload_time_defaults_to_5_minutes_and_saves_only_listed_choices() {
        let (_tmp, conn) = db();
        assert_eq!(unload_after(&conn), Ok(300));
        set_unload_after(&conn, -1).unwrap();
        assert_eq!(unload_after(&conn), Ok(-1));
        assert!(set_unload_after(&conn, 7).is_err());
        assert_eq!(unload_after(&conn), Ok(-1));
    }

    #[test]
    fn catalog_is_well_formed() {
        for (task, models) in [("stt", catalog().stt), ("diar", catalog().diar)] {
            assert_eq!(models.iter().filter(|m| m.default).count(), 1, "{task}: one default");
            for m in &models {
                assert!((1..=5).contains(&m.acc) && (1..=5).contains(&m.spd), "{}", m.id);
                assert_eq!(m.revision.len(), 40, "{}: pinned commit", m.id);
                if !m.files.is_empty() {
                    assert_eq!(m.files.iter().map(|f| f.size).sum::<u64>(), m.size, "{}: size", m.id);
                }
                for f in &m.files {
                    assert!(f.sha256.len() == 64 && f.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "{}", f.path);
                }
            }
        }
    }

    #[test]
    fn fresh_install_lists_everything_uninstalled_with_defaults() {
        let (tmp, conn) = db();
        let tasks = list(&conn, &tmp.path().join("models"), 15).unwrap();
        assert!(tasks.iter().flat_map(|t| &t.models).all(|m| !m.installed));
        assert_eq!(active(&conn, tmp.path(), 15), [Some("parakeet-tdt-v3".into()), Some("who".into())]);
    }

    #[test]
    fn macos_14_falls_back_from_desert_ant_and_cant_pick_it() {
        let (tmp, conn) = db();
        assert_eq!(active(&conn, tmp.path(), 14), [Some("parakeet-tdt-v3".into()), Some("sortformer".into())]);
        assert!(select(&conn, "diar", "who", 14).unwrap_err().contains("macOS 15"));
    }

    #[test]
    fn choice_persists_across_reopen() {
        let (tmp, conn) = db();
        select(&conn, "stt", "whisper-large-v3-turbo", 15).unwrap();
        select(&conn, "stt", "voz", 15).unwrap();
        drop(conn);
        let conn = crate::storage::open(tmp.path()).unwrap();
        assert_eq!(active(&conn, tmp.path(), 15)[0].as_deref(), Some("voz"));
        assert!(select(&conn, "stt", "nope", 15).is_err());
    }

    #[test]
    fn installed_when_every_file_is_present_at_full_size() {
        let (tmp, conn) = db();
        let dir = tmp.path().join("models/whisper-large-v3-turbo");
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::File::create(dir.join("ggml-large-v3-turbo.bin")).unwrap();
        let installed = |conn: &Connection| {
            list(conn, &tmp.path().join("models"), 15).unwrap()[0].models.iter().find(|m| m.id.starts_with("whisper")).unwrap().installed
        };
        assert!(!installed(&conn), "partial file");
        f.set_len(1624555275).unwrap(); // sparse, no real disk use
        assert!(installed(&conn));
    }
}
