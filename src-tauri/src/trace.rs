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

/// One row of Voice typing history and Stats for nerds' list, newest first. `heard` is what speech-to-text wrote;
/// `text` is what the last step left, which is what was pasted (or would have been).
#[derive(serde::Serialize)]
pub struct Row {
    pub id: i64,
    pub started: i64,
    pub app: Option<String>,
    pub outcome: String,
    pub heard: Option<String>,
    pub text: Option<String>,
    /// "up", "down" or none.
    pub rating: Option<String>,
}

pub fn list(conn: &Connection) -> Result<Vec<Row>, String> {
    let err = |e: rusqlite::Error| format!("Couldn't read the dictation traces: {e}");
    let mut q = conn
        // shortcut: loads every trace at once; page it if history grows past a few thousand.
        .prepare(
            "SELECT id, started, app, outcome, (SELECT text FROM step WHERE trace = trace.id AND kind = 'speech-to-text'),
                (SELECT text FROM step WHERE trace = trace.id ORDER BY rowid DESC LIMIT 1),
                (SELECT rating FROM feedback WHERE trace = trace.id) FROM trace ORDER BY started DESC, id DESC",
        )
        .map_err(err)?;
    let rows = q
        .query_map([], |r| Ok(Row { id: r.get(0)?, started: r.get(1)?, app: r.get(2)?, outcome: r.get(3)?, heard: r.get(4)?, text: r.get(5)?, rating: r.get(6)? }))
        .map_err(err)?;
    rows.collect::<Result<_, _>>().map_err(err)
}

/// Delete from Voice typing history: the trace and its steps go together.
pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    let err = |e: rusqlite::Error| format!("Couldn't delete the dictation: {e}");
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM feedback WHERE trace = ?1", [id]).map_err(err)?;
    tx.execute("DELETE FROM step WHERE trace = ?1", [id]).map_err(err)?;
    tx.execute("DELETE FROM trace WHERE id = ?1", [id]).map_err(err)?;
    tx.commit().map_err(err)
}

/// Clear all, in Voice typing history and Stats for nerds: every trace and its steps.
pub fn clear(conn: &Connection) -> Result<(), String> {
    let err = |e: rusqlite::Error| format!("Couldn't clear the history: {e}");
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM feedback", []).map_err(err)?;
    tx.execute("DELETE FROM step", []).map_err(err)?;
    tx.execute("DELETE FROM trace", []).map_err(err)?;
    tx.commit().map_err(err)
}

const COLS: &str = "id, started, mode, app, model, audio_ms, outcome, reason, load_ms, paste_ms, (SELECT rating FROM feedback WHERE trace = trace.id)";

/// A trace's columns as JSON, the model still as its id.
fn trace_json(r: &rusqlite::Row) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, i64>(0)?, "started": r.get::<_, i64>(1)?, "mode": r.get::<_, String>(2)?, "app": r.get::<_, Option<String>>(3)?,
        "model": r.get::<_, Option<String>>(4)?, "audio_ms": r.get::<_, i64>(5)?, "outcome": r.get::<_, String>(6)?,
        "reason": r.get::<_, Option<String>>(7)?, "load_ms": r.get::<_, Option<f64>>(8)?, "paste_ms": r.get::<_, Option<f64>>(9)?, "rating": r.get::<_, Option<String>>(10)?,
    }))
}

/// A step as JSON, from `kind, text, ms, settings, edits` starting at column `at`.
fn step_json(r: &rusqlite::Row, at: usize) -> rusqlite::Result<serde_json::Value> {
    let json = |s: Option<String>| s.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    Ok(serde_json::json!({ "kind": r.get::<_, String>(at)?, "text": r.get::<_, String>(at + 1)?, "ms": r.get::<_, f64>(at + 2)?,
        "settings": json(r.get(at + 3)?), "edits": json(r.get(at + 4)?) }))
}

/// One trace with its steps, for the opened view. The model shows by name; a model no longer in the catalog keeps its id.
pub fn get(conn: &Connection, id: i64) -> Result<serde_json::Value, String> {
    let err = |e: rusqlite::Error| format!("Couldn't read the dictation trace: {e}");
    let mut t = conn.query_row(&format!("SELECT {COLS} FROM trace WHERE id = ?1"), [id], trace_json).map_err(err)?;
    if let Some(m) = t["model"].as_str().map(String::from) {
        t["model"] = crate::models::find(&m).map_or(m, |m| m.name).into();
    }
    let mut q = conn.prepare("SELECT kind, text, ms, settings, edits FROM step WHERE trace = ?1 ORDER BY rowid").map_err(err)?;
    let steps = q.query_map([id], |r| step_json(r, 0)).map_err(err)?.collect::<Result<Vec<_>, _>>().map_err(err)?;
    t["steps"] = steps.into();
    Ok(t)
}

