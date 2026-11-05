//! A trace: the record of one dictation that reached speech-to-text, saved by the Dictation session when it ends.
//! Voice typing history and Stats for nerds read it. No audio is kept.

use rusqlite::Connection;

#[derive(Debug, Default)]
pub struct Trace {
    /// Unix time in milliseconds.
    pub started: i64,
    /// "hold" or "tap".
    pub mode: &'static str,
    /// The app in front when the dictation started: its name and bundle id.
    pub app: Option<String>,
    pub bundle: Option<String>,
    /// The speech model's id.
    pub model: Option<String>,
    pub audio_ms: i64,
    /// "pasted", "nothing_heard", "failed" or "not_pasted" (its text is kept in the last step).
    pub outcome: &'static str,
    /// Why it failed or wasn't pasted.
    pub reason: Option<String>,
    /// Time after letting go spent waiting for a cold model, kept apart from the step times.
    pub load_ms: Option<f64>,
    /// Let go to pasted.
    pub paste_ms: Option<f64>,
    pub steps: Vec<Step>,
}

/// One step the text passed through, recorded even when it changed nothing.
#[derive(Debug)]
pub struct Step {
    /// "speech-to-text" or "rules".
    pub kind: &'static str,
    pub text: String,
    pub ms: f64,
    /// JSON: the settings that shaped it.
    pub settings: Option<String>,
    /// JSON: the rules step's `rules::Edit`s.
    pub edits: Option<String>,
}

pub fn save(conn: &Connection, t: &Trace) -> Result<(), String> {
    let err = |e: rusqlite::Error| format!("Couldn't save the dictation's trace: {e}");
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "INSERT INTO trace(started, mode, app, bundle, model, audio_ms, outcome, reason, load_ms, paste_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        (t.started, t.mode, &t.app, &t.bundle, &t.model, t.audio_ms, t.outcome, &t.reason, t.load_ms, t.paste_ms),
    )
    .map_err(err)?;
    let id = tx.last_insert_rowid();
    for s in &t.steps {
        tx.execute("INSERT INTO step(trace, kind, text, ms, settings, edits) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", (id, s.kind, &s.text, s.ms, &s.settings, &s.edits))
            .map_err(err)?;
    }
    tx.commit().map_err(err)
}
