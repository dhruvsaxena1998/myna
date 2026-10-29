//! The Summary tab: prompt templates (three built in, the rest the user's) and one stored AI
//! summary per meeting, so reopening a meeting never costs a request.
use super::{
    store,
    transcript::{self, Format},
};
use crate::{
    ai::{self, Config, Message},
    Db,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const BUILT_IN: [(&str, &str); 3] = [
    (
        "Summary",
        "Summarize this meeting in one short paragraph, then list the decisions and the action items, naming the owner when the transcript does. Use the headings Summary, Decisions and Action items.",
    ),
    ("Action items", "List the action items from this meeting as bullets, naming the owner when the transcript does. If there are none, say so."),
    ("Decisions", "List the decisions made in this meeting as bullets. If there are none, say so."),
];

#[derive(Serialize, Debug)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub prompt: String,
    pub builtin: bool,
}

#[derive(Serialize, Debug)]
pub struct Summary {
    pub text: String,
    /// The prompt it was generated from; the editor opens prefilled with it.
    pub prompt: String,
    pub model: String,
    pub provider: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// None when the model's prices were blank.
    pub cost: Option<f64>,
    /// Whether the request left this Mac.
    pub off_device: bool,
    pub generated_at: i64,
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            prompt TEXT NOT NULL,
            builtin INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS summaries (
            meeting_id INTEGER PRIMARY KEY,
            text TEXT NOT NULL,
            prompt TEXT NOT NULL,
            model TEXT NOT NULL,
            provider TEXT NOT NULL,
            tokens_in INTEGER NOT NULL,
            tokens_out INTEGER NOT NULL,
            cost REAL,
            off_device INTEGER NOT NULL,
            generated_at INTEGER NOT NULL
        );",
    )?;
    // Re-seeded on every start so a better built-in prompt reaches existing installs.
    for (i, (name, prompt)) in BUILT_IN.iter().enumerate() {
        db.execute(
            "INSERT INTO templates (id, name, prompt, builtin) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, prompt = excluded.prompt",
            params![i as i64 + 1, name, prompt],
        )?;
    }
    Ok(())
}

fn template(r: &rusqlite::Row) -> rusqlite::Result<Template> {
    Ok(Template { id: r.get(0)?, name: r.get(1)?, prompt: r.get(2)?, builtin: r.get(3)? })
}

pub fn templates(db: &Connection) -> rusqlite::Result<Vec<Template>> {
    db.prepare("SELECT id, name, prompt, builtin FROM templates ORDER BY builtin DESC, id")?.query_map([], template)?.collect()
}

// shortcut: a template is renamed by saving it under the new name and deleting the old one; add an
// edit-in-place when the templates list grows a manage view.
/// Saves under `name`: a new template, or the prompt of the user's template of that name.
pub fn save_template(db: &Connection, name: &str, prompt: &str) -> Result<Template, String> {
    let (name, prompt) = (name.trim(), prompt.trim());
    if name.is_empty() || prompt.is_empty() {
        return Err("A template needs a name and a prompt.".into());
    }
    let changed = db
        .execute(
            "INSERT INTO templates (name, prompt, builtin) VALUES (?1, ?2, 0)
             ON CONFLICT(name) DO UPDATE SET prompt = excluded.prompt WHERE builtin = 0",
            [name, prompt],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err(format!("“{name}” is a built-in template. Pick another name."));
    }
    db.query_row("SELECT id, name, prompt, builtin FROM templates WHERE name = ?1", [name], template).map_err(|e| e.to_string())
}

/// Built-in templates stay.
pub fn delete_template(db: &Connection, id: i64) {
    let _ = db.execute("DELETE FROM templates WHERE id = ?1 AND builtin = 0", [id]);
}

pub fn get(db: &Connection, meeting: i64) -> rusqlite::Result<Option<Summary>> {
    db.query_row(
        "SELECT text, prompt, model, provider, tokens_in, tokens_out, cost, off_device, generated_at FROM summaries WHERE meeting_id = ?1",
        [meeting],
        |r| {
            Ok(Summary {
                text: r.get(0)?,
                prompt: r.get(1)?,
                model: r.get(2)?,
                provider: r.get(3)?,
                tokens_in: r.get(4)?,
                tokens_out: r.get(5)?,
                cost: r.get(6)?,
                off_device: r.get(7)?,
                generated_at: r.get(8)?,
            })
        },
    )
    .optional()
}

