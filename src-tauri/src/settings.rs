use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.execute(
        "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;
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
        }
        let db = open(&path).unwrap();
        assert_eq!(get(&db, "meetings_enabled").as_deref(), Some("false"));
        let _ = std::fs::remove_file(&path);
    }
}
