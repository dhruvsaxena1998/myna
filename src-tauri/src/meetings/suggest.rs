//! The folder suggestion chip: after transcription, the provider picks one of the user's folders for a
//! meeting in the Inbox, or says none. The answer is stored so a meeting costs one request.
use super::{
    store, summary,
    transcript::{self, Format},
};
use crate::{
    ai::{self, Config, Message},
    settings, Db,
};
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const OPENING_LINES: usize = 20;

// shortcut: the answer lives in settings under folder_suggestion:<id> and outlives a deleted meeting; move it
// to a column on meetings if that ever matters.
fn key(id: i64) -> String {
    format!("folder_suggestion:{id}")
}

/// What the provider said, as a folder: a name that matches no folder (or "none") is no suggestion.
fn pick(reply: &str, folders: &[store::Folder]) -> Option<store::Folder> {
    let name = reply.trim().trim_matches(|c: char| matches!(c, '"' | '\'' | '.' | '“' | '”')).trim();
    folders.iter().find(|f| f.name.eq_ignore_ascii_case(name)).cloned()
}

/// Asks once per meeting; the lock is not held while the request runs.
fn suggest(db: &Mutex<Connection>, id: i64, cfg: &Config) -> Result<Option<store::Folder>, String> {
    let (meeting, folders, summary, segments) = {
        let conn = db.lock().unwrap();
        if !ai::enabled(&conn, "folders") {
            return Ok(None);
        }
        let meeting = store::get(&conn, id).map_err(|e| e.to_string())?;
        let folders = store::folders(&conn).map_err(|e| e.to_string())?;
        if meeting.status != "done" || meeting.folder_id.is_some() || folders.is_empty() {
            return Ok(None);
        }
        if let Some(said) = settings::get(&conn, &key(id)) {
            return Ok(folders.into_iter().find(|f| f.id.to_string() == said));
        }
        // Muted or excluded speech never leaves the Mac unless the user included it.
        let segments: Vec<_> = store::segments(&conn, id).map_err(|e| e.to_string())?.into_iter().filter(|s| s.shown()).take(OPENING_LINES).collect();
        (meeting, folders, summary::get(&conn, id).map_err(|e| e.to_string())?, segments)
    };
    if segments.is_empty() || cfg.problem().is_some() {
        return Ok(None);
    }
    let opening = transcript::export(&meeting, &segments, Format::Text);
    // A stored summary was made from shown lines too, and says more in less.
    let about = summary.map(|s| s.text).filter(|s| s.len() < opening.len()).map(|s| format!("{}\n\n{s}", meeting.title)).unwrap_or(opening);
    let names = folders.iter().map(|f| format!("- {}", f.name)).collect::<Vec<_>>().join("\n");
    let reply = cfg.complete(&[
        Message { role: "system", content: "Pick the one folder this meeting belongs in. Answer with exactly the folder's name from the list, or none if no folder fits.".into() },
        Message { role: "user", content: format!("Folders:\n{names}\n\nMeeting:\n{about}") },
    ])?;
    let found = pick(&reply.text, &folders);
    let _ = settings::set(&db.lock().unwrap(), &key(id), &found.as_ref().map_or("none".into(), |f| f.id.to_string()));
    Ok(found)
}

/// Async so the request, and a possible Keychain prompt, stay off the main thread.
#[tauri::command(async)]
pub fn folder_suggest(app: AppHandle, id: i64) -> Result<Option<store::Folder>, String> {
    let db = &app.state::<Db>().0;
    suggest(db, id, &Config::load(db))
}

