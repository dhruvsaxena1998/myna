//! Meetings and their transcript segments, in the same SQLite file as settings.
use rusqlite::{params, Connection};
use serde::Serialize;

pub const ME: &str = "me";
pub const THEM: &str = "them";

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Segment {
    pub speaker: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: i64,
    pub duration_ms: u64,
    /// App whose audio was tapped; None when only the mic was recorded.
    pub app: Option<String>,
    /// Why system audio is missing, when it is.
    pub note: Option<String>,
    /// recording | transcribing | done | failed
    pub status: String,
    pub progress: f64,
    pub error: Option<String>,
    pub segments: u32,
    pub has_them: bool,
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS meetings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL DEFAULT 'Untitled',
            started_at INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            app TEXT,
            note TEXT,
            status TEXT NOT NULL,
            progress REAL NOT NULL DEFAULT 0,
            error TEXT
        );
        CREATE TABLE IF NOT EXISTS segments (
            meeting_id INTEGER NOT NULL,
            speaker TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            text TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS segments_meeting ON segments (meeting_id, start_ms);",
    )
}

pub fn create(db: &Connection, started_at: i64) -> rusqlite::Result<i64> {
    db.execute("INSERT INTO meetings (started_at, status) VALUES (?1, 'recording')", [started_at])?;
    Ok(db.last_insert_rowid())
}

pub fn delete(db: &Connection, id: i64) {
    let _ = db.execute("DELETE FROM meetings WHERE id = ?1", [id]);
}

pub fn set_source(db: &Connection, id: i64, app: Option<&str>, note: Option<&str>) -> rusqlite::Result<()> {
    db.execute("UPDATE meetings SET app = ?2, note = ?3 WHERE id = ?1", params![id, app, note])?;
    Ok(())
}

/// Queues (or re-queues) transcription; clears any earlier error.
pub fn set_transcribing(db: &Connection, id: i64) -> rusqlite::Result<()> {
    db.execute("UPDATE meetings SET status = 'transcribing', progress = 0, error = NULL WHERE id = ?1", [id])?;
    Ok(())
}

pub fn set_progress(db: &Connection, id: i64, progress: f64) {
    let _ = db.execute("UPDATE meetings SET progress = ?2 WHERE id = ?1", params![id, progress]);
}

pub fn fail(db: &Connection, id: i64, error: &str) {
    let _ = db.execute("UPDATE meetings SET status = 'failed', error = ?2 WHERE id = ?1", params![id, error]);
}

pub fn finish(db: &Connection, id: i64, duration_ms: u64, segments: &[Segment]) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("DELETE FROM segments WHERE meeting_id = ?1", [id])?;
    for s in segments {
        tx.execute(
            "INSERT INTO segments (meeting_id, speaker, start_ms, end_ms, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, s.speaker, s.start_ms, s.end_ms, s.text],
        )?;
    }
    tx.execute(
        "UPDATE meetings SET status = 'done', progress = 1, duration_ms = ?2, error = NULL WHERE id = ?1",
        params![id, duration_ms],
    )?;
    tx.commit()
}

const SELECT: &str = "SELECT id, title, started_at, duration_ms, app, note, status, progress, error,
    (SELECT COUNT(*) FROM segments s WHERE s.meeting_id = m.id),
    (SELECT COUNT(*) FROM segments s WHERE s.meeting_id = m.id AND s.speaker = 'them')
    FROM meetings m";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: r.get(0)?,
        title: r.get(1)?,
        started_at: r.get(2)?,
        duration_ms: r.get(3)?,
        app: r.get(4)?,
        note: r.get(5)?,
        status: r.get(6)?,
        progress: r.get(7)?,
        error: r.get(8)?,
        segments: r.get(9)?,
        has_them: r.get::<_, u32>(10)? > 0,
    })
}

pub fn list(db: &Connection) -> rusqlite::Result<Vec<Meeting>> {
    db.prepare(&format!("{SELECT} ORDER BY started_at DESC, id DESC"))?
        .query_map([], row)?
        .collect()
}

pub fn get(db: &Connection, id: i64) -> rusqlite::Result<Meeting> {
    db.query_row(&format!("{SELECT} WHERE id = ?1"), [id], row)
}

pub fn segments(db: &Connection, id: i64) -> rusqlite::Result<Vec<Segment>> {
    db.prepare("SELECT speaker, start_ms, end_ms, text FROM segments WHERE meeting_id = ?1 ORDER BY start_ms, rowid")?
        .query_map([id], |r| {
            Ok(Segment { speaker: r.get(0)?, start_ms: r.get(1)?, end_ms: r.get(2)?, text: r.get(3)? })
        })?
        .collect()
}

/// Meetings left mid-recording or mid-transcription by a crash or force-quit. Their audio is still
/// on disk (track headers are refreshed while recording), so they go back in the transcription queue.
pub fn recover(db: &Connection) -> rusqlite::Result<Vec<i64>> {
    let ids = db
        .prepare("SELECT id FROM meetings WHERE status IN ('recording', 'transcribing') ORDER BY id")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    for id in &ids {
        set_transcribing(db, *id)?;
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        c
    }

    fn seg(speaker: &str, start_ms: u64, text: &str) -> Segment {
        Segment { speaker: speaker.into(), start_ms, end_ms: start_ms + 1000, text: text.into() }
    }

    #[test]
    fn finished_meeting_lists_with_its_segments_in_time_order() {
        let c = db();
        let id = create(&c, 100).unwrap();
        finish(&c, id, 5000, &[seg(THEM, 2000, "b"), seg(ME, 0, "a")]).unwrap();
        let m = get(&c, id).unwrap();
        assert_eq!((m.status.as_str(), m.segments, m.has_them, m.duration_ms), ("done", 2, true, 5000));
        let texts: Vec<_> = segments(&c, id).unwrap().into_iter().map(|s| s.text).collect();
        assert_eq!(texts, ["a", "b"]);
        // Re-running replaces the old transcript instead of doubling it.
        finish(&c, id, 5000, &[seg(ME, 0, "only")]).unwrap();
        assert_eq!(segments(&c, id).unwrap().len(), 1);
    }

    #[test]
    fn list_is_newest_first() {
        let c = db();
        let old = create(&c, 100).unwrap();
        let new = create(&c, 200).unwrap();
        assert_eq!(list(&c).unwrap().iter().map(|m| m.id).collect::<Vec<_>>(), [new, old]);
    }

    #[test]
    fn recovery_requeues_only_unfinished_meetings() {
        let c = db();
        let recording = create(&c, 1).unwrap();
        let transcribing = create(&c, 2).unwrap();
        set_transcribing(&c, transcribing).unwrap();
        let done = create(&c, 3).unwrap();
        finish(&c, done, 1, &[]).unwrap();
        let failed = create(&c, 4).unwrap();
        fail(&c, failed, "no model");

        assert_eq!(recover(&c).unwrap(), [recording, transcribing]);
        assert_eq!(get(&c, recording).unwrap().status, "transcribing");
        assert_eq!(get(&c, done).unwrap().status, "done");
        // A failed meeting waits for an explicit retry.
        assert_eq!(get(&c, failed).unwrap().status, "failed");
    }
}
