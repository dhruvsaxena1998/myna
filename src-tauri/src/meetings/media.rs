//! A meeting's media on disk: audio tracks, window video and screenshots. How much it weighs, deleting it, and the
//! retention rule. Only these three folders are ever touched. Transcripts, summaries and chats live in SQLite
//! and nothing here deletes them.
use super::store;
use rusqlite::Connection;
use std::{fs, io::ErrorKind, path::Path};

/// The folders media lives in, as `audio/`, `video/` and `screenshots/` under the data directory.
pub type Dirs<'a> = [&'a Path; 3];

const DAY: i64 = 86_400;

/// "Delete after N days" from the setting: None (keep everything) when it is unset, 0 or not a number.
pub fn retention_days(setting: Option<String>) -> Option<i64> {
    setting?.parse().ok().filter(|d| (1..=3650).contains(d))
}

/// Finished meetings that started `days` or more ago. A meeting that is recording, or waiting for its audio to be
/// transcribed, or that failed and can still be retried, keeps its media: the audio is the only copy of what was said.
pub fn expired(db: &Connection, now: i64, days: i64) -> rusqlite::Result<Vec<i64>> {
    db.prepare("SELECT id FROM meetings WHERE status = 'done' AND started_at <= ?1 ORDER BY id")?
        .query_map([now - days * DAY], |r| r.get(0))?
        .collect()
}

