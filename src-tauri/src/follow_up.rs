//! Follow-up: the steps after a meeting is transcribed, in order: LLM cleanup of the transcript, the title, then the
//! Summary; and Regenerate, the Summary again on a template and model picked for that run. Each step says how it
//! goes to the windows as an event (`meeting-cleanup`, `meeting-title`, `meeting-summary`); the app reaches them, and
//! the menu's Recent list, through `App`, so tests run it with a fake.

use crate::ai::{self, Backend, Keys};
use crate::{summary, title, transcript};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// What follow-up needs from the app.
pub trait App: Send + Sync + 'static {
    /// Event `event` to every window.
    fn emit(&self, event: &str, payload: Value);
    /// Refreshes "Recent" in the menu.
    fn recent(&self);
}

pub struct FollowUp {
    db: Arc<Mutex<Connection>>,
    keys: Arc<dyn Keys>,
    app: Box<dyn App>,
}

impl FollowUp {
    pub fn new(db: Arc<Mutex<Connection>>, keys: Arc<dyn Keys>, app: Box<dyn App>) -> Self {
        Self { db, keys, app }
    }

    /// Meeting `id` is transcribed: cleans it, names it, then summarizes it, each when it's on and its model resolves.
    /// A failed cleanup keeps the text as heard and the rest runs on that; the title goes on the cleaned lines, before
    /// the Summary, so the list shows a name soon. Returns when all three are done or failed.
    pub fn run(&self, id: i64) {
        crate::ai_log::about(Some(format!("meeting:{id}")), || self.run_all(id))
    }

    fn run_all(&self, id: i64) {
        self.clean(id);
        self.name(id);
        let r = summary::auto(&self.db, &|p| self.open(p, "Summary"), id, &mut |p| self.summary_progress(id, p));
        self.say_summary(match r {
            Ok(Some(v)) => json!({ "id": id, "state": "done", "version": v }),
            Ok(None) => json!({ "id": id, "state": "skipped" }), // Summarize when a meeting finishes is off, or no model
            Err(e) => json!({ "id": id, "state": "failed", "error": e }),
        });
    }

    /// Regenerate: the template's name and instruction text on `provider`'s `model`, for this run only. Says how it
    /// goes like the automatic run; the answer is the new version.
    pub fn regenerate(&self, id: i64, template: String, prompt: String, provider: i64, model: String) -> Result<summary::Version, String> {
        let run = || {
            let (backend, ask) = {
                let db = self.db.lock().unwrap();
                let name: String = db.query_row("SELECT name FROM provider WHERE id = ?1", [provider], |r| r.get(0)).map_err(|_| "That provider isn't there anymore.".to_string())?;
                (ai::provider(&db, &*self.keys, provider, "Summary")?, summary::Ask { template, prompt, provider: name, context: ai::context(&db, provider, &model), model })
            };
            summary::run(&self.db, &*backend, id, &ask, &mut |p| self.summary_progress(id, p))
        };
        let r = crate::ai_log::about(Some(format!("meeting:{id}")), run);
        self.say_summary(match &r {
            Ok(v) => json!({ "id": id, "state": "done", "version": v }),
            Err(e) => json!({ "id": id, "state": "failed", "error": e }),
        });
        r
    }

    /// `meeting-cleanup`: "reading" part `n` of `of`, then "done" with the cost or "failed" with why. Nothing is said
    /// when it's off or its model is Not set.
    fn clean(&self, id: i64) {
        let say = |v: Value| self.app.emit("meeting-cleanup", v);
        match transcript::auto(&self.db, &|p| self.open(p, "Transcript cleanup"), id, &mut |n, of| say(json!({ "id": id, "state": "reading", "n": n, "of": of }))) {
            Ok(Some(cost)) => say(json!({ "id": id, "state": "done", "cost": cost })),
            Ok(None) => {}
            Err(e) => say(json!({ "id": id, "state": "failed", "error": e })),
        }
    }

    /// The Title model names it when it's still untitled; then the windows and the menu hear of it.
    fn name(&self, id: i64) {
        match title::auto(&self.db, &|p| self.open(p, "Title"), id) {
            Ok(Some(_)) => {
                self.app.emit("meeting-title", json!(id));
                self.app.recent();
            }
            Ok(None) => {}
            Err(e) => eprintln!("meeting {id}: couldn't name it: {e}"), // it stays Untitled; the user can rename it
        }
    }

    /// `meeting-summary` while it runs: "reading" part `n` of `of`, "merging", "writing" with the text so far.
    fn summary_progress(&self, id: i64, p: summary::Progress) {
        self.say_summary(match p {
            summary::Progress::Part(n, of) => json!({ "id": id, "state": "reading", "n": n, "of": of }),
            summary::Progress::Merging => json!({ "id": id, "state": "merging" }),
            summary::Progress::Writing(text) => json!({ "id": id, "state": "writing", "text": text }),
        });
    }

    fn say_summary(&self, v: Value) {
        self.app.emit("meeting-summary", v);
    }

