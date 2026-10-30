//! Meetings and their transcript segments, in the same SQLite file as settings.
use super::mute::{Event, Source, State};
use rusqlite::{params, Connection};
use serde::Serialize;

pub const ME: &str = "me";
pub const THEM: &str = "them";

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Segment {
    /// The row id; 0 until stored.
    pub id: i64,
    pub speaker: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    /// Only mic segments are ever anything but unmuted.
    pub mute: State,
    /// The user pulled a collapsed segment into the transcript.
    pub included: bool,
    /// Which speaker on the system track, numbered from 1 within the meeting; None for the mic and for a
    /// meeting whose speakers weren't separated. The name lives in the `speakers` table.
    pub cluster: Option<u32>,
    /// Included by the experimental triage pass rather than by the user.
    pub auto: bool,
}

impl Segment {
    /// In the transcript, the export and anything sent to a provider: lines that were live, and collapsed
    /// lines the user included. Everything that reads a transcript goes through this.
    pub fn shown(&self) -> bool {
        self.mute == State::Unmuted || self.included
    }
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
    /// None means Inbox.
    pub folder_id: Option<i64>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Folder {
    pub id: i64,
    pub name: String,
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
        CREATE INDEX IF NOT EXISTS segments_meeting ON segments (meeting_id, start_ms);
        CREATE TABLE IF NOT EXISTS folders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE COLLATE NOCASE
        );
        CREATE TABLE IF NOT EXISTS mute_events (
            meeting_id INTEGER NOT NULL,
            at_ms INTEGER NOT NULL,
            muted INTEGER NOT NULL,
            source TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS mute_events_meeting ON mute_events (meeting_id, at_ms);",
    )?;
    // Databases from before folders have no column; their meetings stay NULL, which is the Inbox.
    add_column(db, "meetings", "folder_id", "INTEGER")?;
    // Segments from before mute state were all live, so they stay in the transcript.
    add_column(db, "segments", "mute", "TEXT NOT NULL DEFAULT 'unmuted'")?;
    add_column(db, "segments", "included", "INTEGER NOT NULL DEFAULT 0")?;
    // Segments from before speaker separation have no cluster and read as "Them".
    add_column(db, "segments", "cluster", "INTEGER")?;
    add_column(db, "segments", "auto", "INTEGER NOT NULL DEFAULT 0")
}

pub(super) fn add_column(db: &Connection, table: &str, column: &str, ddl: &str) -> rusqlite::Result<()> {
    let has = db.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?.exists([column])?;
    if !has {
        db.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"), [])?;
    }
    Ok(())
}

fn clean(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() { Err("Give the folder a name.".into()) } else { Ok(name) }
}

fn name_error(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation => "A folder with that name already exists.".into(),
        e => e.to_string(),
    }
}

pub fn folders(db: &Connection) -> rusqlite::Result<Vec<Folder>> {
    db.prepare("SELECT id, name FROM folders ORDER BY name COLLATE NOCASE")?
        .query_map([], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)? }))?
        .collect()
}

pub fn create_folder(db: &Connection, name: &str) -> Result<Folder, String> {
    let name = clean(name)?;
    db.execute("INSERT INTO folders (name) VALUES (?1)", [name]).map_err(name_error)?;
    Ok(Folder { id: db.last_insert_rowid(), name: name.into() })
}

pub fn rename_folder(db: &Connection, id: i64, name: &str) -> Result<(), String> {
    db.execute("UPDATE folders SET name = ?2 WHERE id = ?1", params![id, clean(name)?]).map_err(name_error)?;
    Ok(())
}

/// Its meetings go back to the Inbox.
pub fn delete_folder(db: &Connection, id: i64) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("UPDATE meetings SET folder_id = NULL WHERE folder_id = ?1", [id])?;
    tx.execute("DELETE FROM folders WHERE id = ?1", [id])?;
    tx.commit()
}

