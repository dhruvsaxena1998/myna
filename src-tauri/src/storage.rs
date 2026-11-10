use rusqlite::Connection;
use std::path::Path;

/// One step per schema version; `open` runs every step above the file's `user_version`.
/// Version 1 is the empty base schema. Append, never edit, a step.
///
/// Every step only adds: new tables, new columns that are nullable or have a DEFAULT. Never drop, rename or
/// reshape what an older Myna reads or writes, because a user's data file outlives the build that made it
/// (they update, a dev build opens the same folder, they go back). `migrations_only_add` enforces this.
/// To retire something, stop using it in one release and drop it, if ever, a release or two later.
const MIGRATIONS: &[&str] = &[
    "",
    "CREATE TABLE active_model(task TEXT PRIMARY KEY, model_id TEXT NOT NULL);",
    "CREATE TABLE setting(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    "CREATE TABLE word(id INTEGER PRIMARY KEY, spelling TEXT NOT NULL, heard TEXT);",
    "CREATE TABLE trace(id INTEGER PRIMARY KEY, started INTEGER NOT NULL, mode TEXT NOT NULL, app TEXT, bundle TEXT, model TEXT,
        audio_ms INTEGER NOT NULL, outcome TEXT NOT NULL, reason TEXT, load_ms REAL, paste_ms REAL);
     CREATE TABLE step(trace INTEGER NOT NULL REFERENCES trace(id), kind TEXT NOT NULL, text TEXT NOT NULL, ms REAL NOT NULL,
        settings TEXT, edits TEXT);",
    "CREATE TABLE provider(id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, name TEXT NOT NULL UNIQUE COLLATE NOCASE, url TEXT NOT NULL);
     CREATE TABLE route(slot TEXT PRIMARY KEY, provider_id INTEGER NOT NULL REFERENCES provider(id), model TEXT NOT NULL);",
    "UPDATE provider SET kind = 'openai' WHERE kind <> 'openrouter';
     CREATE TABLE model_list(provider_id INTEGER PRIMARY KEY REFERENCES provider(id), json TEXT NOT NULL, fetched INTEGER NOT NULL);",
    "CREATE TABLE keep_word(word TEXT PRIMARY KEY COLLATE NOCASE);
     CREATE TABLE feedback(trace INTEGER PRIMARY KEY REFERENCES trace(id), rating TEXT NOT NULL, corrected TEXT, created INTEGER NOT NULL);
     CREATE TABLE fix_dismissed(key TEXT PRIMARY KEY, seen INTEGER NOT NULL);",
    "CREATE TABLE meeting(id INTEGER PRIMARY KEY, started INTEGER NOT NULL, duration_ms INTEGER, source_app TEXT,
        title TEXT NOT NULL DEFAULT 'Untitled meeting', state TEXT NOT NULL);",
    "CREATE TABLE segment(id INTEGER PRIMARY KEY, meeting INTEGER NOT NULL REFERENCES meeting(id), start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL,
        speaker TEXT NOT NULL, text TEXT NOT NULL);
     ALTER TABLE meeting ADD COLUMN failure TEXT;",
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
    // A newer Myna only added to the schema (see MIGRATIONS), so this one reads and writes what it knows and
    // leaves the rest, and leaves the version alone.
    for v in version..SCHEMA_VERSION {
        let step = format!("BEGIN; {} PRAGMA user_version = {}; COMMIT;", MIGRATIONS[v as usize], v + 1);
        conn.execute_batch(&step).map_err(|e| format!("Couldn't set up {}: {e}", db.display()))?;
    }
    Ok(conn)
}