/// Sends the transcript with `prompt` to the provider and stores the answer in place of any earlier one.
/// The database lock is not held while the request runs.
fn generate(db: &Mutex<Connection>, id: i64, prompt: &str, cfg: &Config, now: i64) -> Result<Summary, String> {
    let (enabled, meeting, segments) = {
        let conn = db.lock().unwrap();
        (
            ai::enabled(&conn, "summaries"),
            store::get(&conn, id).map_err(|e| e.to_string())?,
            // Muted or excluded speech never leaves the Mac unless the user included it.
            store::segments(&conn, id).map_err(|e| e.to_string())?.into_iter().filter(|s| s.shown()).collect::<Vec<_>>(),
        )
    };
    if !enabled {
        return Err("Meeting summaries are off. Turn them on under AI provider in Settings.".into());
    }
    if let Some(problem) = cfg.problem() {
        return Err(problem.into());
    }
    if meeting.status != "done" || segments.is_empty() {
        return Err("There is no transcript to summarize yet.".into());
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("The prompt is empty.".into());
    }
    let reply = cfg.complete(&[
        Message { role: "system", content: prompt.into() },
        Message { role: "user", content: transcript::export(&meeting, &segments, Format::Text) },
    ])?;
    let summary = Summary {
        cost: cfg.cost(reply.tokens_in, reply.tokens_out),
        text: reply.text,
        prompt: prompt.into(),
        model: cfg.model.clone(),
        provider: cfg.preset.name.into(),
        tokens_in: reply.tokens_in,
        tokens_out: reply.tokens_out,
        off_device: cfg.off_device(),
        generated_at: now,
    };
    db.lock()
        .unwrap()
        .execute(
            "INSERT OR REPLACE INTO summaries (meeting_id, text, prompt, model, provider, tokens_in, tokens_out, cost, off_device, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, summary.text, summary.prompt, summary.model, summary.provider, summary.tokens_in, summary.tokens_out, summary.cost, summary.off_device, summary.generated_at],
        )
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

#[tauri::command]
pub fn template_list(db: tauri::State<Db>) -> Result<Vec<Template>, String> {
    templates(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn template_save(db: tauri::State<Db>, name: String, prompt: String) -> Result<Template, String> {
    save_template(&db.0.lock().unwrap(), &name, &prompt)
}

#[tauri::command]
pub fn template_delete(db: tauri::State<Db>, id: i64) {
    delete_template(&db.0.lock().unwrap(), id)
}

#[tauri::command]
pub fn summary_get(db: tauri::State<Db>, id: i64) -> Result<Option<Summary>, String> {
    get(&db.0.lock().unwrap(), id).map_err(|e| e.to_string())
}

/// Async so the request, and a possible Keychain prompt, stay off the main thread.
#[tauri::command(async)]
pub fn summary_generate(app: AppHandle, id: i64, prompt: String) -> Result<Summary, String> {
    let db = &app.state::<Db>().0;
    generate(db, id, &prompt, &Config::load(db), super::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ai::stub, settings};

    fn db() -> Mutex<Connection> {
        let c = stub::db();
        store::init(&c).unwrap();
        init(&c).unwrap();
        Mutex::new(c)
    }

    fn done_meeting(db: &Mutex<Connection>) -> i64 {
        let c = db.lock().unwrap();
        let id = store::create(&c, 100).unwrap();
        let seg = |speaker: &str, start_ms, text: &str| store::Segment { speaker: speaker.into(), start_ms, end_ms: start_ms + 1000, text: text.into(), ..Default::default() };
        store::finish(&c, id, 60_000, &[seg(store::ME, 0, "Ship it Friday."), seg(store::THEM, 5000, "Agreed, Priya owns the notes.")]).unwrap();
        id
    }

    fn config(base_url: &str) -> Config {
        Config {
            preset: &ai::PRESETS[3],
            base_url: base_url.into(),
            model: "llama".into(),
            key: None,
            input_price: Some(0.25),
            output_price: Some(2.0),
            context: None,
        }
    }

    #[test]
    fn built_ins_are_seeded_once_and_user_templates_follow() {
        let db = db();
        let c = db.lock().unwrap();
        init(&c).unwrap(); // a second start does not duplicate them
        let names = |c: &Connection| templates(c).unwrap().into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert_eq!(names(&c), ["Summary", "Action items", "Decisions"]);

        let mine = save_template(&c, "  Five bullets ", "Bullet list, max 5").unwrap();
        assert_eq!((mine.name.as_str(), mine.builtin), ("Five bullets", false));
        assert_eq!(names(&c), ["Summary", "Action items", "Decisions", "Five bullets"]);

        // Same name again edits the prompt in place.
        let again = save_template(&c, "Five bullets", "Bullet list, max 3").unwrap();
        assert_eq!((again.id, again.prompt.as_str()), (mine.id, "Bullet list, max 3"));
        assert_eq!(templates(&c).unwrap().len(), 4);

        // Built-ins can be neither overwritten nor deleted; an empty name or prompt is refused.
        assert!(save_template(&c, "Summary", "x").unwrap_err().contains("built-in"));
        assert!(save_template(&c, " ", "x").is_err() && save_template(&c, "n", " ").is_err());
        delete_template(&c, 1);
        delete_template(&c, mine.id);
        assert_eq!(names(&c), ["Summary", "Action items", "Decisions"]);
        assert_eq!(templates(&c).unwrap()[0].prompt, BUILT_IN[0].1);
    }

    #[test]
    fn a_summary_is_requested_once_stored_and_replaced_on_regenerate() {
        let db = db();
        let id = done_meeting(&db);
        settings::set(&db.lock().unwrap(), "ai_summaries", "true").unwrap();
        let (base, seen) = stub::serve(vec![stub::ok("## Summary\nShip Friday."), stub::ok("- Ship Friday")]);
        let cfg = config(&base);

        assert_eq!(get(&db.lock().unwrap(), id).unwrap().map(|s| s.text), None);
        let s = generate(&db, id, " Bullets only ", &cfg, 500).unwrap();
        assert_eq!((s.text.as_str(), s.prompt.as_str(), s.model.as_str(), s.generated_at), ("## Summary\nShip Friday.", "Bullets only", "llama", 500));
        // 1000 in at $0.25 and 200 out at $2.00 per million; the stub is on loopback, so nothing left the Mac.
        assert_eq!((s.tokens_in, s.tokens_out, s.cost, s.off_device), (1000, 200, Some(0.00065), false));

        let req: serde_json::Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        assert_eq!((req["messages"][0]["role"].as_str(), req["messages"][0]["content"].as_str()), (Some("system"), Some("Bullets only")));
        let sent = req["messages"][1]["content"].as_str().unwrap();
        assert!(sent.contains("[00:00] Me: Ship it Friday.") && sent.contains("Them: Agreed, Priya owns the notes."), "{sent}");

        // Reading it back needs no request: the stub has one reply left and nothing asks for it.
        let stored = get(&db.lock().unwrap(), id).unwrap().unwrap();
        assert_eq!((stored.text, stored.cost), (s.text, s.cost));
        assert!(seen.try_recv().is_err());

        generate(&db, id, "Other prompt", &cfg, 600).unwrap();
        let stored = get(&db.lock().unwrap(), id).unwrap().unwrap();
        assert_eq!((stored.text.as_str(), stored.prompt.as_str(), stored.generated_at), ("- Ship Friday", "Other prompt", 600));
    }

    #[test]
    fn nothing_is_sent_while_the_toggle_is_off_or_there_is_no_transcript() {
        let db = db();
        let id = done_meeting(&db);
        let (base, seen) = stub::serve(vec![stub::ok("never asked")]);
        let cfg = config(&base);

        let off = generate(&db, id, "p", &cfg, 1).unwrap_err();
        assert!(off.contains("Meeting summaries are off"), "{off}");

        settings::set(&db.lock().unwrap(), "ai_summaries", "true").unwrap();
        let recording = store::create(&db.lock().unwrap(), 200).unwrap();
        assert!(generate(&db, recording, "p", &cfg, 1).unwrap_err().contains("no transcript"));
        assert!(generate(&db, id, "  ", &cfg, 1).is_err());
        let mut unready = config(&base);
        unready.model.clear();
        assert!(generate(&db, id, "p", &unready, 1).unwrap_err().contains("model"));

        assert!(seen.try_recv().is_err());
        assert!(get(&db.lock().unwrap(), id).unwrap().is_none());
    }

    #[test]
    fn excluded_segments_are_not_in_the_prompt_until_the_user_includes_them() {
        use super::super::mute::State;
        let db = db();
        let id = {
            let c = db.lock().unwrap();
            let id = store::create(&c, 100).unwrap();
            let seg = |text: &str, mute, included| store::Segment { text: text.into(), mute, included, speaker: store::ME.into(), ..Default::default() };
            let segs = [seg("Live line.", State::Unmuted, false), seg("Muted secret.", State::Muted, false), seg("Kept aside.", State::Unknown, true)];
            store::finish(&c, id, 60_000, &segs).unwrap();
            id
        };
        settings::set(&db.lock().unwrap(), "ai_summaries", "true").unwrap();
        let (base, seen) = stub::serve(vec![stub::ok("ok")]);
        generate(&db, id, "p", &config(&base), 1).unwrap();
        let req: serde_json::Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        let sent = req["messages"][1]["content"].as_str().unwrap();
        assert!(sent.contains("Live line.") && sent.contains("Kept aside."), "{sent}");
        assert!(!sent.contains("Muted secret."), "{sent}");

        // Only excluded speech: there is nothing to send, so no request is made.
        let only = {
            let c = db.lock().unwrap();
            let id = store::create(&c, 200).unwrap();
            store::finish(&c, id, 1000, &[store::Segment { text: "Muted secret.".into(), mute: State::Muted, ..Default::default() }]).unwrap();
            id
        };
        assert!(generate(&db, only, "p", &config(&base), 2).unwrap_err().contains("no transcript"));
        assert!(seen.try_recv().is_err());
    }

    #[test]
    fn a_failed_request_keeps_the_earlier_summary() {
        let db = db();
        let id = done_meeting(&db);
        settings::set(&db.lock().unwrap(), "ai_summaries", "true").unwrap();
        let (base, _seen) = stub::serve(vec![stub::ok("first"), (500, r#"{"error":{"message":"overloaded"}}"#.into())]);
        let cfg = config(&base);
        generate(&db, id, "p", &cfg, 1).unwrap();
        assert!(generate(&db, id, "p", &cfg, 2).unwrap_err().contains("overloaded"));
        assert_eq!(get(&db.lock().unwrap(), id).unwrap().unwrap().text, "first");
    }
}
