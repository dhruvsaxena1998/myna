//! AI log (Experimental): every call Myna made to an AI provider, kept on this Mac as `ai-log.jsonl` in the data
//! folder, one entry per line, newest last. Not a Trace. `ai::provider()` records into it while it's on.

use crate::ai::{Backend, Cost, Decision as Picked, Model, Reply};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const FILE: &str = "ai-log.jsonl";
/// Entries kept, and how many the file may reach before it's rewritten with the newest `KEEP`.
const KEEP: usize = 500;
const SLACK: usize = 600;

/// One call. Every field but `id`, `at` and `feature` is optional, so lines written before a field existed still load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub id: String,
    /// When the call started, Unix time in milliseconds.
    pub at: u64,
    /// "LLM cleanup", "Summary", "Test"…
    pub feature: String,
    /// "part 3/7", for a feature that makes several calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// "endpoint" or "agent".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// "chat", "chat_stream", "decide", "check", "models", "key_usage" or "transcribe".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    /// The text Myna took from the reply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The reply as it came back, cut at 256 KB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_cut: Option<bool>,
    /// "ok", "error", "timeout", "refused" or "blocked".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    /// The time limit the call had, in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// Transcription only: the clip's length, and the segments that came back as text. Never the audio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<String>,
}

/// A Decision task's call: what it was asked and what it picked.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Decision {
    pub question: String,
    pub options: Vec<String>,
    pub instructions: String,
    /// "decisions" (a Decision API) or "chat" (a chat answer held to a JSON schema).
    pub via: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// An entry as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub id: String,
    pub at: u64,
    pub feature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub method: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// The first 120 characters of the text, else of the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl Entry {
    pub fn summary(&self) -> Summary {
        let preview = self.text.as_deref().filter(|t| !t.is_empty()).or(self.error.as_deref()).map(|t| t.chars().take(120).collect());
        Summary {
            id: self.id.clone(),
            at: self.at,
            feature: self.feature.clone(),
            step: self.step.clone(),
            provider: self.provider.clone(),
            kind: self.kind.clone(),
            model: self.model.clone(),
            method: self.method.clone().unwrap_or_default(),
            outcome: self.outcome.clone().unwrap_or_else(|| "ok".into()),
            total_ms: self.total_ms,
            cost: self.cost.clone(),
            preview,
        }
    }

    fn failed(&self) -> bool {
        self.outcome.as_deref().is_some_and(|o| o != "ok")
    }
}

impl Entry {
    /// A call starting now.
    pub fn new(feature: &str, provider: Option<&str>, agent: bool, model: Option<&str>, method: &str) -> Entry {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Entry {
            id: format!("{at}-{}-{n}", std::process::id()),
            at,
            feature: feature.into(),
            provider: provider.map(Into::into),
            kind: Some(if agent { "agent" } else { "endpoint" }.into()),
            model: model.map(Into::into),
            method: Some(method.into()),
            ..Default::default()
        }
    }
}

/// Starts timing a call; clears any reply an earlier call left on this thread.
pub fn begin() -> Instant {
    crate::ai::take_raw();
    Instant::now()
}

/// Records `e`, started at `start`, with how `r` went; `fill` adds what a success brought back.
pub fn finish<T>(dir: &Path, mut e: Entry, start: Instant, r: &Result<T, String>, fill: impl FnOnce(&mut Entry, &T)) {
    e.total_ms = Some(start.elapsed().as_millis() as u64);
    match r {
        Ok(v) => {
            e.outcome = Some("ok".into());
            fill(&mut e, v);
        }
        Err(why) => {
            e.outcome = Some(outcome(why).into());
            e.error = Some(why.clone());
        }
    }
    let left = crate::ai::take_raw();
    if let Some(raw) = e.raw.take().or(left) {
        let (raw, cut) = cut(raw);
        (e.raw, e.raw_cut) = (Some(raw), cut.then_some(true));
    }
    append(dir, &e);
}

/// A call that never went out, and why.
pub fn blocked(dir: &Path, feature: &str, provider: &str, agent: bool, why: &str) {
    append(dir, &Entry { outcome: Some("blocked".into()), error: Some(why.into()), method: None, ..Entry::new(feature, Some(provider), agent, None, "") });
}