/// Bytes of media a meeting has on disk.
pub fn size(dirs: Dirs, id: i64) -> u64 {
    let files = dirs.iter().filter_map(|d| fs::read_dir(d.join(id.to_string())).ok()).flatten().flatten();
    files.filter_map(|f| f.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

/// Removes a meeting's audio, video and screenshots, and the screenshot rows that pointed at them.
pub fn delete(db: &Connection, dirs: Dirs, id: i64) -> Result<(), String> {
    for dir in dirs {
        let dir = dir.join(id.to_string());
        match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != ErrorKind::NotFound => return Err(format!("{}: {e}", dir.display())),
            _ => {}
        }
    }
    db.execute("DELETE FROM screenshots WHERE meeting_id = ?1", [id]).map_err(|e| e.to_string())?;
    Ok(())
}

/// The retention job: deletes the media of every meeting past `days`. Returns the meetings it cleared.
pub fn sweep(db: &Connection, dirs: Dirs, now: i64, days: i64) -> Vec<i64> {
    let ids = expired(db, now, days).unwrap_or_default();
    ids.into_iter().filter(|&id| size(dirs, id) > 0 && delete(db, dirs, id).is_ok()).collect()
}

/// The per-meeting action: refused while the audio is still needed.
pub fn delete_one(db: &Connection, dirs: Dirs, id: i64) -> Result<(), String> {
    if matches!(store::get(db, id).map_err(|e| e.to_string())?.status.as_str(), "recording" | "transcribing") {
        return Err("This meeting is still being recorded or transcribed, and needs its audio until that finishes.".into());
    }
    delete(db, dirs, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meetings::{chat, shots, store::Segment, summary};
    use std::path::PathBuf;

    struct Disk {
        root: PathBuf,
        audio: PathBuf,
        video: PathBuf,
        shots: PathBuf,
    }

    impl Disk {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("myna-media-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            Self { audio: root.join("audio"), video: root.join("video"), shots: root.join("screenshots"), root }
        }
        fn dirs(&self) -> Dirs<'_> {
            [&self.audio, &self.video, &self.shots]
        }
        /// Puts a file of `len` bytes in each media folder for meeting `id`.
        fn media(&self, id: i64, len: usize) {
            for (dir, file) in self.dirs().into_iter().zip(["mic.wav", "window.mp4", "0.jpg"]) {
                fs::create_dir_all(dir.join(id.to_string())).unwrap();
                fs::write(dir.join(id.to_string()).join(file), vec![0u8; len]).unwrap();
            }
        }
        fn has(&self, id: i64) -> bool {
            self.dirs().iter().any(|d| d.join(id.to_string()).exists())
        }
    }

    impl Drop for Disk {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
        store::init(&c).unwrap();
        shots::init(&c).unwrap();
        summary::init(&c).unwrap();
        chat::init(&c).unwrap();
        c
    }

    /// A meeting started at `at` in the given state, with a transcript, a summary and a screenshot row.
    fn meeting(c: &Connection, at: i64, status: &str) -> i64 {
        let id = store::create(c, at).unwrap();
        match status {
            "done" => store::finish(c, id, 1000, &[Segment { text: "Ship it.".into(), ..Default::default() }]).unwrap(),
            "transcribing" => store::set_transcribing(c, id).unwrap(),
            "failed" => store::fail(c, id, "no model"),
            _ => {}
        }
        c.execute("INSERT INTO summaries VALUES (?1, 'The summary.', 'p', 'm', 'x', 1, 1, NULL, 0, 1)", [id]).unwrap();
        c.execute("INSERT INTO screenshots VALUES (?1, 0, '0.jpg')", [id]).unwrap();
        c.execute("INSERT INTO chat_turns (meeting_id, question, answer, model, tokens_in, tokens_out) VALUES (?1, 'Who owns it?', 'Priya.', 'm', 1, 1)", [id]).unwrap();
        id
    }

    /// What must survive: transcript segments, whether the summary and chat are still there, and screenshot rows.
    fn kept(c: &Connection, id: i64) -> (usize, bool, i64) {
        let chat: i64 = c.query_row("SELECT COUNT(*) FROM chat_turns WHERE meeting_id = ?1", [id], |r| r.get(0)).unwrap();
        assert_eq!(chat, 1, "meeting {id}'s chat was deleted");
        (
            store::segments(c, id).unwrap().len(),
            c.query_row("SELECT COUNT(*) FROM summaries WHERE meeting_id = ?1", [id], |r| r.get::<_, i64>(0)).unwrap() == 1,
            c.query_row("SELECT COUNT(*) FROM screenshots WHERE meeting_id = ?1", [id], |r| r.get(0)).unwrap(),
        )
    }

    #[test]
    fn the_setting_is_a_whole_number_of_days_or_nothing() {
        let s = |v: &str| Some(v.to_string());
        assert_eq!(retention_days(None), None);
        assert_eq!(retention_days(s("")), None);
        assert_eq!(retention_days(s("0")), None); // 0 means keep forever
        assert_eq!(retention_days(s("-3")), None);
        assert_eq!(retention_days(s("soon")), None);
        assert_eq!(retention_days(s("1")), Some(1));
        assert_eq!(retention_days(s("30")), Some(30));
        assert_eq!(retention_days(s("99999")), None);
    }

    #[test]
    fn only_finished_meetings_at_least_n_days_old_are_selected() {
        let c = db();
        let now = 100 * DAY;
        let old = meeting(&c, now - 8 * DAY, "done");
        let edge = meeting(&c, now - 7 * DAY, "done"); // exactly 7 days is old enough
        let fresh = meeting(&c, now - 7 * DAY + 1, "done");
        // Their audio is still needed, however old they are.
        for status in ["recording", "transcribing", "failed"] {
            meeting(&c, now - 30 * DAY, status);
        }
        assert_eq!(expired(&c, now, 7).unwrap(), [old, edge]);
        assert_eq!(expired(&c, now, 1).unwrap(), [old, edge, fresh]);
        assert_eq!(expired(&c, now, 31).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn the_sweep_deletes_media_only_and_leaves_transcripts_and_summaries() {
        let (c, disk) = (db(), Disk::new("sweep"));
        let now = 100 * DAY;
        let old = meeting(&c, now - 10 * DAY, "done");
        let fresh = meeting(&c, now - DAY, "done");
        let waiting = meeting(&c, now - 10 * DAY, "transcribing");
        let failed = meeting(&c, now - 10 * DAY, "failed");
        for id in [old, fresh, waiting, failed] {
            disk.media(id, 10);
        }

        assert_eq!(sweep(&c, disk.dirs(), now, 7), [old]);
        assert!(!disk.has(old));
        assert_eq!(kept(&c, old), (1, true, 0)); // transcript and summary stay; the screenshot rows are gone
        for id in [fresh, waiting, failed] {
            assert!(disk.has(id), "meeting {id}'s media was deleted");
            assert_eq!(kept(&c, id).2, 1);
        }
        // A second pass finds nothing left to clear.
        assert_eq!(sweep(&c, disk.dirs(), now, 7), Vec::<i64>::new());
        // And a different age policy picks different meetings.
        assert_eq!(sweep(&c, disk.dirs(), now, 1), [fresh]);
        assert!(!disk.has(fresh));
        assert_eq!(kept(&c, fresh), (1, true, 0));
    }

    #[test]
    fn one_meetings_media_goes_and_nothing_else_does() {
        let (c, disk) = (db(), Disk::new("one"));
        let (a, b) = (meeting(&c, 1, "done"), meeting(&c, 2, "done"));
        disk.media(a, 100);
        disk.media(b, 7);
        assert_eq!(size(disk.dirs(), a), 300);
        assert_eq!(size(disk.dirs(), 99), 0);

        delete_one(&c, disk.dirs(), a).unwrap();
        assert_eq!((disk.has(a), size(disk.dirs(), a)), (false, 0));
        assert_eq!((disk.has(b), size(disk.dirs(), b)), (true, 21));
        assert_eq!(kept(&c, a), (1, true, 0));
        assert_eq!(kept(&c, b), (1, true, 1));
        // Nothing left to delete is not an error.
        delete_one(&c, disk.dirs(), a).unwrap();
    }

    #[test]
    fn media_is_kept_while_the_audio_is_still_needed() {
        let (c, disk) = (db(), Disk::new("busy"));
        let (recording, transcribing) = (meeting(&c, 1, "recording"), meeting(&c, 2, "transcribing"));
        for id in [recording, transcribing] {
            disk.media(id, 10);
            assert!(delete_one(&c, disk.dirs(), id).unwrap_err().contains("still being"));
            assert!(disk.has(id));
        }
        // A failed meeting is the user's call: they can delete it, knowing Retry will not work.
        let failed = meeting(&c, 3, "failed");
        disk.media(failed, 10);
        delete_one(&c, disk.dirs(), failed).unwrap();
        assert!(!disk.has(failed));
    }
}