/// `folder` None files the meeting in the Inbox.
pub fn move_meeting(db: &Connection, id: i64, folder: Option<i64>) -> Result<(), String> {
    if let Some(f) = folder {
        if !db.query_row("SELECT EXISTS (SELECT 1 FROM folders WHERE id = ?1)", [f], |r| r.get(0)).map_err(|e| e.to_string())? {
            return Err("That folder no longer exists.".into());
        }
    }
    db.execute("UPDATE meetings SET folder_id = ?2 WHERE id = ?1", params![id, folder]).map_err(|e| e.to_string())?;
    Ok(())
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

/// Adds a line to the note shown under the title, unless it is already there.
pub fn add_note(db: &Connection, id: i64, note: &str) {
    let _ = db.execute(
        "UPDATE meetings SET note = CASE WHEN note IS NULL OR note = '' THEN ?2 WHEN instr(note, ?2) > 0 THEN note ELSE note || ' ' || ?2 END WHERE id = ?1",
        params![id, note],
    );
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
            "INSERT INTO segments (meeting_id, speaker, start_ms, end_ms, text, mute, included, cluster, auto) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, s.speaker, s.start_ms, s.end_ms, s.text, s.mute.as_str(), s.included, s.cluster, s.auto],
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
    (SELECT COUNT(*) FROM segments s WHERE s.meeting_id = m.id AND s.speaker = 'them'),
    folder_id
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
        folder_id: r.get(11)?,
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

const SEGMENT: &str = "SELECT rowid, speaker, start_ms, end_ms, text, mute, included, meeting_id, cluster, auto FROM segments";

fn segment_row(r: &rusqlite::Row) -> rusqlite::Result<Segment> {
    Ok(Segment {
        id: r.get(0)?,
        speaker: r.get(1)?,
        start_ms: r.get(2)?,
        end_ms: r.get(3)?,
        text: r.get(4)?,
        mute: State::parse(&r.get::<_, String>(5)?),
        included: r.get(6)?,
        cluster: r.get(8)?,
        auto: r.get(9)?,
    })
}

pub fn segments(db: &Connection, id: i64) -> rusqlite::Result<Vec<Segment>> {
    db.prepare(&format!("{SEGMENT} WHERE meeting_id = ?1 ORDER BY start_ms, rowid"))?.query_map([id], segment_row)?.collect()
}

/// One segment and the meeting it belongs to.
pub fn segment(db: &Connection, id: i64) -> rusqlite::Result<(i64, Segment)> {
    db.query_row(&format!("{SEGMENT} WHERE rowid = ?1"), [id], |r| Ok((r.get(7)?, segment_row(r)?)))
}

/// Pulls a collapsed segment into the transcript, or puts it back. Either way it is now the user's call, not triage's.
pub fn include_segment(db: &Connection, id: i64, included: bool) -> rusqlite::Result<()> {
    db.execute("UPDATE segments SET included = ?2, auto = 0 WHERE rowid = ?1", params![id, included])?;
    Ok(())
}

/// The meeting being recorded, if any.
pub fn recording(db: &Connection) -> Option<i64> {
    db.query_row("SELECT id FROM meetings WHERE status = 'recording' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).ok()
}

pub fn add_mute_event(db: &Connection, meeting: i64, e: &Event) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO mute_events (meeting_id, at_ms, muted, source) VALUES (?1, ?2, ?3, ?4)",
        params![meeting, e.at_ms, e.muted, e.source.as_str()],
    )?;
    Ok(())
}

/// The meeting's mute timeline, oldest first.
pub fn mute_events(db: &Connection, meeting: i64) -> rusqlite::Result<Vec<Event>> {
    db.prepare("SELECT at_ms, muted, source FROM mute_events WHERE meeting_id = ?1 ORDER BY at_ms, rowid")?
        .query_map([meeting], |r| Ok(Event { at_ms: r.get(0)?, muted: r.get(1)?, source: Source::parse(&r.get::<_, String>(2)?) }))?
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
        Segment { speaker: speaker.into(), start_ms, end_ms: start_ms + 1000, text: text.into(), ..Default::default() }
    }

    fn muted(start_ms: u64, text: &str, mute: State) -> Segment {
        Segment { mute, ..seg(ME, start_ms, text) }
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
    fn folder_names_are_trimmed_and_unique_ignoring_case() {
        let c = db();
        let acme = create_folder(&c, "  Acme ").unwrap();
        assert_eq!(acme.name, "Acme");
        assert!(create_folder(&c, "acme").is_err());
        assert!(create_folder(&c, "   ").is_err());
        let other = create_folder(&c, "1:1s").unwrap();
        assert!(rename_folder(&c, other.id, "ACME").is_err());
        rename_folder(&c, other.id, "Team").unwrap();
        assert_eq!(folders(&c).unwrap().iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Acme", "Team"]);
    }

    #[test]
    fn a_meeting_is_in_one_folder_and_moves_between_them_and_the_inbox() {
        let c = db();
        let (a, b) = (create_folder(&c, "A").unwrap().id, create_folder(&c, "B").unwrap().id);
        let m = create(&c, 1).unwrap();
        assert_eq!(get(&c, m).unwrap().folder_id, None);
        move_meeting(&c, m, Some(a)).unwrap();
        move_meeting(&c, m, Some(b)).unwrap();
        assert_eq!(get(&c, m).unwrap().folder_id, Some(b));
        move_meeting(&c, m, None).unwrap();
        assert_eq!(get(&c, m).unwrap().folder_id, None);
        assert!(move_meeting(&c, m, Some(999)).is_err());
    }

    #[test]
    fn deleting_a_folder_returns_its_meetings_to_the_inbox() {
        let c = db();
        let (a, b) = (create_folder(&c, "A").unwrap().id, create_folder(&c, "B").unwrap().id);
        let (m1, m2) = (create(&c, 1).unwrap(), create(&c, 2).unwrap());
        move_meeting(&c, m1, Some(a)).unwrap();
        move_meeting(&c, m2, Some(b)).unwrap();
        delete_folder(&c, a).unwrap();
        assert_eq!(get(&c, m1).unwrap().folder_id, None);
        assert_eq!(get(&c, m2).unwrap().folder_id, Some(b));
        assert_eq!(folders(&c).unwrap().len(), 1);
    }

    #[test]
    fn init_migrates_a_database_from_before_folders() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE meetings (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL DEFAULT 'Untitled', started_at INTEGER NOT NULL,
             duration_ms INTEGER NOT NULL DEFAULT 0, app TEXT, note TEXT, status TEXT NOT NULL, progress REAL NOT NULL DEFAULT 0, error TEXT);
             INSERT INTO meetings (started_at, status) VALUES (1, 'done');",
        )
        .unwrap();
        init(&c).unwrap();
        init(&c).unwrap(); // idempotent
        assert_eq!(list(&c).unwrap()[0].folder_id, None);
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

    #[test]
    fn collapsed_segments_keep_their_state_and_only_included_ones_are_shown() {
        let c = db();
        let id = create(&c, 1).unwrap();
        finish(&c, id, 9000, &[muted(0, "live", State::Unmuted), muted(2000, "private", State::Muted), muted(4000, "who knows", State::Unknown)]).unwrap();
        let got = segments(&c, id).unwrap();
        assert_eq!(got.iter().map(|s| (s.mute, s.included, s.shown())).collect::<Vec<_>>(), [
            (State::Unmuted, false, true), (State::Muted, false, false), (State::Unknown, false, false),
        ]);
        include_segment(&c, got[1].id, true).unwrap();
        assert_eq!(segment(&c, got[1].id).unwrap().0, id);
        let after = segments(&c, id).unwrap();
        assert_eq!(after.iter().map(Segment::shown).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!((after[1].mute, after[1].included), (State::Muted, true));
        include_segment(&c, got[1].id, false).unwrap();
        assert!(!segments(&c, id).unwrap()[1].shown());
    }

    #[test]
    fn a_triage_inclusion_is_marked_until_the_user_decides() {
        let c = db();
        let id = create(&c, 1).unwrap();
        finish(&c, id, 9000, &[Segment { included: true, auto: true, ..muted(0, "on topic", State::Muted) }]).unwrap();
        let s = segments(&c, id).unwrap().remove(0);
        assert_eq!((s.shown(), s.auto), (true, true));
        include_segment(&c, s.id, true).unwrap(); // the user confirms it: no longer automatic
        assert!(!segments(&c, id).unwrap()[0].auto);
    }

    #[test]
    fn mute_timeline_is_kept_per_meeting_in_time_order() {
        let c = db();
        let (a, b) = (create(&c, 1).unwrap(), create(&c, 2).unwrap());
        let ev = |at_ms, muted, source| Event { at_ms, muted, source };
        add_mute_event(&c, a, &ev(5000, true, Source::Manual)).unwrap();
        add_mute_event(&c, a, &ev(1000, false, Source::Extension)).unwrap();
        add_mute_event(&c, b, &ev(0, false, Source::Manual)).unwrap();
        assert_eq!(mute_events(&c, a).unwrap(), [ev(1000, false, Source::Extension), ev(5000, true, Source::Manual)]);
        assert_eq!(mute_events(&c, b).unwrap().len(), 1);
        assert_eq!(recording(&c), Some(b));
        set_transcribing(&c, b).unwrap();
        assert_eq!(recording(&c), Some(a));
    }

    #[test]
    fn init_migrates_segments_from_before_mute_state_as_live() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE segments (meeting_id INTEGER NOT NULL, speaker TEXT NOT NULL, start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL, text TEXT NOT NULL);
             INSERT INTO segments VALUES (1, 'me', 0, 1000, 'old line');",
        )
        .unwrap();
        init(&c).unwrap();
        init(&c).unwrap(); // idempotent
        let old = segments(&c, 1).unwrap();
        assert_eq!((old[0].mute, old[0].included, old[0].shown()), (State::Unmuted, false, true));
    }
}
