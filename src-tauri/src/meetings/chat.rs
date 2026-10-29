//! The Chat tab: one persisted thread per meeting. Every turn sends the transcript, the summary and the
//! earlier turns, so the provider keeps no memory of its own.
use super::{
    store::{self, Meeting, Segment},
    summary,
    transcript::{self, Format},
};
use crate::{
    ai::{self, Config, Message},
    Db,
};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const SYSTEM: &str = "You answer questions about one meeting. Its transcript, and a summary when there is one, follow. Answer from them, say so when they do not cover the question, and keep answers short.";

/// A question and its answer. A turn is stored only once the answer has arrived, so a thread always alternates.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Turn {
    pub question: String,
    pub answer: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct Usage {
    /// What the next question would send, before the question itself.
    pub context_tokens: u64,
    /// Spent on this chat so far; None when the model's prices are blank.
    pub cost: Option<f64>,
    /// The model's context window as the user entered it.
    pub window: Option<u64>,
    pub over_window: bool,
}

/// What the Chat tab renders; the shape is the contract with ui/ai.js.
#[derive(Serialize, Debug)]
pub struct Chat {
    pub turns: Vec<Turn>,
    #[serde(flatten)]
    pub usage: Usage,
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_turns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            meeting_id INTEGER NOT NULL,
            question TEXT NOT NULL,
            answer TEXT NOT NULL,
            model TEXT NOT NULL,
            tokens_in INTEGER NOT NULL,
            tokens_out INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS chat_turns_meeting ON chat_turns (meeting_id, id);",
    )
}

fn turns(db: &Connection, meeting: i64) -> rusqlite::Result<Vec<Turn>> {
    db.prepare("SELECT question, answer, model, tokens_in, tokens_out FROM chat_turns WHERE meeting_id = ?1 ORDER BY id")?
        .query_map([meeting], |r| Ok(Turn { question: r.get(0)?, answer: r.get(1)?, model: r.get(2)?, tokens_in: r.get(3)?, tokens_out: r.get(4)? }))?
        .collect()
}

/// The system message (transcript and summary) followed by the earlier turns, ready for the next question.
/// Only lines `Segment::shown` lets through reach it: muted speech never leaves the Mac unless the user included it.
pub fn context(meeting: &Meeting, segments: &[Segment], summary: Option<&str>, history: &[Turn]) -> Vec<Message> {
    // `export` keeps only the shown lines, the same rule the transcript view and the summary use.
    let mut system = format!("{SYSTEM}\n\nTranscript:\n{}", transcript::export(meeting, segments, Format::Text));
    if let Some(summary) = summary {
        system += &format!("\nSummary:\n{summary}\n");
    }
    let mut messages = vec![Message { role: "system", content: system }];
    for t in history {
        messages.push(Message { role: "user", content: t.question.clone() });
        messages.push(Message { role: "assistant", content: t.answer.clone() });
    }
    messages
}

/// Approximate tokens: characters ÷ 4, the same rule the provider client falls back on.
pub fn tokens(messages: &[Message]) -> u64 {
    messages.iter().map(|m| m.content.chars().count() as u64).sum::<u64>() / 4
}

/// The header numbers. `cost` adds up what each turn really used, so it only grows; the window is
/// checked against the context the next question would send.
pub fn usage(cfg: &Config, context: &[Message], history: &[Turn]) -> Usage {
    let (spent_in, spent_out) = history.iter().fold((0, 0), |(i, o), t| (i + t.tokens_in, o + t.tokens_out));
    let context_tokens = tokens(context);
    Usage {
        context_tokens,
        cost: cfg.cost(spent_in, spent_out),
        window: cfg.context,
        over_window: cfg.context.is_some_and(|w| context_tokens > w),
    }
}

/// Everything a question is built from.
struct Thread {
    meeting: Meeting,
    segments: Vec<Segment>,
    summary: Option<String>,
    turns: Vec<Turn>,
}