/// Every trace with its steps, newest first, for Stats for nerds' Overview. As `get`, except that a model no longer
/// in the catalog shows as "<id> (removed)" and a missing model or app as "Unknown model" or "Unknown app", so neither drops out of a breakdown.
pub fn all(conn: &Connection) -> Result<Vec<serde_json::Value>, String> {
    let err = |e: rusqlite::Error| format!("Couldn't read the dictation traces: {e}");
    let mut steps: std::collections::HashMap<i64, Vec<serde_json::Value>> = Default::default();
    let mut q = conn.prepare("SELECT trace, kind, text, ms, settings, edits FROM step ORDER BY rowid").map_err(err)?;
    for s in q.query_map([], |r| Ok((r.get::<_, i64>(0)?, step_json(r, 1)?))).map_err(err)? {
        let (trace, s) = s.map_err(err)?;
        steps.entry(trace).or_default().push(s);
    }
    let names: std::collections::HashMap<String, String> = crate::models::all().into_iter().map(|m| (m.id, m.name)).collect();
    // shortcut: loads every trace at once; page it if history grows past a few thousand.
    let mut q = conn.prepare(&format!("SELECT {COLS} FROM trace ORDER BY started DESC, id DESC")).map_err(err)?;
    let traces = q.query_map([], trace_json).map_err(err)?.collect::<Result<Vec<_>, _>>().map_err(err)?;
    Ok(traces
        .into_iter()
        .map(|mut t| {
            t["model"] = match t["model"].as_str() {
                Some(m) => names.get(m).cloned().unwrap_or(format!("{m} (removed)")).into(),
                None => "Unknown model".into(),
            };
            if t["app"].is_null() {
                t["app"] = "Unknown app".into();
            }
            t["steps"] = steps.remove(&t["id"].as_i64().unwrap_or_default()).unwrap_or_default().into();
            t
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_traces_list_newest_first_and_open_with_their_steps() {
        let conn = crate::storage::open(&std::env::temp_dir().join(format!("myna-trace-{}", std::process::id()))).unwrap();
        let step = |kind, text: &str, edits| Step { kind, text: text.into(), ms: 1.5, settings: None, edits };
        for (started, heard) in [(1, "old one"), (2, "Um hello")] {
            let steps = vec![step("speech-to-text", heard, None), step("rules", "hello", Some(r#"[{"kind":"filler","start":0,"end":3,"to":""}]"#.into()))];
            save(&conn, &Trace { started, mode: "hold", app: Some("Mail".into()), model: Some("gone".into()), outcome: "pasted", steps, ..Default::default() }).unwrap();
        }
        let rows = list(&conn).unwrap();
        assert_eq!(rows.iter().map(|r| r.started).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(rows[0].heard.as_deref(), Some("Um hello"));
        let t = get(&conn, rows[0].id).unwrap();
        assert_eq!(t["model"], "gone");
        assert_eq!(t["steps"][1]["edits"][0]["kind"], "filler");
        assert_eq!(rows[0].text.as_deref(), Some("hello"));
        assert_eq!((rows[0].rating.as_deref(), t["rating"].is_null()), (None, true));
        assert!(get(&conn, 99).is_err());
    }

    #[test]
    fn clearing_takes_every_trace_and_step() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        for started in [1, 2] {
            let steps = vec![Step { kind: "speech-to-text", text: "hi".into(), ms: 1.0, settings: None, edits: None }];
            save(&conn, &Trace { started, mode: "hold", outcome: "pasted", steps, ..Default::default() }).unwrap();
        }
        clear(&conn).unwrap();
        let left: i64 = conn.query_row("SELECT (SELECT count(*) FROM trace) + (SELECT count(*) FROM step)", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
        clear(&conn).unwrap(); // nothing to clear is fine
    }

    #[test]
    fn deleting_a_trace_takes_its_steps_and_leaves_the_others() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        for started in [1, 2] {
            let steps = vec![Step { kind: "speech-to-text", text: "hi".into(), ms: 1.0, settings: None, edits: None }];
            save(&conn, &Trace { started, mode: "hold", outcome: "pasted", steps, ..Default::default() }).unwrap();
        }
        let gone = list(&conn).unwrap()[0].id;
        delete(&conn, gone).unwrap();
        assert_eq!(list(&conn).unwrap().iter().map(|r| r.started).collect::<Vec<_>>(), [1]);
        let steps: i64 = conn.query_row("SELECT count(*) FROM step WHERE trace = ?1", [gone], |r| r.get(0)).unwrap();
        let left: i64 = conn.query_row("SELECT count(*) FROM step", [], |r| r.get(0)).unwrap();
        assert_eq!((steps, left), (0, 1));
    }

    #[test]
    fn all_traces_come_with_their_steps_and_keep_a_removed_model_and_a_missing_app() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let step = |text: &str| Step { kind: "speech-to-text", text: text.into(), ms: 2.0, settings: None, edits: None };
        save(&conn, &Trace { started: 1, mode: "hold", model: Some("gone-model".into()), outcome: "pasted", steps: vec![step("old")], ..Default::default() }).unwrap();
        save(&conn, &Trace { started: 2, mode: "tap", app: Some("Mail".into()), outcome: "failed", ..Default::default() }).unwrap();
        save(&conn, &Trace { started: 3, mode: "hold", app: Some("Notes".into()), outcome: "pasted", steps: vec![step("new"), step("newer")], ..Default::default() }).unwrap();
        let all = all(&conn).unwrap();
        assert_eq!(all.iter().map(|t| t["started"].as_i64().unwrap()).collect::<Vec<_>>(), [3, 2, 1]);
        assert_eq!(all[0]["steps"].as_array().unwrap().iter().map(|s| s["text"].as_str().unwrap()).collect::<Vec<_>>(), ["new", "newer"]);
        assert_eq!(all[1]["steps"], serde_json::json!([]));
        assert_eq!(all[2]["model"], "gone-model (removed)");
        assert_eq!(all[2]["app"], "Unknown app");
        assert_eq!(all[1]["model"], "Unknown model");
        let one = get(&conn, all[2]["id"].as_i64().unwrap()).unwrap();
        assert_eq!((one["model"].as_str(), one["app"].is_null()), (Some("gone-model"), true), "the opened trace is unchanged");
        assert_eq!(one["steps"], all[2]["steps"]);
    }
}