/// How a failed call ended, from the adapters' messages.
fn outcome(why: &str) -> &'static str {
    let w = why.to_lowercase();
    if why == crate::ai::AGENTS_OFF {
        "blocked"
    } else if ["timeout", "timed out", "took longer than"].iter().any(|t| w.contains(t)) {
        "timeout"
    } else if ["isn't one of the options", "didn't answer with one of the options", "refused to answer", "loaded or used tools"].iter().any(|t| w.contains(t)) {
        "refused"
    } else {
        "error"
    }
}

const CUT: usize = 256 * 1024;

/// `raw` up to 256 KB, and whether it was cut.
fn cut(mut raw: String) -> (String, bool) {
    if raw.len() <= CUT {
        return (raw, false);
    }
    let mut at = CUT;
    while !raw.is_char_boundary(at) {
        at -= 1;
    }
    raw.truncate(at);
    raw.push_str("\n[cut at 256 KB]");
    (raw, true)
}

/// `s` with each inline image (`data:image/…;base64,…`) replaced by "[image, N KB]".
fn images(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("data:image/") {
        let tail = &rest[i..];
        let Some(b) = tail.find(";base64,").filter(|b| !tail[..*b].contains(char::is_whitespace)) else {
            out.push_str(&rest[..i + 11]);
            rest = &rest[i + 11..];
            continue;
        };
        let data = &tail[b + 8..];
        let len = data.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))).unwrap_or(data.len());
        out.push_str(&rest[..i]);
        out.push_str(&format!("[image, {} KB]", (len * 3 / 4).div_ceil(1024)));
        rest = &data[len..];
    }
    out + rest
}

/// A WAV's length in milliseconds.
pub fn wav_ms(wav: &[u8]) -> Option<u64> {
    let r = hound::WavReader::new(std::io::Cursor::new(wav)).ok()?;
    Some(r.duration() as u64 * 1000 / r.spec().sample_rate.max(1) as u64)
}

/// A provider's backend, recording each call into the AI log as one feature's.
pub struct Logged {
    inner: Box<dyn Backend>,
    dir: PathBuf,
    provider: Option<String>,
    agent: bool,
    feature: String,
    step: Mutex<Option<String>>,
}

impl Logged {
    pub fn wrap(inner: Box<dyn Backend>, dir: PathBuf, provider: Option<&str>, agent: bool, feature: &str) -> Box<dyn Backend> {
        Box::new(Logged { inner, dir, provider: provider.map(Into::into), agent, feature: feature.into(), step: Mutex::new(None) })
    }

    fn entry(&self, model: Option<&str>, method: &str) -> Entry {
        Entry { step: self.step.lock().unwrap().clone(), ..Entry::new(&self.feature, self.provider.as_deref(), self.agent, model, method) }
    }

    fn sent(&self, model: &str, method: &str, system: Option<&str>, prompt: &str, secs: u64) -> Entry {
        Entry { system: system.map(images), prompt: Some(images(prompt)), limit_s: Some(secs), ..self.entry(Some(model), method) }
    }

    /// Model lists and key usage are logged only for Test, so browsing Settings doesn't flood the log.
    fn listing<T>(&self, method: &str, call: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        if self.feature != "Test" {
            return call();
        }
        let (e, start) = (self.entry(None, method), begin());
        let r = call();
        finish(&self.dir, e, start, &r, |_, _| {});
        r
    }
}

fn replied(e: &mut Entry, r: &Reply) {
    e.text = Some(r.text.clone());
    e.raw = r.raw.clone();
    (e.tokens_in, e.tokens_out) = (r.tokens.map(|t| t.0), r.tokens.map(|t| t.1));
    e.cost = Some(r.cost.clone());
}

impl Backend for Logged {
    fn models(&self) -> Result<Vec<Model>, String> {
        self.listing("models", || self.inner.models())
    }

    fn key_usage(&self) -> Result<Option<(f64, Option<f64>)>, String> {
        self.listing("key_usage", || self.inner.key_usage())
    }