impl Thread {
    fn load(db: &Connection, id: i64) -> Result<Self, String> {
        let e = |e: rusqlite::Error| e.to_string();
        Ok(Thread {
            meeting: store::get(db, id).map_err(e)?,
            segments: store::segments(db, id).map_err(e)?,
            summary: summary::get(db, id).map_err(e)?.map(|s| s.text),
            turns: turns(db, id).map_err(e)?,
        })
    }

    fn messages(&self) -> Vec<Message> {
        context(&self.meeting, &self.segments, self.summary.as_deref(), &self.turns)
    }

    fn view(self, cfg: &Config) -> Chat {
        let usage = usage(cfg, &self.messages(), &self.turns);
        Chat { turns: self.turns, usage }
    }
}

/// Sends `question` with the whole context and stores the answer. The database lock is not held while the request runs;
/// a failed request stores nothing, so the thread is as it was.
fn send(db: &Mutex<Connection>, id: i64, question: &str, cfg: &Config) -> Result<Chat, String> {
    let (enabled, mut thread) = {
        let conn = db.lock().unwrap();
        (ai::enabled(&conn, "chat"), Thread::load(&conn, id)?)
    };
    if !enabled {
        return Err("Chat is off. Turn it on under AI provider in Settings.".into());
    }
    if let Some(problem) = cfg.problem() {
        return Err(problem.into());
    }
    if thread.meeting.status != "done" || !thread.segments.iter().any(Segment::shown) {
        return Err("There is no transcript to chat about yet.".into());
    }
    let question = question.trim();
    if question.is_empty() {
        return Err("Type a question first.".into());
    }
    let mut messages = thread.messages();
    messages.push(Message { role: "user", content: question.into() });
    let reply = cfg.complete(&messages)?;
    let turn = Turn { question: question.into(), answer: reply.text, model: cfg.model.clone(), tokens_in: reply.tokens_in, tokens_out: reply.tokens_out };
    db.lock()
        .unwrap()
        .execute(
            "INSERT INTO chat_turns (meeting_id, question, answer, model, tokens_in, tokens_out) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, turn.question, turn.answer, turn.model, turn.tokens_in, turn.tokens_out],
        )
        .map_err(|e| e.to_string())?;
    thread.turns.push(turn);
    Ok(thread.view(cfg))
}

/// Reading a thread needs no key, so it never touches the Keychain.
#[tauri::command]
pub fn chat_get(db: tauri::State<Db>, id: i64) -> Result<Chat, String> {
    let conn = db.0.lock().unwrap();
    Ok(Thread::load(&conn, id)?.view(&Config::from_settings(&conn)))
}

