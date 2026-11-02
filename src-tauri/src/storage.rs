use rusqlite::Connection;
use std::path::Path;

/// One step per schema version; `open` runs every step above the file's `user_version`.
/// Version 1 is the empty base schema. Append, never edit, a step.
const MIGRATIONS: &[&str] = &[
    "",
    "CREATE TABLE active_model(task TEXT PRIMARY KEY, model_id TEXT NOT NULL);",
];
const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Create the data folder and the SQLite database in it if missing. Safe to call on every launch.
pub fn open(dir: &Path) -> Result<Connection, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let db = dir.join("myna.sqlite");
    let conn = Connection::open(&db).map_err(|e| format!("Couldn't open {}: {e}", db.display()))?;
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| format!("Couldn't read {}: {e}", db.display()))?;
    if version > SCHEMA_VERSION {
        return Err(format!("{} was written by a newer Myna (schema {version}).", db.display()));
    }
    for v in version..SCHEMA_VERSION {
        let step = format!("BEGIN; {} PRAGMA user_version = {}; COMMIT;", MIGRATIONS[v as usize], v + 1);
        conn.execute_batch(&step).map_err(|e| format!("Couldn't set up {}: {e}", db.display()))?;
    }
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(dir: &Path) -> i64 {
        Connection::open(dir.join("myna.sqlite")).unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn first_launch_creates_folder_and_db_second_leaves_them() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Myna");
        open(&dir).unwrap();
        assert_eq!(version(&dir), SCHEMA_VERSION);
        Connection::open(dir.join("myna.sqlite")).unwrap().execute_batch("CREATE TABLE keep(x)").unwrap();
        open(&dir).unwrap();
        let kept: i64 = Connection::open(dir.join("myna.sqlite")).unwrap()
            .query_row("SELECT count(*) FROM sqlite_master WHERE name='keep'", [], |r| r.get(0)).unwrap();
        assert_eq!(kept, 1);
    }

    #[test]
    fn newer_schema_is_an_error_not_a_downgrade() {
        let tmp = tempfile::tempdir().unwrap();
        open(tmp.path()).unwrap();
        Connection::open(tmp.path().join("myna.sqlite")).unwrap().pragma_update(None, "user_version", 99).unwrap();
        assert!(open(tmp.path()).unwrap_err().contains("newer Myna"));
    }

    #[test]
    fn unwritable_location_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-dir");
        std::fs::write(&file, "").unwrap();
        assert!(open(&file.join("Myna")).unwrap_err().starts_with("Couldn't create"));
    }
}