    fn chat(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64) -> Result<Reply, String> {
        let (e, start) = (self.sent(model, "chat", system, prompt, secs), begin());
        let r = self.inner.chat(model, system, prompt, secs);
        finish(&self.dir, e, start, &r, replied);
        r
    }

    fn chat_stream(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64, on: &mut dyn FnMut(&str)) -> Result<Reply, String> {
        let (mut e, start) = (self.sent(model, "chat_stream", system, prompt, secs), begin());
        let mut first = None;
        let r = self.inner.chat_stream(model, system, prompt, secs, &mut |t| {
            first.get_or_insert_with(|| start.elapsed().as_millis() as u64);
            on(t)
        });
        e.ttft_ms = first;
        finish(&self.dir, e, start, &r, replied);
        r
    }

    fn check(&self) -> Result<(), String> {
        let (e, start) = (self.entry(None, "check"), begin());
        let r = self.inner.check();
        finish(&self.dir, e, start, &r, |_, _| {});
        r
    }

    fn decision_models(&self) -> Result<Vec<Model>, String> {
        self.inner.decision_models()
    }

    fn decide(&self, model: &str, question: &str, options: &[String], instructions: &str, decisions: bool, secs: u64) -> Result<Picked, String> {
        let d = Decision { question: question.into(), options: options.to_vec(), instructions: instructions.into(), via: if decisions { "decisions" } else { "chat" }.into(), ..Default::default() };
        let (e, start) = (Entry { decision: Some(d), limit_s: Some(secs), ..self.entry(Some(model), "decide") }, begin());
        let r = self.inner.decide(model, question, options, instructions, decisions, secs);
        finish(&self.dir, e, start, &r, |e, p| {
            if let Some(d) = e.decision.as_mut() {
                (d.choice, d.confidence) = (Some(p.choice.clone()), p.confidence);
            }
            (e.text, e.raw, e.cost) = (Some(p.choice.clone()), p.raw.clone(), Some(p.cost.clone()));
        });
        r
    }

    fn step(&self, step: Option<&str>) {
        *self.step.lock().unwrap() = step.map(Into::into);
    }
}

/// Lines in each file this run has appended to, known after its first append; the lock guards the files too.
static LINES: Mutex<Option<HashMap<PathBuf, usize>>> = Mutex::new(None);
/// Told of each entry once it's written: the app sends it to the windows as `ai-log`.
static HEARD: OnceLock<Box<dyn Fn(Summary) + Send + Sync>> = OnceLock::new();

pub fn on_append(f: impl Fn(Summary) + Send + Sync + 'static) {
    let _ = HEARD.set(Box::new(f));
}

/// Adds `e` as one line. Never fails the caller: a failed write goes to stderr.
pub fn append(dir: &Path, e: &Entry) {
    let path = dir.join(FILE);
    let written = (|| -> Result<(), String> {
        let mut line = serde_json::to_string(e).map_err(|e| e.to_string())?;
        line.push('\n');
        let mut lines = LINES.lock().unwrap_or_else(|p| p.into_inner());
        let lines = lines.get_or_insert_with(HashMap::new);
        let n = match lines.get(&path) {
            Some(n) => *n,
            None => std::fs::read_to_string(&path).map_or(0, |s| s.lines().count()),
        };
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| e.to_string())?;
        f.write_all(line.as_bytes()).and_then(|_| f.flush()).map_err(|e| e.to_string())?;
        let mut n = n + 1;
        if n > SLACK {
            let all = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let all: Vec<&str> = all.lines().collect();
            let kept = all[all.len().saturating_sub(KEEP)..].join("\n") + "\n";
            let tmp = dir.join(format!("{FILE}.tmp"));
            std::fs::write(&tmp, kept).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| e.to_string())?;
            n = all.len().min(KEEP);
        }
        lines.insert(path.clone(), n);
        Ok(())
    })();
    match written {
        Ok(()) => {
            if let Some(f) = HEARD.get() {
                f(e.summary());
            }
        }
        Err(why) => eprintln!("AI log: couldn't write {}: {why}", path.display()),
    }
}

