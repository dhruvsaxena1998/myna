use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;
    // Per-model prices the user used to type; the cost now comes from the model (prices.rs).
    db.execute("DELETE FROM settings WHERE key LIKE 'ai_in:%' OR key LIKE 'ai_out:%'", [])?;
    Ok(db)
}

pub fn get(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

pub fn set(db: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_survive_reopen() {
        let path = std::env::temp_dir().join(format!("myna-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let db = open(&path).unwrap();
            assert_eq!(get(&db, "meetings_enabled"), None);
            set(&db, "meetings_enabled", "true").unwrap();
            set(&db, "meetings_enabled", "false").unwrap();
            set(&db, "ai_in:m", "1").unwrap();
            set(&db, "ai_out:m", "1").unwrap();
            set(&db, "ai_ctx:m", "1").unwrap();
        }
        let db = open(&path).unwrap();
        assert_eq!(get(&db, "meetings_enabled").as_deref(), Some("false"));
        // Old typed prices are dropped on open; other per-model settings stay.
        assert_eq!((get(&db, "ai_in:m"), get(&db, "ai_out:m"), get(&db, "ai_ctx:m").as_deref()), (None, None, Some("1")));
        let _ = std::fs::remove_file(&path);
    }
}