/// Async so the request, and a possible Keychain prompt, stay off the main thread.
#[tauri::command(async)]
pub fn chat_send(app: AppHandle, id: i64, question: String) -> Result<Chat, String> {
    let db = &app.state::<Db>().0;
    send(db, id, &question, &Config::load(db))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ai::stub, settings};

    fn db() -> Mutex<Connection> {
        let c = stub::db();
        store::init(&c).unwrap();
        summary::init(&c).unwrap();
        init(&c).unwrap();
        settings::set(&c, "ai_chat", "true").unwrap();
        Mutex::new(c)
    }

    fn done_meeting(db: &Mutex<Connection>) -> i64 {
        let c = db.lock().unwrap();
        let id = store::create(&c, 100).unwrap();
        let seg = |speaker: &str, start_ms, text: &str| Segment { speaker: speaker.into(), start_ms, end_ms: start_ms + 1000, text: text.into(), ..Default::default() };
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

    fn body(seen: &std::sync::mpsc::Receiver<stub::Seen>) -> serde_json::Value {
        serde_json::from_str(&seen.recv().unwrap().body).unwrap()
    }

    fn msg(text: &str) -> Message {
        Message { role: "user", content: text.into() }
    }

    fn turn(q: &str, a: &str, tokens_in: u64, tokens_out: u64) -> Turn {
        Turn { question: q.into(), answer: a.into(), model: "llama".into(), tokens_in, tokens_out }
    }

    #[test]
    fn the_context_is_the_transcript_then_the_summary_then_the_turns_so_far() {
        let db = db();
        let id = done_meeting(&db);
        let t = Thread::load(&db.lock().unwrap(), id).unwrap();
        let first = context(&t.meeting, &t.segments, None, &[]);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].role, "system");
        assert!(first[0].content.contains("[00:00] Me: Ship it Friday.") && first[0].content.contains("Them: Agreed, Priya owns the notes."));
        assert!(!first[0].content.contains("Summary:"));

        let history = [turn("Who owns the notes?", "Priya.", 10, 2)];
        let next = context(&t.meeting, &t.segments, Some("A short sync."), &history);
        let roles: Vec<_> = next.iter().map(|m| (m.role, m.content.as_str())).collect();
        assert_eq!(roles[1..], [("user", "Who owns the notes?"), ("assistant", "Priya.")]);
        assert!(next[0].content.contains("Summary:\nA short sync."));
    }

    #[test]
    fn tokens_are_characters_over_four_and_cost_follows_the_price_table() {
        assert_eq!(tokens(&[]), 0);
        assert_eq!(tokens(&[msg(&"x".repeat(40)), msg(&"é".repeat(8))]), 12); // characters, not bytes
        assert_eq!(tokens(&[msg("abc")]), 0);

        let mut cfg = config("http://localhost/v1");
        let history = [turn("q", "a", 1000, 200), turn("q", "a", 1_001_000, 300)];
        let u = usage(&cfg, &[msg(&"x".repeat(400))], &history);
        assert_eq!(u.context_tokens, 100);
        // 1_002_000 in at $0.25 and 500 out at $2.00 per million.
        assert!((u.cost.unwrap() - 0.2515).abs() < 1e-9, "{:?}", u.cost);

        // A blank price means tokens only; a blank thread has spent nothing.
        cfg.output_price = None;
        assert_eq!(usage(&cfg, &[], &history).cost, None);
        cfg.output_price = Some(2.0);
        assert_eq!(usage(&cfg, &[], &[]).cost, Some(0.0));
    }

    #[test]
    fn the_window_warning_fires_only_past_the_entered_window() {
        let mut cfg = config("http://localhost/v1");
        let ctx = [msg(&"x".repeat(400))]; // 100 tokens
        assert!(!usage(&cfg, &ctx, &[]).over_window); // no window entered
        for (window, over) in [(99, true), (100, false), (101, false), (10, true)] {
            cfg.context = Some(window);
            let u = usage(&cfg, &ctx, &[]);
            assert_eq!((u.over_window, u.window), (over, Some(window)), "{window}");
        }
    }

    #[test]
    fn a_thread_is_stored_and_every_turn_sends_the_whole_context_again() {
        let db = db();
        let id = done_meeting(&db);
        // A stored summary rides along in the context.
        db.lock()
            .unwrap()
            .execute("INSERT INTO summaries VALUES (?1, 'A short sync.', 'p', 'm', 'x', 1, 1, NULL, 0, 1)", [id])
            .unwrap();
        let (base, seen) = stub::serve(vec![stub::ok("Priya."), stub::ok("Friday.")]);
        let cfg = config(&base);

        let c = send(&db, id, "  Who owns the notes? ", &cfg).unwrap();
        assert_eq!(c.turns, [turn("Who owns the notes?", "Priya.", 1000, 200)]);
        assert!((c.usage.cost.unwrap() - 0.00065).abs() < 1e-9);
        let req = body(&seen);
        assert_eq!(req["messages"].as_array().unwrap().len(), 2);
        assert!(req["messages"][0]["content"].as_str().unwrap().contains("Summary:\nA short sync."));
        assert_eq!(req["messages"][1]["content"], "Who owns the notes?");

        let c = send(&db, id, "When do we ship?", &cfg).unwrap();
        let sent: Vec<_> = body(&seen)["messages"].as_array().unwrap().iter().map(|m| (m["role"].as_str().unwrap().to_string(), m["content"].as_str().unwrap().to_string())).collect();
        let roles: Vec<_> = sent.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert_eq!((sent[1].1.as_str(), sent[2].1.as_str(), sent[3].1.as_str()), ("Who owns the notes?", "Priya.", "When do we ship?"));
        assert!((c.usage.cost.unwrap() - 0.0013).abs() < 1e-9); // two turns of 1000 in, 200 out

        // Reopening the meeting, or the app, finds the same thread without a request.
        let again = Thread::load(&db.lock().unwrap(), id).unwrap().view(&cfg);
        assert_eq!(again.turns, c.turns);
        assert_eq!(again.usage, c.usage);
        assert!(seen.try_recv().is_err());
    }

    #[test]
    fn excluded_segments_are_not_in_the_chat_context_until_the_user_includes_them() {
        use super::super::mute::State;
        let db = db();
        let id = {
            let c = db.lock().unwrap();
            let id = store::create(&c, 100).unwrap();
            let seg = |text: &str, mute, included| Segment { text: text.into(), mute, included, speaker: store::ME.into(), ..Default::default() };
            let segs = [seg("Live line.", State::Unmuted, false), seg("Muted secret.", State::Muted, false), seg("Kept aside.", State::Unknown, true)];
            store::finish(&c, id, 60_000, &segs).unwrap();
            id
        };
        // The rule holds in the pure function, with nothing filtered beforehand...
        let t = Thread::load(&db.lock().unwrap(), id).unwrap();
        let ctx = context(&t.meeting, &t.segments, None, &[]);
        assert!(!ctx[0].content.contains("Muted secret.") && ctx[0].content.contains("Live line.") && ctx[0].content.contains("Kept aside."));

        // ...and on the wire, on the first turn and the later ones.
        let (base, seen) = stub::serve(vec![stub::ok("ok"), stub::ok("ok")]);
        send(&db, id, "What was said?", &config(&base)).unwrap();
        send(&db, id, "Anything else?", &config(&base)).unwrap();
        for _ in 0..2 {
            let req = seen.recv().unwrap().body;
            assert!(req.contains("Live line.") && req.contains("Kept aside."), "{req}");
            assert!(!req.contains("Muted secret."), "{req}");
        }

        // Only excluded speech: there is nothing to send, so no request is made.
        let only = {
            let c = db.lock().unwrap();
            let id = store::create(&c, 200).unwrap();
            store::finish(&c, id, 1000, &[Segment { text: "Muted secret.".into(), mute: State::Muted, ..Default::default() }]).unwrap();
            id
        };
        assert!(send(&db, only, "q", &config(&base)).unwrap_err().contains("no transcript"));
        assert!(seen.try_recv().is_err());
    }

    #[test]
    fn nothing_is_sent_while_chat_is_off_or_not_ready_and_a_failed_request_stores_nothing() {
        let db = db();
        let id = done_meeting(&db);
        let (base, seen) = stub::serve(vec![(500, r#"{"error":{"message":"overloaded"}}"#.into())]);
        let cfg = config(&base);

        settings::set(&db.lock().unwrap(), "ai_chat", "false").unwrap();
        assert!(send(&db, id, "q", &cfg).unwrap_err().contains("Chat is off"));
        settings::set(&db.lock().unwrap(), "ai_chat", "true").unwrap();
        let recording = store::create(&db.lock().unwrap(), 200).unwrap();
        assert!(send(&db, recording, "q", &cfg).unwrap_err().contains("no transcript"));
        assert!(send(&db, id, "  ", &cfg).is_err());
        let mut unready = config(&base);
        unready.model.clear();
        assert!(send(&db, id, "q", &unready).unwrap_err().contains("model"));
        assert!(seen.try_recv().is_err());

        assert!(send(&db, id, "q", &cfg).unwrap_err().contains("overloaded"));
        assert!(turns(&db.lock().unwrap(), id).unwrap().is_empty());
    }
}