/// Accepting is the ordinary move; dismissing just remembers "none" so the chip stays gone.
#[tauri::command]
pub fn folder_suggest_dismiss(db: tauri::State<Db>, id: i64) -> Result<(), String> {
    settings::set(&db.0.lock().unwrap(), &key(id), "none").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::stub;
    use crate::meetings::mute::State;

    fn config(base_url: &str) -> Config {
        Config { preset: &ai::PRESETS[3], base_url: base_url.into(), model: "llama".into(), key: None, input_price: None, output_price: None, context: None }
    }

    /// A finished Inbox meeting that mentions Acme, with the toggle on and two folders.
    fn setup() -> (Mutex<Connection>, i64, i64) {
        let c = stub::db();
        store::init(&c).unwrap();
        summary::init(&c).unwrap();
        settings::set(&c, "ai_folders", "true").unwrap();
        let acme = store::create_folder(&c, "Acme").unwrap().id;
        store::create_folder(&c, "1:1s").unwrap();
        let id = store::create(&c, 100).unwrap();
        let seg = |text: &str, mute| store::Segment { text: text.into(), mute, speaker: store::ME.into(), ..Default::default() };
        store::finish(&c, id, 60_000, &[seg("Welcome to the Acme kickoff.", State::Unmuted), seg("Muted secret.", State::Muted)]).unwrap();
        (Mutex::new(c), id, acme)
    }

    #[test]
    fn a_folder_name_becomes_a_suggestion_none_and_unknown_names_do_not() {
        let (db, id, acme) = setup();
        let (base, seen) = stub::serve(vec![stub::ok("“acme”."), stub::ok("none"), stub::ok("Globex")]);
        let cfg = config(&base);

        let found = suggest(&db, id, &cfg).unwrap().unwrap();
        assert_eq!((found.id, found.name.as_str()), (acme, "Acme"));
        // Only folder names and the shown lines were sent; the muted line stayed here.
        let req: serde_json::Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        let sent = req["messages"][1]["content"].as_str().unwrap();
        assert!(sent.contains("- Acme") && sent.contains("- 1:1s") && sent.contains("Acme kickoff."), "{sent}");
        assert!(!sent.contains("Muted secret."), "{sent}");

        // The answer is stored: asking again costs no request.
        assert_eq!(suggest(&db, id, &cfg).unwrap().map(|f| f.id), Some(acme));
        assert!(seen.try_recv().is_err());

        // "none" and a name that is not a folder give no chip, and are remembered too.
        for _ in 0..2 {
            let other = store::create(&db.lock().unwrap(), 200).unwrap();
            store::finish(&db.lock().unwrap(), other, 1000, &[store::Segment { text: "Hello".into(), ..Default::default() }]).unwrap();
            assert_eq!(suggest(&db, other, &cfg).unwrap(), None);
            assert_eq!(suggest(&db, other, &cfg).unwrap(), None);
        }
    }

    #[test]
    fn nothing_is_sent_when_off_unready_filed_dismissed_or_in_progress() {
        let (db, id, _) = setup();
        let (base, seen) = stub::serve(vec![stub::ok("Acme")]);
        let cfg = config(&base);

        settings::set(&db.lock().unwrap(), "ai_folders", "false").unwrap();
        assert_eq!(suggest(&db, id, &cfg).unwrap(), None);
        settings::set(&db.lock().unwrap(), "ai_folders", "true").unwrap();
        let mut unready = config(&base);
        unready.model.clear();
        assert_eq!(suggest(&db, id, &unready).unwrap(), None);
        let recording = store::create(&db.lock().unwrap(), 300).unwrap();
        assert_eq!(suggest(&db, recording, &cfg).unwrap(), None);
        assert!(seen.try_recv().is_err());

        // Already filed: no chip and no request.
        store::move_meeting(&db.lock().unwrap(), id, Some(1)).unwrap();
        assert_eq!(suggest(&db, id, &cfg).unwrap(), None);
        assert!(seen.try_recv().is_err());

        // Dismissed before any answer: the chip stays gone.
        store::move_meeting(&db.lock().unwrap(), id, None).unwrap();
        settings::set(&db.lock().unwrap(), &key(id), "none").unwrap();
        assert_eq!(suggest(&db, id, &cfg).unwrap(), None);
        assert!(seen.try_recv().is_err());
    }

    #[test]
    fn a_smaller_stored_summary_is_sent_in_place_of_the_lines() {
        let (db, id, _) = setup();
        db.lock().unwrap().execute(
            "INSERT INTO summaries VALUES (?1, 'Acme kickoff, decisions.', 'p', 'm', 'x', 1, 1, NULL, 0, 1)", [id]).unwrap();
        let (base, seen) = stub::serve(vec![stub::ok("none")]);
        suggest(&db, id, &config(&base)).unwrap();
        let req: serde_json::Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        let sent = req["messages"][1]["content"].as_str().unwrap();
        assert!(sent.contains("Acme kickoff, decisions.") && !sent.contains("Welcome to the Acme kickoff."), "{sent}");
    }
}