/// Every entry that parses, newest first.
fn all(dir: &Path) -> Vec<Entry> {
    let _held = LINES.lock().unwrap_or_else(|p| p.into_inner());
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap_or_default();
    text.lines().rev().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// Summaries newest first, of one feature's calls when `feature` is given, of failed calls only when `errors`.
pub fn list(dir: &Path, feature: Option<&str>, errors: bool) -> Vec<Summary> {
    all(dir).iter().filter(|e| feature.is_none_or(|f| e.feature == f) && (!errors || e.failed())).map(Entry::summary).collect()
}

pub fn get(dir: &Path, id: &str) -> Option<Entry> {
    all(dir).into_iter().find(|e| e.id == id)
}

pub fn clear(dir: &Path) -> Result<(), String> {
    let path = dir.join(FILE);
    let mut lines = LINES.lock().unwrap_or_else(|p| p.into_inner());
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("Couldn't clear the AI log: {e}")),
        _ => {}
    }
    lines.get_or_insert_with(HashMap::new).insert(path, 0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u64, feature: &str) -> Entry {
        Entry { id: format!("e{n}"), at: n, feature: feature.into(), method: Some("chat".into()), outcome: Some("ok".into()), text: Some(format!("reply {n}")), ..Default::default() }
    }

    fn ids(l: &[Summary]) -> Vec<String> {
        l.iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn entries_list_newest_first_open_whole_and_clear() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), &entry(1, "LLM cleanup"));
        append(tmp.path(), &Entry { outcome: Some("timeout".into()), error: Some("timed out".into()), text: None, ..entry(2, "Summary") });
        append(tmp.path(), &Entry { system: Some("Be brief".into()), ..entry(3, "LLM cleanup") });
        assert_eq!(ids(&list(tmp.path(), None, false)), ["e3", "e2", "e1"]);
        assert_eq!(ids(&list(tmp.path(), Some("LLM cleanup"), false)), ["e3", "e1"]);
        let failed = list(tmp.path(), None, true);
        assert_eq!((ids(&failed), failed[0].outcome.as_str(), failed[0].preview.as_deref()), (vec!["e2".to_string()], "timeout", Some("timed out")));
        assert_eq!(get(tmp.path(), "e3").unwrap().system.as_deref(), Some("Be brief"));
        assert!(get(tmp.path(), "nope").is_none());
        clear(tmp.path()).unwrap();
        assert!(list(tmp.path(), None, false).is_empty());
        append(tmp.path(), &entry(4, "Title"));
        assert_eq!(ids(&list(tmp.path(), None, false)), ["e4"]);
    }

    #[test]
    fn it_keeps_the_newest_500_once_it_passes_600() {
        let tmp = tempfile::tempdir().unwrap();
        for n in 1..=600 {
            append(tmp.path(), &entry(n, "Test"));
        }
        assert_eq!(list(tmp.path(), None, false).len(), 600, "no rewrite until it passes the slack");
        append(tmp.path(), &entry(601, "Test"));
        let l = list(tmp.path(), None, false);
        assert_eq!((l.len(), l[0].id.as_str(), l[499].id.as_str()), (500, "e601", "e102"));
    }

    #[test]
    fn a_bad_line_is_skipped_and_an_old_line_without_newer_fields_loads() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), &entry(1, "Test"));
        let mut f = std::fs::OpenOptions::new().append(true).open(tmp.path().join(FILE)).unwrap();
        f.write_all(b"{\"id\":\"half\n{\"id\":\"old\",\"at\":5,\"feature\":\"Title\"}\n").unwrap();
        append(tmp.path(), &entry(2, "Test"));
        let l = list(tmp.path(), None, false);
        assert_eq!(ids(&l), ["e2", "old", "e1"]);
        assert_eq!((l[1].method.as_str(), l[1].outcome.as_str()), ("", "ok"));
    }

    #[test]
    fn appends_from_many_threads_each_land_as_one_whole_line() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || (0..20).for_each(|n| append(&dir, &Entry { prompt: Some("x".repeat(20_000)), ..entry(t * 100 + n, "Summary") })))
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(list(&dir, None, false).len(), 160);
    }

    #[test]
    fn a_failed_write_doesnt_fail_the_caller() {
        append(Path::new("/nonexistent/myna"), &entry(1, "Test"));
        assert!(list(Path::new("/nonexistent/myna"), None, false).is_empty());
    }
}