    /// Provider `provider`'s backend, its calls labelled `feature` in the AI log.
    fn open(&self, provider: i64, feature: &str) -> Result<Box<dyn Backend>, String> {
        ai::provider(&self.db.lock().unwrap(), &*self.keys, provider, feature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::{Memory, stub};

    #[derive(Default)]
    struct Heard(Mutex<Vec<(String, Value)>>);
    impl App for Arc<Heard> {
        fn emit(&self, event: &str, payload: Value) {
            self.0.lock().unwrap().push((event.into(), payload));
        }
        fn recent(&self) {
            self.0.lock().unwrap().push(("recent".into(), Value::Null));
        }
    }

    /// An untitled, transcribed meeting (id 1), Main and Fast on an Endpoint that answers every prompt with `reply`.
    fn rig(reply: &'static str) -> (FollowUp, Arc<Heard>, i64, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::storage::open(tmp.path()).unwrap();
        db.execute_batch("INSERT INTO meeting(started, state) VALUES (1, 'done');
            INSERT INTO segment(meeting, start_ms, end_ms, speaker, text) VALUES (1, 0, 900, 'Me', 'um lets plan the launch'), (1, 1000, 1900, 'Them', 'friday works');").unwrap();
        let keys: Arc<dyn Keys> = Arc::new(Memory::default());
        let p = ai::add(&db, &*keys, "openai", "Local", &stub(vec![("/chat/completions", 200, reply)]), "").unwrap().id;
        ai::set(&db, "main", p, "m").unwrap();
        ai::set(&db, "fast", p, "m").unwrap();
        let heard = Arc::new(Heard::default());
        (FollowUp::new(Arc::new(Mutex::new(db)), keys, Box::new(heard.clone())), heard, p, tmp)
    }

    fn events(h: &Heard) -> Vec<String> {
        h.0.lock().unwrap().iter().map(|(e, v)| format!("{e} {}", v["state"].as_str().unwrap_or(""))).map(|s| s.trim().to_string()).collect()
    }

    const LAUNCH: &str = r#"{"choices":[{"message":{"content":"Launch plan"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;

    #[test]
    fn after_a_write_up_it_names_the_meeting_then_summarizes_it_saying_each_step() {
        let (f, heard, _, _t) = rig(LAUNCH);
        f.run(1);
        let e = events(&heard);
        assert_eq!(e[..3], ["meeting-title", "recent", "meeting-summary reading"], "cleanup is off by default: it says nothing");
        assert_eq!(e.last().unwrap(), "meeting-summary done");
        let h = heard.0.lock().unwrap();
        assert_eq!(h[0].1, json!(1), "meeting-title carries the meeting's id");
        assert_eq!((&h[2].1["id"], &h[2].1["n"], &h[2].1["of"]), (&json!(1), &json!(1), &json!(1)));
        assert_eq!(h.last().unwrap().1["version"]["text"], "Launch plan");
    }

    #[test]
    fn transcript_cleanup_goes_first_when_its_on_and_a_failure_still_lets_the_rest_run() {
        let (f, heard, _, _t) = rig(r#"{"error":{"message":"down"}}"#);
        let mut prefs = transcript::prefs(&f.db.lock().unwrap()).unwrap();
        prefs.on = true;
        transcript::set_prefs(&f.db.lock().unwrap(), &prefs).unwrap();
        f.run(1);
        let e = events(&heard);
        assert_eq!(e.first().unwrap(), "meeting-cleanup reading");
        assert!(e.contains(&"meeting-cleanup failed".to_string()), "{e:?}");
        assert_eq!(e.last().unwrap(), "meeting-summary failed", "no title (it failed quietly), and the Summary still ran");
        assert!(!e.iter().any(|x| x == "recent"));
    }

    #[test]
    fn summarize_when_a_meeting_finishes_off_says_skipped() {
        let (f, heard, _, _t) = rig(LAUNCH);
        let mut prefs = summary::prefs(&f.db.lock().unwrap()).unwrap();
        prefs.auto = false;
        summary::set_prefs(&f.db.lock().unwrap(), &prefs).unwrap();
        f.run(1);
        assert_eq!(events(&heard).last().unwrap(), "meeting-summary skipped");
    }

    #[test]
    fn regenerate_runs_on_the_model_picked_and_a_provider_thats_gone_fails_saying_why() {
        let (f, heard, p, _t) = rig(LAUNCH);
        let v = f.regenerate(1, "Recap".into(), "Recap it.".into(), p, "m".into()).unwrap();
        assert_eq!((v.template.as_str(), v.provider.as_str(), v.model.as_str()), ("Recap", "Local", "m"));
        assert_eq!(events(&heard).last().unwrap(), "meeting-summary done");
        assert_eq!(f.regenerate(1, "Recap".into(), "Recap it.".into(), p + 1, "m".into()).unwrap_err(), "That provider isn't there anymore.");
        let h = heard.0.lock().unwrap();
        assert_eq!(h.last().unwrap().1, json!({ "id": 1, "state": "failed", "error": "That provider isn't there anymore." }));
    }
}