/// Starts `to` (a dev build's data folder) as a snapshot of `from` (the installed app's), the first time only.
/// Read-only on `from`, so testing a migration in dev never touches the installed Myna's data.
pub fn seed(from: &Path, to: &Path) -> Result<(), String> {
    let (src, dst) = (from.join("myna.sqlite"), to.join("myna.sqlite"));
    if dst.exists() || !src.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(|e| format!("Couldn't create {}: {e}", to.display()))?;
    let c = Connection::open_with_flags(&src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| format!("Couldn't read {}: {e}", src.display()))?;
    c.execute("VACUUM INTO ?1", [dst.to_string_lossy()]).map(drop).map_err(|e| format!("Couldn't copy {}: {e}", src.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(dir: &Path) -> i64 {
        Connection::open(dir.join("myna.sqlite")).unwrap()
            .query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn a_dev_folder_starts_as_a_copy_once_and_never_writes_the_original() {
        let tmp = tempfile::tempdir().unwrap();
        let (real, dev) = (tmp.path().join("Myna"), tmp.path().join("Myna Dev"));
        seed(&real, &dev).unwrap(); // nothing to copy yet
        assert!(!dev.exists());
        open(&real).unwrap().execute("INSERT INTO word(spelling) VALUES ('Myna')", []).unwrap();
        let before = std::fs::read(real.join("myna.sqlite")).unwrap();
        seed(&real, &dev).unwrap();
        let words = |d: &Path| open(d).unwrap().query_row("SELECT count(*) FROM word", [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(words(&dev), 1);
        open(&dev).unwrap().execute("INSERT INTO word(spelling) VALUES ('Dev')", []).unwrap();
        seed(&real, &dev).unwrap(); // already there: left alone
        assert_eq!(words(&dev), 2);
        assert_eq!(std::fs::read(real.join("myna.sqlite")).unwrap(), before, "the installed app's file is untouched");
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
    fn a_newer_database_still_opens_and_is_left_as_it_was() {
        let tmp = tempfile::tempdir().unwrap();
        let c = open(tmp.path()).unwrap();
        c.execute("INSERT INTO setting VALUES ('voice_key', 'Alt+Space')", []).unwrap();
        // What a later Myna adds: a table, and a column on an old table.
        c.execute_batch("CREATE TABLE from_the_future(x); ALTER TABLE word ADD COLUMN note TEXT; PRAGMA user_version = 99;").unwrap();
        drop(c);
        let c = open(tmp.path()).unwrap();
        assert_eq!(version(tmp.path()), 99, "never lowered");
        let key: String = c.query_row("SELECT value FROM setting WHERE key = 'voice_key'", [], |r| r.get(0)).unwrap();
        assert_eq!(key, "Alt+Space");
        c.execute("INSERT INTO word(spelling, heard) VALUES ('Myna', NULL)", []).unwrap(); // an older Myna's insert still works
    }

    /// The rule at the top of MIGRATIONS, as a check: an older Myna must keep working on what a newer one wrote.
    #[test]
    fn migrations_only_add() {
        // Step 7 rewrote values in a table beta.6 never had; allowed once, because no older build could read it.
        const REWROTE_UNSEEN_DATA: &[usize] = &[6];
        for (i, step) in MIGRATIONS.iter().enumerate() {
            let sql = step.to_uppercase();
            for bad in ["DROP ", " RENAME ", "DELETE FROM", "PRAGMA USER_VERSION"] {
                assert!(!sql.contains(bad), "migration {} contains `{}`: add, never remove or reshape", i + 1, bad.trim());
            }
            if sql.contains("UPDATE ") {
                assert!(REWROTE_UNSEEN_DATA.contains(&i), "migration {} rewrites existing rows: an older Myna still reads them", i + 1);
            }
            for part in sql.split("ADD COLUMN").skip(1) {
                let def = part.split(';').next().unwrap_or("");
                assert!(!def.contains("NOT NULL") || def.contains("DEFAULT"), "migration {} adds a NOT NULL column with no DEFAULT: an older Myna's inserts would fail", i + 1);
            }
        }
    }

    #[test]
    fn unwritable_location_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-dir");
        std::fs::write(&file, "").unwrap();
        assert!(open(&file.join("Myna")).unwrap_err().starts_with("Couldn't create"));
    }

    #[test]
    fn kinds_collapse_to_two_and_the_model_cache_appears() {
        let tmp = tempfile::tempdir().unwrap();
        let c = Connection::open(tmp.path().join("myna.sqlite")).unwrap();
        let before: String = MIGRATIONS[..6].iter().map(|m| format!("{m};")).collect();
        c.execute_batch(&before).unwrap();
        c.execute_batch("INSERT INTO provider(kind, name, url) VALUES ('ollama','A','http://x'),('custom','B','http://y'),('openrouter','C','http://z'); PRAGMA user_version = 6;").unwrap();
        drop(c);
        let c = open(tmp.path()).unwrap();
        let kinds: String = c.query_row("SELECT group_concat(kind) FROM provider ORDER BY id", [], |r| r.get(0)).unwrap();
        assert_eq!(kinds, "openai,openai,openrouter");
        c.execute("INSERT INTO model_list VALUES (1, '[]', 0)", []).unwrap();
    }
}
