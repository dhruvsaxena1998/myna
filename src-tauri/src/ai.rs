//! AI providers: where Myna sends text for an LLM to answer, and which model each slot uses.
//! Records live in SQLite, keys in the macOS Keychain (never in SQLite, never sent to the frontend).
//! Every call goes through the `Backend` trait, like `runtime::Transcriber`; one adapter, OpenAI-compatible chat plus
//! OpenRouter's decisions route.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const KINDS: [&str; 2] = ["openai", "openrouter"];
const SLOTS: [&str; 2] = ["main", "fast"];
/// Features with their own model, as a setting: (key, label, the slot it uses while the key is missing).
/// Meeting speech-to-text's "slot" is "local": while its key is missing, the Local model transcribes.
const OWN: [(&str, &str, &str); 5] = [
    ("summary_model", "Summary model", "main"),
    ("transcript_cleanup_model", "Cleanup model", "fast"),
    (DECIDE, "Decision model", "fast"),
    (crate::title::KEY, "Title model", "fast"),
    (STT, "Meeting speech-to-text", "local"),
];
/// The Decision model: a decision model picked from an OpenRouter Endpoint, else the Fast model with a JSON schema.
const DECIDE: &str = "decision_model";
/// Meeting speech-to-text on an Endpoint (#45), after the meeting only.
pub const STT: &str = "meeting_stt_endpoint";
/// Their models aren't in the saved (chat) model list, so missing from it says nothing.
const UNLISTED: [&str; 2] = [DECIDE, STT];
/// About 5 s of speech, sent by Test: a model is picked only if its reply carries segment timestamps.
const CLIP: &[u8] = include_bytes!("../sounds/stt-test.wav");
const SERVICE: &str = "com.dhruvsaxena.myna.ai-provider";

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Provider {
    pub(crate) id: i64,
    kind: String,
    name: String,
    url: String,
    has_key: bool,
    off_device: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Route {
    provider: i64,
    model: String,
    /// The provider's last good list no longer has this model; features treat the slot as Not set.
    gone: bool,
}

#[derive(Debug, Serialize)]
pub struct State {
    providers: Vec<Provider>,
    main: Option<Route>,
    fast: Option<Route>,
    /// The Decision model, in `feature_get`'s form.
    decide: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Model {
    pub id: String,
    pub input_per_m: Option<f64>,
    pub output_per_m: Option<f64>,
    pub context: Option<u64>,
    pub images: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Cost {
    Usd { usd: f64 },
    Free,
    Unknown,
}

/// One of the options, how sure the model was (decision models only) and how long the call took.
#[derive(Debug, Serialize, PartialEq)]
pub struct Decision {
    pub choice: String,
    pub confidence: Option<f64>,
    pub cost: Cost,
    pub ms: u64,
    /// The reply as it came back, for the AI log; in memory only.
    #[serde(skip)]
    pub raw: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub text: String,
    pub tokens: Option<(u64, u64)>,
    pub cost: Cost,
    /// The reply as it came back (the HTTP body, the SSE lines, an Agent's stream-json lines), for the AI log.
    pub raw: Option<String>,
}

thread_local! {
    /// The last reply an adapter read on this thread, so the AI log has it on the error path too.
    static RAW: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn keep_raw(raw: Option<String>) {
    RAW.with(|r| *r.borrow_mut() = raw);
}

pub(crate) fn take_raw() -> Option<String> {
    RAW.with(|r| r.borrow_mut().take())
}

/// `r`, keeping `raw` for the AI log when it failed.
fn with_raw<T>(raw: Option<String>, r: Result<T, String>) -> Result<T, String> {
    if r.is_err() {
        keep_raw(raw);
    }
    r
}

/// What Test found: `models` for an OpenAI-compatible server, `used` and `limit` (USD) for an OpenRouter key.
#[derive(Debug, Serialize, PartialEq)]
pub struct Check {
    ms: u64,
    models: Option<usize>,
    used: Option<f64>,
    limit: Option<f64>,
}

pub trait Backend: Send {
    fn models(&self) -> Result<Vec<Model>, String>;
    /// USD used and limit of the key, for a service that reports them (OpenRouter); None for the rest.
    fn key_usage(&self) -> Result<Option<(f64, Option<f64>)>, String>;
    /// One user message, after an optional system message, answered within `secs` seconds.
    fn chat(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64) -> Result<Reply, String>;
    /// `chat`, handing `on` the reply's text so far each time more arrives, for a service that streams; others hand it once, whole.
    fn chat_stream(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64, on: &mut dyn FnMut(&str)) -> Result<Reply, String> {
        let r = self.chat(model, system, prompt, secs)?;
        on(&r.text);
        Ok(r)
    }
    /// Test's own check before the usual one: an Agent runs once, to see it's installed and signed in.
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    /// The service's decision models (OpenRouter); none elsewhere.
    fn decision_models(&self) -> Result<Vec<Model>, String> {
        Ok(vec![])
    }
    /// Picks one of `options`: through the decisions route when `decisions`, else a chat answer held to a JSON schema.
    fn decide(&self, _model: &str, _question: &str, _options: &[String], _instructions: &str, _decisions: bool, _secs: u64) -> Result<Decision, String> {
        Err("This provider can't make decisions".into())
    }
    /// Names the next calls' step for the AI log ("part 3/7"), None for the feature's own call; nothing unlogged.
    fn step(&self, _step: Option<&str>) {}
}

/// Where API keys live. The real one is the Keychain; tests use memory.
pub trait Keys: Send + Sync {
    fn get(&self, id: i64) -> Result<Option<String>, String>;
    fn set(&self, id: i64, key: &str) -> Result<(), String>;
    fn delete(&self, id: i64) -> Result<(), String>;
}

pub struct Keychain;

const NOT_FOUND: i32 = -25300; // errSecItemNotFound

impl Keys for Keychain {
    fn get(&self, id: i64) -> Result<Option<String>, String> {
        match security_framework::passwords::get_generic_password(SERVICE, &id.to_string()) {
            Ok(b) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(e) => Err(format!("Couldn't read the key from the Keychain: {e}")),
        }
    }
    fn set(&self, id: i64, key: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(SERVICE, &id.to_string(), key.as_bytes())
            .map_err(|e| format!("Couldn't save the key to the Keychain: {e}"))
    }
    fn delete(&self, id: i64) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(SERVICE, &id.to_string()) {
            Err(e) if e.code() != NOT_FOUND => Err(format!("Couldn't remove the key from the Keychain: {e}")),
            _ => Ok(()),
        }
    }
}

fn host(url: &str) -> Option<String> {
    let u: ureq::http::Uri = url.parse().ok()?;
    if !matches!(u.scheme_str(), Some("http" | "https")) {
        return None;
    }
    u.host().map(|h| h.trim_matches(|c| c == '[' || c == ']').to_lowercase())
}

/// Anything but loopback is off this Mac, LAN included.
pub fn off_device(url: &str) -> bool {
    match host(url) {
        Some(h) => h != "localhost" && !h.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback()),
        None => true,
    }
}

// ---- OpenAI-compatible adapter ----

pub struct OpenAi {
    url: String,
    key: Option<String>,
    /// OpenRouter's list carries prices, context and image support; other Endpoints give ids only.
    rich: bool,
}

impl OpenAi {
    fn send(&self, path: &str, body: Option<String>, secs: u64) -> Result<Value, String> {
        self.send_as(path, body.map(|b| (b.into_bytes(), "application/json".to_string())), secs)
    }

    /// `send` with a body of any type: (bytes, Content-Type).
    fn send_as(&self, path: &str, body: Option<(Vec<u8>, String)>, secs: u64) -> Result<Value, String> {
        let mut res = self.request(path, body, secs)?;
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().map_err(|e| format!("Couldn't reach {}: {e}", self.url))?;
        keep_raw(Some(text.clone()));
        self.answer(status, &text)
    }

    /// The request sent, its answer not yet read.
    fn request(&self, path: &str, body: Option<(Vec<u8>, String)>, secs: u64) -> Result<ureq::http::Response<ureq::Body>, String> {
        let unreachable = |e: String| format!("Couldn't reach {}: {e}", self.url);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(secs)))
            .http_status_as_error(false)
            .build()
            .into();
        let (body, kind) = body.map_or((None, "application/json".into()), |(b, k)| (Some(b), k));
        let mut req = ureq::http::Request::builder()
            .method(if body.is_some() { "POST" } else { "GET" })
            .uri(format!("{}{path}", self.url))
            .header("Content-Type", kind);
        if let Some(k) = &self.key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        let req = req.body(body.unwrap_or_default()).map_err(|_| unreachable("that key can't be sent".into()))?;
        agent.run(req).map_err(|e| unreachable(e.to_string()))
    }

    /// An answer's JSON, or why there is none: a refused key, the service's error message, or not JSON.
    fn answer(&self, status: u16, text: &str) -> Result<Value, String> {
        let unreachable = |e: String| format!("Couldn't reach {}: {e}", self.url);
        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if status == 401 || status == 403 {
            return Err(format!("{} refused the key (HTTP {status})", self.url));
        }
        if status >= 400 {
            let why = json["error"]["message"].as_str().map(str::to_string).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(unreachable(why));
        }
        if json.is_null() {
            return Err(unreachable("it didn't answer with JSON".into()));
        }
        Ok(json)
    }
}

/// USD per token as a string -> USD per 1M tokens. Negative means "varies" (routers), so none.
fn per_m(v: &Value) -> Option<f64> {
    v.as_str()?.parse::<f64>().ok().filter(|p| *p >= 0.0).map(|p| p * 1e6)
}

/// First match wins: the reply's own cost, price x tokens, loopback is free, else unknown.
fn cost_of(reported: Option<f64>, price: Option<(f64, f64)>, tokens: Option<(u64, u64)>, loopback: bool) -> Cost {
    if let Some(usd) = reported {
        return Cost::Usd { usd };
    }
    if let (Some((i, o)), Some((inp, out))) = (price, tokens) {
        return Cost::Usd { usd: (i * inp as f64 + o * out as f64) / 1e6 };
    }
    if loopback { Cost::Free } else { Cost::Unknown }
}

impl OpenAi {
    fn list(&self, path: &str) -> Result<Vec<Model>, String> {
        let json = self.send(path, None, 15)?;
        let list = json["data"].as_array().ok_or_else(|| format!("Couldn't read the model list from {}", self.url))?;
        Ok(list
            .iter()
            .filter_map(|m| {
                let id = m["id"].as_str()?.to_string();
                if !self.rich {
                    return Some(Model { id, ..Default::default() });
                }
                let images = m["architecture"]["input_modalities"].as_array().map(|a| a.iter().any(|x| x == "image"));
                Some(Model {
                    id,
                    input_per_m: per_m(&m["pricing"]["prompt"]),
                    output_per_m: per_m(&m["pricing"]["completion"]),
                    context: m["context_length"].as_u64(),
                    images,
                })
            })
            .collect())
    }

    fn ask(&self, model: &str, system: Option<&str>, prompt: &str, format: Option<Value>, secs: u64) -> Result<Reply, String> {
        let mut body = json!({"model": model, "messages": messages(system, prompt)});
        if let Some(f) = format {
            body["response_format"] = f;
        }
        let json = self.send("/chat/completions", Some(body.to_string()), secs)?;
        let raw = take_raw(); // before `reply`, which may fetch prices
        let text = with_raw(raw.clone(), json["choices"][0]["message"]["content"].as_str().ok_or_else(|| format!("{} sent no reply", self.url)))?.to_string();
        Ok(Reply { raw, ..self.reply(model, text, &json["usage"]) })
    }

    /// `ask` as a stream (server-sent events), `on` getting the text so far as it grows. A server that ignores
    /// `stream` and answers whole is read as `ask` reads it.
    fn ask_stream(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64, on: &mut dyn FnMut(&str)) -> Result<Reply, String> {
        let mut raw = String::new();
        let r = self.ask_stream_into(model, system, prompt, secs, on, &mut raw);
        let raw = (!raw.is_empty()).then_some(raw).or_else(take_raw);
        match r {
            Ok(r) => Ok(Reply { raw, ..r }),
            Err(e) => with_raw(raw, Err(e)),
        }
    }

    /// `ask_stream`, every line read kept in `raw`.
    fn ask_stream_into(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64, on: &mut dyn FnMut(&str), raw: &mut String) -> Result<Reply, String> {
        use std::io::BufRead;
        let unreachable = |e: String| format!("Couldn't reach {}: {e}", self.url);
        let body = json!({"model": model, "messages": messages(system, prompt), "stream": true, "stream_options": {"include_usage": true}});
        let mut res = self.request("/chat/completions", Some((body.to_string().into_bytes(), "application/json".into())), secs)?;
        let status = res.status().as_u16();
        if status >= 400 {
            let text = res.body_mut().read_to_string().unwrap_or_default();
            raw.push_str(&text);
            return self.answer(status, &text).and_then(|_| Err(unreachable(format!("HTTP {status}"))));
        }
        let (mut text, mut usage, mut whole, mut streamed) = (String::new(), Value::Null, String::new(), false);
        for line in std::io::BufReader::new(res.body_mut().as_reader()).lines() {
            let line = line.map_err(|e| unreachable(e.to_string()))?;
            raw.push_str(&line);
            raw.push('\n');
            let Some(data) = line.strip_prefix("data:") else {
                // ":" lines keep the connection open; anything else is a reply sent whole.
                if !line.starts_with(':') {
                    whole.push_str(&line);
                    whole.push('\n');
                }
                continue;
            };
            streamed = true;
            let data = data.trim();
            if data == "[DONE]" {
                break;
            }
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(why) = v["error"]["message"].as_str() {
                return Err(unreachable(why.into()));
            }
            if let Some(d) = v["choices"][0]["delta"]["content"].as_str().filter(|d| !d.is_empty()) {
                text.push_str(d);
                on(&text);
            }
            if v["usage"].is_object() {
                usage = v["usage"].clone();
            }
        }
        if !streamed {
            let json = self.answer(status, &whole)?;
            text = json["choices"][0]["message"]["content"].as_str().ok_or_else(|| format!("{} sent no reply", self.url))?.to_string();
            usage = json["usage"].clone();
            on(&text);
        }
        if text.is_empty() {
            return Err(format!("{} sent no reply", self.url));
        }
        Ok(self.reply(model, text, &usage))
    }

    /// A reply's text with its tokens and cost, from the `usage` the service sent.
    fn reply(&self, model: &str, text: String, usage: &Value) -> Reply {
        let tokens = usage["prompt_tokens"].as_u64().zip(usage["completion_tokens"].as_u64());
        let reported = usage["cost"].as_f64();
        // Only fetch prices when the reply didn't carry its own cost.
        let price = match (reported, self.rich) {
            (None, true) => self.models().ok().and_then(|l| l.into_iter().find(|m| m.id == model)).and_then(|m| m.input_per_m.zip(m.output_per_m)),
            _ => None,
        };
        Reply { text, tokens, cost: cost_of(reported, price, tokens, !off_device(&self.url)), raw: None }
    }
}

/// A chat's messages: the system message, when there is one, then the user's.
fn messages(system: Option<&str>, prompt: &str) -> Vec<Value> {
    system.map(|s| json!({"role": "system", "content": s})).into_iter().chain([json!({"role": "user", "content": prompt})]).collect()
}

impl Backend for OpenAi {
    fn models(&self) -> Result<Vec<Model>, String> {
        self.list("/models")
    }

    fn key_usage(&self) -> Result<Option<(f64, Option<f64>)>, String> {
        if !self.rich {
            return Ok(None);
        }
        let d = &self.send("/key", None, 15)?["data"];
        let used = d["usage"].as_f64().ok_or_else(|| format!("Couldn't read the key's usage from {}", self.url))?;
        Ok(Some((used, d["limit"].as_f64())))
    }

    fn chat(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64) -> Result<Reply, String> {
        self.ask(model, system, prompt, None, secs)
    }

    fn chat_stream(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64, on: &mut dyn FnMut(&str)) -> Result<Reply, String> {
        self.ask_stream(model, system, prompt, secs, on)
    }

    fn decision_models(&self) -> Result<Vec<Model>, String> {
        if !self.rich {
            return Ok(vec![]);
        }
        self.list("/models?output_modalities=decisions") // decision models are left out of the plain list
    }

    fn decide(&self, model: &str, question: &str, options: &[String], instructions: &str, decisions: bool, secs: u64) -> Result<Decision, String> {
        let picked = |c: &str| options.iter().find(|o| *o == c).cloned().ok_or_else(|| format!("{model} picked “{c}”, which isn't one of the options"));
        if !decisions {
            let schema = json!({"type": "object", "properties": {"choice": {"type": "string", "enum": options}}, "required": ["choice"], "additionalProperties": false});
            let prompt = format!("{question}\n\nOptions:\n{}", options.iter().map(|o| format!("- {o}")).collect::<Vec<_>>().join("\n"));
            let format = json!({"type": "json_schema", "json_schema": {"name": "decision", "strict": true, "schema": schema}});
            let r = self.ask(model, Some(instructions), &prompt, Some(format), secs)?;
            let c = serde_json::from_str::<Value>(&r.text).ok().and_then(|v| v["choice"].as_str().map(str::to_string)).ok_or_else(|| format!("{model} didn't answer with one of the options"));
            let choice = with_raw(r.raw.clone(), c.and_then(|c| picked(&c)))?;
            return Ok(Decision { choice, confidence: None, cost: r.cost, ms: 0, raw: r.raw });
        }
        // OpenRouter's Jev-shape route; a question without `instructions` is refused.
        let criteria: serde_json::Map<String, Value> = options.iter().map(|o| (o.clone(), json!(o))).collect();
        let body = json!({"model": model, "state": question, "questions": {"pick": {"type": "choice", "instructions": instructions, "criteria": criteria}}});
        // It sits beside /v1 (…/api/alpha/decisions), not under it.
        let root = OpenAi { url: self.url.trim_end_matches("/v1").into(), key: self.key.clone(), rich: true };
        let json = root.send("/alpha/decisions", Some(body.to_string()), secs)?;
        let raw = take_raw(); // before `decision_models`, which may fetch prices
        let a = &json["answers"]["pick"];
        if a["type"] == "refusal" {
            return with_raw(raw, Err(format!("{model} refused to answer")));
        }
        let choice = with_raw(raw.clone(), a["choice"].as_str().ok_or_else(|| format!("{model} sent no choice")).and_then(picked))?;
        let u = &json["usage"];
        let tokens = u["input_tokens"].as_u64().map(|i| (i, u["output_tokens"].as_u64().unwrap_or(0)));
        let price = match u["cost"].as_f64() {
            None => self.decision_models().ok().and_then(|l| l.into_iter().find(|m| m.id == model)).and_then(|m| Some((m.input_per_m?, m.output_per_m.unwrap_or(0.0)))),
            Some(_) => None,
        };
        Ok(Decision { choice, confidence: a["confidence"].as_f64(), cost: cost_of(u["cost"].as_f64(), price, tokens, false), ms: 0, raw })
    }
}

impl OpenAi {
    /// One WAV to `{url}/audio/transcriptions`: its segments, times in seconds from its start, and the cost.
    /// A reply without `segments` is an error: Myna can't place untimed words in a meeting.
    pub fn transcribe(&self, model: &str, wav: &[u8], secs: u64) -> Result<(Vec<crate::runtime::Segment>, Cost), String> {
        const B: &str = "myna-form-boundary-7d1c";
        let mut body = Vec::with_capacity(wav.len() + 600);
        for (k, v) in [("model", model), ("response_format", "verbose_json"), ("timestamp_granularities[]", "segment")] {
            body.extend_from_slice(format!("--{B}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
        }
        body.extend_from_slice(format!("--{B}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
        body.extend_from_slice(wav);
        body.extend_from_slice(format!("\r\n--{B}--\r\n").as_bytes());
        let json = self.send_as("/audio/transcriptions", Some((body, format!("multipart/form-data; boundary={B}"))), secs)?;
        let segs = json["segments"].as_array().ok_or("it sent no segment timestamps")?;
        let segs = segs
            .iter()
            .map(|s| Some(crate::runtime::Segment { start: s["start"].as_f64()? as f32, end: s["end"].as_f64()? as f32, text: s["text"].as_str()?.into() }))
            .collect::<Option<Vec<_>>>()
            .ok_or("it sent segments without times")?;
        Ok((segs, cost_of(json["usage"]["cost"].as_f64(), None, None, !off_device(&self.url))))
    }
}

/// Meeting speech-to-text's Endpoint: who and which model, ready to send to.
pub struct SttEndpoint {
    pub provider: String,
    pub model: String,
    pub off: bool,
    api: OpenAi,
    /// The data folder while the AI log is on.
    log: Option<std::path::PathBuf>,
}

impl SttEndpoint {
    /// One chunk, within OpenRouter's 60 s upstream timeout (plus the upload).
    pub fn send(&self, wav: &[u8]) -> Result<(Vec<crate::runtime::Segment>, Cost), String> {
        let Some(dir) = &self.log else { return self.api.transcribe(&self.model, wav, 90) };
        let e = crate::ai_log::Entry { audio_ms: crate::ai_log::wav_ms(wav), limit_s: Some(90), ..crate::ai_log::Entry::new("Meeting speech-to-text", Some(&self.provider), false, Some(&self.model), "transcribe") };
        let start = crate::ai_log::begin();
        let r = self.api.transcribe(&self.model, wav, 90);
        crate::ai_log::finish(dir, e, start, &r, |e, (segs, cost)| {
            e.segments = Some(segs.iter().map(|s| format!("[{:.1}–{:.1}] {}", s.start, s.end, s.text.trim())).collect::<Vec<_>>().join("\n"));
            e.cost = Some(cost.clone());
        });
        r
    }
}

/// Meeting speech-to-text's Endpoint: None while the Local model is picked (the default), else the provider and model,
/// or "Meeting speech-to-text: Not set" once its provider was removed.
pub fn stt_endpoint(db: &Connection, keys: &dyn Keys) -> Result<Option<SttEndpoint>, String> {
    if matches!(own(db, STT)?, Own::Slot) {
        return Ok(None);
    }
    let (r, model) = feature(db, STT)?;
    Ok(Some(SttEndpoint { provider: r.name, model, off: off_device(&r.url), api: adapter(db, keys, r.id)?, log: log_dir(db) }))
}

/// Provider `provider`'s `model`, for one Transcribe again; the setting for new meetings stays as it is.
pub fn stt_endpoint_for(db: &Connection, keys: &dyn Keys, provider: i64, model: &str) -> Result<SttEndpoint, String> {
    let r = find(db, provider)?;
    Ok(SttEndpoint { provider: r.name, model: model.into(), off: off_device(&r.url), api: adapter(db, keys, r.id)?, log: log_dir(db) })
}

/// OpenRouter's transcription models; other providers have their model id typed in, so none.
pub fn stt_models(db: &Mutex<Connection>, keys: &dyn Keys, id: i64) -> Result<Vec<Model>, String> {
    let a = adapter(&db.lock().unwrap(), keys, id)?; // the lock is not held while waiting on the network
    if !a.rich {
        return Ok(vec![]);
    }
    a.list("/models?output_modalities=transcription")
}

/// Provider `id`'s decision models; none but OpenRouter's have any.
pub fn decision_models(db: &Mutex<Connection>, keys: &dyn Keys, id: i64) -> Result<Vec<Model>, String> {
    let p = provider(&db.lock().unwrap(), keys, id, "Decision models")?; // the lock is not held while waiting on the network
    p.decision_models()
}

/// What Test found: how long the clip took, how many segments came back, and the cost.
#[derive(Debug, Serialize, PartialEq)]
pub struct SttCheck {
    ms: u64,
    segments: usize,
    cost: Cost,
}

/// Sends the bundled clip to `model` on provider `id`; picks it for meetings only if the reply has timestamped segments.
/// Called without the database held: it waits on the network.
pub fn stt_test(db: &Mutex<Connection>, keys: &dyn Keys, id: i64, model: &str) -> Result<SttCheck, String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("Enter a model".into());
    }
    let a = adapter(&db.lock().unwrap(), keys, id)?;
    let start = Instant::now();
    let (segs, cost) = a.transcribe(model, CLIP, 90).map_err(|e| format!("{model} can't transcribe meetings: {e}"))?;
    if segs.is_empty() {
        return Err(format!("{model} can't transcribe meetings: it sent no segment timestamps"));
    }
    let ms = start.elapsed().as_millis() as u64;
    feature_set(&db.lock().unwrap(), STT, &format!("{id}|{model}"))?;
    Ok(SttCheck { ms, segments: segs.len(), cost })
}

// ---- records ----

fn record(r: &rusqlite::Row) -> rusqlite::Result<Record> {
    Ok(Record { id: r.get(0)?, kind: r.get(1)?, name: r.get(2)?, url: r.get(3)? })
}

fn find(db: &Connection, id: i64) -> Result<Record, String> {
    db.query_row("SELECT id, kind, name, url FROM provider WHERE id = ?1", [id], record)
        .optional().map_err(|e| e.to_string())?
        .ok_or_else(|| "That provider no longer exists".to_string())
}

fn view(r: Record, keys: &dyn Keys) -> Result<Provider, String> {
    Ok(Provider { has_key: keys.get(r.id)?.is_some(), off_device: off_device(&r.url), id: r.id, kind: r.kind, name: r.name, url: r.url })
}

fn get_route(db: &Connection, slot: &str) -> Result<Option<Route>, String> {
    let r = db.query_row("SELECT provider_id, model FROM route WHERE slot = ?1", [slot], |r| Ok((r.get(0)?, r.get::<_, String>(1)?)))
        .optional().map_err(|e| e.to_string())?;
    Ok(r.map(|(provider, model)| Route { gone: gone(db, provider, &model), provider, model }))
}

// No saved list means unknown, not gone: a failed fetch never marks a model as gone.
fn gone(db: &Connection, provider: i64, model: &str) -> bool {
    cached(db, provider).is_some_and(|l| !l.iter().any(|m| m.id == model))
}

pub fn get(db: &Connection, keys: &dyn Keys) -> Result<State, String> {
    let mut q = db.prepare("SELECT id, kind, name, url FROM provider ORDER BY position IS NULL, position, id").map_err(|e| e.to_string())?;
    let rows = q.query_map([], record).map_err(|e| e.to_string())?;
    let providers = rows.map(|r| view(r.map_err(|e| e.to_string())?, keys)).collect::<Result<_, _>>()?;
    Ok(State { providers, main: get_route(db, "main")?, fast: get_route(db, "fast")?, decide: feature_get(db, DECIDE)?.0 })
}

pub fn add(db: &Connection, keys: &dyn Keys, kind: &str, name: &str, url: &str, key: &str) -> Result<Provider, String> {
    let (name, url, key) = (name.trim(), url.trim().trim_end_matches('/'), key.trim());
    let agent = crate::agent::is_agent(kind);
    if !KINDS.contains(&kind) && !agent {
        return Err("Pick a kind of provider".into());
    }
    if agent && !agents_on(db)? {
        return Err(AGENTS_OFF.into());
    }
    // An Agent has no address or key: it's the CLI on this Mac, signed in on its own.
    let (url, key) = if agent { ("", "") } else { (url, key) };
    if name.is_empty() || name.chars().count() > 40 {
        return Err("Give the provider a name of up to 40 characters".into());
    }
    let taken = db.query_row("SELECT 1 FROM provider WHERE name = ?1 COLLATE NOCASE", [name], |_| Ok(())).optional().map_err(|e| e.to_string())?;
    if taken.is_some() {
        return Err(format!("There is already a provider called {name}"));
    }
    if !agent && host(url).is_none() {
        return Err("Enter a web address starting with http:// or https://".into());
    }
    if kind == "openrouter" && !key.starts_with("sk-or-") {
        return Err("An OpenRouter key starts with sk-or-".into());
    }
    db.execute("INSERT INTO provider(kind, name, url, position) VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM provider))", params![kind, name, url]).map_err(|e| e.to_string())?;
    let id = db.last_insert_rowid();
    if !key.is_empty() && let Err(e) = keys.set(id, key) {
        let _ = db.execute("DELETE FROM provider WHERE id = ?1", [id]);
        return Err(e);
    }
    view(find(db, id)?, keys)
}

/// Deletes the provider, its key and the routes to it; returns the slots that are now Not set, then the label of each
/// feature's own model that used it ("Summary model"), which is now Not set too, never back on its slot.
pub fn remove(db: &mut Connection, keys: &dyn Keys, id: i64) -> Result<Vec<String>, String> {
    find(db, id)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let mut slots = {
        let mut q = tx.prepare("SELECT slot FROM route WHERE provider_id = ?1 ORDER BY slot DESC").map_err(|e| e.to_string())?; // main before fast
        q.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<Result<Vec<String>, _>>().map_err(|e| e.to_string())?
    };
    for (key, label, _) in OWN {
        if matches!(own(&tx, key)?, Own::Pick(p, _) if p == id) {
            crate::storage::write_text(&tx, key, "none").map_err(|e| e.to_string())?;
            slots.push(label.into());
        }
    }
    tx.execute("DELETE FROM route WHERE provider_id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM model_list WHERE provider_id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM provider WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    keys.delete(id)?;
    Ok(slots)
}

/// Saves the order AI providers lists them in; `ids` is every provider, first to last.
pub fn order(db: &mut Connection, ids: &[i64]) -> Result<(), String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let all: i64 = tx.query_row("SELECT COUNT(*) FROM provider", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if all != ids.len() as i64 {
        return Err("The providers changed. Open AI providers again".into());
    }
    for (i, id) in ids.iter().enumerate() {
        if tx.execute("UPDATE provider SET position = ?1 WHERE id = ?2", params![i as i64 + 1, id]).map_err(|e| e.to_string())? != 1 {
            return Err("The providers changed. Open AI providers again".into());
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

pub fn set(db: &Connection, slot: &str, provider: i64, model: &str) -> Result<(), String> {
    if !SLOTS.contains(&slot) {
        return Err("Unknown model slot".into());
    }
    if model.trim().is_empty() {
        return Err("Pick a model".into());
    }
    find(db, provider)?;
    db.execute("INSERT OR REPLACE INTO route(slot, provider_id, model) VALUES (?1, ?2, ?3)", params![slot, provider, model])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The provider and model a feature should use for `slot` ("main" or "fast"), or a "Not set" error.
pub fn route(db: &Connection, slot: &str) -> Result<(Record, String), String> {
    let label = if slot == "main" { "Main model" } else { "Fast model" };
    let not_set = || format!("{label}: Not set");
    let r = get_route(db, slot)?.ok_or_else(not_set)?;
    let p = find(db, r.provider).map_err(|_| not_set())?;
    if r.gone {
        return Err(format!("{label}: {} is no longer offered by {}", r.model, p.name));
    }
    Ok((p, r.model))
}

/// A feature's own model as saved (`OWN`): missing uses its slot's, "none" is Not set, else {"provider": id, "model": "…"}.
pub(crate) enum Own {
    Slot,
    NotSet,
    Pick(i64, String),
}

#[derive(Serialize, Deserialize)]
struct Pick {
    provider: i64,
    model: String,
}

pub(crate) fn own(db: &Connection, key: &str) -> Result<Own, String> {
    Ok(match crate::storage::read_text(db, key).map_err(|e| e.to_string())? {
        None => Own::Slot,
        Some(v) => serde_json::from_str::<Pick>(&v).map_or(Own::NotSet, |p| Own::Pick(p.provider, p.model)),
    })
}

fn own_of(key: &str) -> (&'static str, &'static str) {
    OWN.iter().find(|o| o.0 == key).map(|o| (o.1, o.2)).expect("a feature in OWN")
}

/// The provider and model feature `key` (in `OWN`) should use, or a "Not set" error, like `route`.
pub fn feature(db: &Connection, key: &str) -> Result<(Record, String), String> {
    let (label, slot) = own_of(key);
    let not_set = || format!("{label}: Not set");
    let (provider, model) = match own(db, key)? {
        Own::Slot => return route(db, slot),
        Own::NotSet => return Err(not_set()),
        Own::Pick(p, m) => (p, m),
    };
    let p = find(db, provider).map_err(|_| not_set())?;
    // Decision and transcription models aren't in the saved list, so it can't tell; a retired one fails with the service's reason.
    if !UNLISTED.contains(&key) && gone(db, provider, &model) {
        return Err(format!("{label}: {model} is no longer offered by {}", p.name));
    }
    Ok((p, model))
}

/// Feature `key`'s model for Settings: its slot's name ("main"), "none", or "<provider id>|<model>"; and whether
/// that model is gone from its provider's list.
pub fn feature_get(db: &Connection, key: &str) -> Result<(String, bool), String> {
    Ok(match own(db, key)? {
        Own::Slot => (own_of(key).1.into(), false),
        Own::NotSet => ("none".into(), false),
        Own::Pick(p, m) => (format!("{p}|{m}"), key != STT && gone(db, p, &m)),
    })
}

/// Saves feature `key`'s model, in `feature_get`'s form.
pub fn feature_set(db: &Connection, key: &str, value: &str) -> Result<(), String> {
    let saved = if value == own_of(key).1 {
        db.execute("DELETE FROM setting WHERE key = ?1", [key]).map(drop)
    } else if value == "none" {
        crate::storage::write_text(db, key, "none")
    } else {
        let (provider, model) = value.split_once('|').and_then(|(p, m)| Some((p.parse::<i64>().ok()?, m))).filter(|(_, m)| !m.trim().is_empty()).ok_or("Pick a model")?;
        find(db, provider)?;
        crate::storage::write_text(db, key, &serde_json::to_string(&Pick { provider, model: model.into() }).map_err(|e| e.to_string())?)
    };
    saved.map_err(|e| format!("Couldn't save the model: {e}"))
}

/// The adapter for provider `id`, holding its key; while the AI log is on, wrapped to record each call as `feature`'s.
pub fn provider(db: &Connection, keys: &dyn Keys, id: i64, feature: &str) -> Result<Box<dyn Backend>, String> {
    let r = find(db, id)?;
    let agent = crate::agent::is_agent(&r.kind);
    let b: Box<dyn Backend> = match crate::agent::Agent::new(&r.kind) {
        // No silent fallback: while Agents are off, a feature routed to one says so.
        Some(_) if !agents_on(db)? => {
            if let Some(dir) = log_dir(db) {
                crate::ai_log::blocked(&dir, feature, &r.name, true, AGENTS_OFF);
            }
            return Err(AGENTS_OFF.into());
        }
        Some(a) => Box::new(a),
        None => Box::new(adapter(db, keys, id)?),
    };
    Ok(logged(db, b, Some(&r.name), agent, feature))
}

/// `b` wrapped to record each call while the AI log is on, else `b` itself.
fn logged(db: &Connection, b: Box<dyn Backend>, provider: Option<&str>, agent: bool, feature: &str) -> Box<dyn Backend> {
    match log_dir(db) {
        Some(dir) => crate::ai_log::Logged::wrap(b, dir, provider, agent, feature),
        None => b,
    }
}

pub(crate) const AGENTS_OFF: &str = "Agents are off. Turn them on in Settings, Experimental";

/// The AI log is Experimental: on while both the master switch and its own are.
pub fn ai_log_on(db: &Connection) -> Result<bool, String> {
    let on = |k| crate::storage::read_text(db, k).map(|v| v.as_deref() == Some("on")).map_err(|e| e.to_string());
    Ok(on("experimental")? && on("exp_ailog")?)
}

/// The data folder (beside the database) while the AI log is on.
fn log_dir(db: &Connection) -> Option<std::path::PathBuf> {
    if !ai_log_on(db).unwrap_or(false) {
        return None;
    }
    std::path::Path::new(db.path()?).parent().map(|p| p.to_path_buf()).filter(|p| !p.as_os_str().is_empty())
}

/// Agents are Experimental: on while both the master switch and their own are.
pub fn agents_on(db: &Connection) -> Result<bool, String> {
    let on = |k| crate::storage::read_text(db, k).map(|v| v.as_deref() == Some("on")).map_err(|e| e.to_string());
    Ok(on("experimental")? && on("exp_agents")?)
}

/// Whether provider `id` is an Agent, so a feature can give it the time it needs.
pub fn is_agent(db: &Connection, id: i64) -> bool {
    find(db, id).is_ok_and(|r| crate::agent::is_agent(&r.kind))
}

fn adapter(db: &Connection, keys: &dyn Keys, id: i64) -> Result<OpenAi, String> {
    let r = find(db, id)?;
    if crate::agent::is_agent(&r.kind) {
        return Err(format!("{} is an Agent, which can't transcribe speech", r.name));
    }
    Ok(OpenAi { key: keys.get(id)?, rich: r.kind == "openrouter", url: r.url })
}

/// Model `model` as provider `id`'s last fetched list has it.
pub fn model(db: &Connection, id: i64, model: &str) -> Option<Model> {
    cached(db, id)?.into_iter().find(|m| m.id == model)
}

/// Model `model`'s context window from provider `id`'s last fetched list, when it says.
pub fn context(db: &Connection, id: i64, model: &str) -> Option<u64> {
    self::model(db, id, model)?.context
}

fn cached(db: &Connection, id: i64) -> Option<Vec<Model>> {
    let json: String = db.query_row("SELECT json FROM model_list WHERE provider_id = ?1", [id], |r| r.get(0)).optional().ok()??;
    serde_json::from_str(&json).ok()
}

pub(crate) fn store(db: &Connection, id: i64, list: &[Model]) -> Result<(), String> {
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    // The WHERE keeps a list from outliving a provider removed while it was being fetched.
    db.execute(
        "INSERT OR REPLACE INTO model_list(provider_id, json, fetched) SELECT id, ?2, ?3 FROM provider WHERE id = ?1",
        params![id, serde_json::to_string(list).map_err(|e| e.to_string())?, at],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The model list of provider `id`: the cached one unless `fresh`, else fetched and cached. A failed fetch leaves the cache alone.
pub fn models(db: &Mutex<Connection>, keys: &dyn Keys, id: i64, fresh: bool) -> Result<Vec<Model>, String> {
    if !fresh && let Some(l) = cached(&db.lock().unwrap(), id) {
        return Ok(l);
    }
    let p = provider(&db.lock().unwrap(), keys, id, "Models")?; // the lock is not held while waiting on the network
    let list = p.models()?;
    store(&db.lock().unwrap(), id, &list)?;
    Ok(list)
}

/// Checks the address and key without spending tokens: OpenRouter's key endpoint, else the model list (returned, to cache).
fn check(p: &dyn Backend) -> Result<(Check, Option<Vec<Model>>), String> {
    let start = Instant::now();
    p.check()?;
    let (mut list, mut used, mut limit) = (None, None, None);
    match p.key_usage()? {
        Some((u, l)) => (used, limit) = (Some(u), l),
        None => list = Some(p.models()?),
    }
    Ok((Check { ms: start.elapsed().as_millis() as u64, models: list.as_ref().map(Vec::len), used, limit }, list))
}

/// Picks one of `options` for `question` with the Decision model, else with the Fast model held to a JSON schema.
/// A failed call returns its reason and never falls back to the Fast model.
pub fn decide(db: &Mutex<Connection>, keys: &dyn Keys, question: &str, options: &[String], instructions: &str) -> Result<Decision, String> {
    if options.is_empty() {
        return Err("Give it at least one option".into());
    }
    let (p, model, decisions) = {
        let db = db.lock().unwrap(); // not held while waiting on the network
        let decisions = matches!(own(&db, DECIDE)?, Own::Pick(..));
        let (r, model) = feature(&db, DECIDE)?;
        (provider(&db, keys, r.id, "Decision")?, model, decisions)
    };
    let start = Instant::now();
    let mut d = p.decide(&model, question, options, instructions, decisions, 30)?;
    d.ms = start.elapsed().as_millis() as u64;
    Ok(d)
}

/// Saves the Decision model, in `feature_get`'s form ("fast" uses the Fast model).
pub fn decide_set(db: &Connection, value: &str) -> Result<(), String> {
    feature_set(db, DECIDE, value)
}

/// Tests saved provider `id`, caching its model list on the way.
pub fn test(db: &Mutex<Connection>, id: i64, p: &dyn Backend) -> Result<Check, String> {
    let (c, list) = check(p)?;
    if let Some(list) = list {
        store(&db.lock().unwrap(), id, &list)?;
    }
    Ok(c)
}

/// Tests a provider before it is added, with the address and key as typed (trimmed like `add`).
pub fn test_draft(db: &Mutex<Connection>, kind: &str, url: &str, key: &str) -> Result<Check, String> {
    let log = |b: Box<dyn Backend>| logged(&db.lock().unwrap(), b, None, crate::agent::is_agent(kind), "Test");
    if let Some(a) = crate::agent::Agent::new(kind) {
        return Ok(check(&*log(Box::new(a)))?.0);
    }
    let (url, key) = (url.trim().trim_end_matches('/'), key.trim());
    if host(url).is_none() {
        return Err("Enter a web address starting with http:// or https://".into());
    }
    let p = OpenAi { url: url.into(), key: (!key.is_empty()).then(|| key.into()), rich: kind == "openrouter" };
    Ok(check(&*log(Box::new(p)))?.0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[derive(Default)]
    pub(crate) struct Memory(Mutex<HashMap<i64, String>>);
    impl Keys for Memory {
        fn get(&self, id: i64) -> Result<Option<String>, String> { Ok(self.0.lock().unwrap().get(&id).cloned()) }
        fn set(&self, id: i64, key: &str) -> Result<(), String> { self.0.lock().unwrap().insert(id, key.into()); Ok(()) }
        fn delete(&self, id: i64) -> Result<(), String> { self.0.lock().unwrap().remove(&id); Ok(()) }
    }

    struct Fake;
    impl Backend for Fake {
        fn models(&self) -> Result<Vec<Model>, String> { Ok(vec![Model { id: "fake-1".into(), ..Default::default() }]) }
        fn key_usage(&self) -> Result<Option<(f64, Option<f64>)>, String> { Ok(None) }
        fn chat(&self, model: &str, _: Option<&str>, _: &str, _: u64) -> Result<Reply, String> {
            Ok(Reply { text: format!("hi from {model}"), tokens: Some((3, 4)), cost: Cost::Free, raw: None })
        }
    }

    pub(crate) fn db() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let c = crate::storage::open(tmp.path()).unwrap();
        (tmp, c)
    }

    pub(crate) fn add_ok(db: &Connection, keys: &Memory, kind: &str, name: &str, url: &str, key: &str) -> Provider {
        add(db, keys, kind, name, url, key).unwrap()
    }

    #[test]
    fn an_agent_is_added_and_used_only_while_agents_are_on() {
        let (_tmp, db) = db();
        let keys = Memory::default();
        assert_eq!(add(&db, &keys, "claude-code", "Claude Code", "", "").unwrap_err(), AGENTS_OFF);
        for (k, v) in [("experimental", "on"), ("exp_agents", "on")] {
            crate::storage::write_text(&db, k, v).unwrap();
        }
        let p = add(&db, &keys, "claude-code", "Claude Code", "http://ignored", "ignored").unwrap();
        assert_eq!((p.url.as_str(), p.has_key, p.off_device), ("", false, true), "no address or key, and always off-device");
        assert!(provider(&db, &keys, p.id, "Test").is_ok());
        assert!(is_agent(&db, p.id));
        assert!(super::adapter(&db, &keys, p.id).is_err(), "an Agent can't transcribe");
        crate::storage::write_text(&db, "exp_agents", "off").unwrap();
        assert_eq!(provider(&db, &keys, p.id, "Test").err().unwrap(), AGENTS_OFF, "no silent fallback while off");
    }

    #[test]
    fn providers_list_in_the_saved_order_and_a_new_one_goes_last() {
        let (_t, mut c) = db();
        let keys = Memory::default();
        let a = add_ok(&c, &keys, "openai", "A", "http://localhost:1/v1", "").id;
        let b = add_ok(&c, &keys, "openai", "B", "http://localhost:2/v1", "").id;
        let names = |c: &Connection| get(c, &keys).unwrap().providers.into_iter().map(|p| p.name).collect::<Vec<_>>();
        order(&mut c, &[b, a]).unwrap();
        add_ok(&c, &keys, "openai", "C", "http://localhost:3/v1", "");
        assert_eq!(names(&c), ["B", "A", "C"]);
        assert!(order(&mut c, &[a, b]).is_err(), "a stale list that misses C is refused");
        assert_eq!(names(&c), ["B", "A", "C"]);
    }

    #[test]
    fn add_list_remove_and_key_stays_out_of_the_db() {
        let (_t, mut c) = db();
        let keys = Memory::default();
        let a = add_ok(&c, &keys, "openai", " Ollama ", "http://localhost:11434/v1/", "");
        let b = add_ok(&c, &keys, "openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "sk-or-secret");
        assert_eq!((a.name.as_str(), a.url.as_str(), a.has_key, a.off_device), ("Ollama", "http://localhost:11434/v1", false, false));
        assert!(b.has_key && b.off_device);
        let s = get(&c, &keys).unwrap();
        assert_eq!(s.providers.iter().map(|p| p.id).collect::<Vec<_>>(), [a.id, b.id]);
        let dump: String = c.query_row("SELECT group_concat(kind || name || url, ' ') FROM provider", [], |r| r.get(0)).unwrap();
        assert!(!dump.contains("secret"));
        remove(&mut c, &keys, b.id).unwrap();
        assert!(keys.get(b.id).unwrap().is_none());
        assert_eq!(get(&c, &keys).unwrap().providers.len(), 1);
    }

    #[test]
    fn bad_input_is_refused_in_plain_words() {
        let (_t, c) = db();
        let keys = Memory::default();
        add_ok(&c, &keys, "openai", "Mine", "http://10.0.0.5:8080/v1", "");
        let err = |kind, name: &str, url, key| add(&c, &keys, kind, name, url, key).unwrap_err();
        assert!(err("openai", "mine", "http://x.test", "").contains("already a provider"));
        assert!(err("openai", "  ", "http://x.test", "").contains("name"));
        assert!(err("openai", &"x".repeat(41), "http://x.test", "").contains("40"));
        assert!(err("openai", "A", "ftp://x.test", "").contains("http"));
        assert!(err("openai", "A", "not a url", "").contains("http"));
        assert!(err("openrouter", "A", "https://openrouter.ai/api/v1", "sk-wrong").contains("sk-or-"));
        assert!(err("nope", "A", "http://x.test", "").contains("kind"));
        assert_eq!(get(&c, &keys).unwrap().providers.len(), 1);
    }

    #[test]
    fn off_device_is_anything_but_loopback() {
        for u in ["http://localhost:11434/v1", "http://127.0.0.1:1234", "http://127.5.5.5/v1", "http://[::1]:8000/v1"] {
            assert!(!off_device(u), "{u}");
        }
        for u in ["http://192.168.1.20:11434", "http://10.0.0.2", "https://openrouter.ai/api/v1", "http://my-mac.local:1234"] {
            assert!(off_device(u), "{u}");
        }
    }

    /// A fake feature: asks for a slot and sends its text there.
    fn tidy(db: &Connection) -> Result<(String, String), String> {
        let (p, model) = route(db, "fast")?;
        Ok((p.name, model))
    }

    #[test]
    fn a_feature_gets_the_routed_provider_until_it_is_removed() {
        let (_t, mut c) = db();
        let keys = Memory::default();
        assert_eq!(tidy(&c).unwrap_err(), "Fast model: Not set");
        let a = add_ok(&c, &keys, "openai", "Ollama", "http://localhost:11434/v1", "");
        let b = add_ok(&c, &keys, "openai", "Other", "http://10.0.0.5/v1", "");
        set(&c, "fast", a.id, "llama3").unwrap();
        set(&c, "main", a.id, "qwen").unwrap();
        assert_eq!(tidy(&c).unwrap(), ("Ollama".into(), "llama3".into()));
        set(&c, "fast", b.id, "m").unwrap();
        assert_eq!(tidy(&c).unwrap().0, "Other");
        set(&c, "fast", a.id, "llama3").unwrap();
        assert_eq!(remove(&mut c, &keys, a.id).unwrap(), ["main", "fast"]);
        assert_eq!(tidy(&c).unwrap_err(), "Fast model: Not set");
        let s = get(&c, &keys).unwrap();
        assert!(s.main.is_none() && s.fast.is_none());
        assert!(set(&c, "main", a.id, "x").is_err() && set(&c, "huge", b.id, "x").is_err());
        assert!(remove(&mut c, &keys, b.id).unwrap().is_empty());
    }

    #[test]
    fn a_model_missing_from_the_saved_list_is_gone_and_never_used() {
        let (_t, c) = db();
        let keys = Memory::default();
        let a = add_ok(&c, &keys, "openai", "Ollama", "http://localhost:11434/v1", "");
        set(&c, "fast", a.id, "llama3").unwrap();
        assert!(!get(&c, &keys).unwrap().fast.unwrap().gone, "no saved list: unknown, not gone");
        let m = |id: &str| Model { id: id.into(), ..Default::default() };
        store(&c, a.id, &[m("llama3"), m("qwen")]).unwrap();
        assert!(!get(&c, &keys).unwrap().fast.unwrap().gone);
        store(&c, a.id, &[m("qwen")]).unwrap();
        assert!(get(&c, &keys).unwrap().fast.unwrap().gone);
        assert_eq!(tidy(&c).unwrap_err(), "Fast model: llama3 is no longer offered by Ollama");
        set(&c, "fast", a.id, "qwen").unwrap();
        assert_eq!(tidy(&c).unwrap().1, "qwen");
    }

    #[test]
    fn cost_goes_reported_then_priced_then_free_then_unknown() {
        let usd = |c| matches!(c, Cost::Usd { .. });
        assert_eq!(cost_of(Some(0.5), Some((1.0, 1.0)), Some((1, 1)), true), Cost::Usd { usd: 0.5 });
        assert_eq!(cost_of(None, Some((3.0, 15.0)), Some((1_000_000, 2_000_000)), false), Cost::Usd { usd: 33.0 });
        assert!(usd(cost_of(None, Some((1.0, 1.0)), Some((1, 1)), true)));
        assert_eq!(cost_of(None, None, Some((1, 1)), true), Cost::Free);
        assert_eq!(cost_of(None, None, Some((1, 1)), false), Cost::Unknown);
    }

    /// One request off `s`: its head, lowercased, and its body.
    fn request(s: &mut std::net::TcpStream) -> (String, Vec<u8>) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            let n = s.read(&mut chunk).unwrap_or(0);
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") { break i + 4; }
            if n == 0 { break buf.len(); }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).to_lowercase();
        let want: usize = head.lines().find_map(|l| l.strip_prefix("content-length: ")?.trim().parse().ok()).unwrap_or(0);
        while buf.len() < head_end + want {
            let n = s.read(&mut chunk).unwrap_or(0);
            if n == 0 { break; }
            buf.extend_from_slice(&chunk[..n]);
        }
        (head, buf.split_off(head_end))
    }

    /// Serves each `(path, status, body)` and records nothing; unknown paths get 404.
    pub(crate) fn stub(routes: Vec<(&'static str, u16, &'static str)>) -> String {
        server(move |path, _, _| routes.iter().find(|r| r.0 == path).map_or((404, "{}".into()), |r| (r.1, r.2.into()))).0
    }

    /// Answers request `n` (from 0) to `path` with `reply(path, n, body)` and keeps every body it was sent.
    pub(crate) fn server(reply: impl Fn(&str, usize, &[u8]) -> (u16, String) + Send + 'static) -> (String, std::sync::Arc<Mutex<Vec<Vec<u8>>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let bodies = std::sync::Arc::new(Mutex::new(vec![]));
        let kept = bodies.clone();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let (head, body) = request(&mut s);
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let n = { let mut b = kept.lock().unwrap(); b.push(body.clone()); b.len() - 1 };
                let (status, out) = reply(&path, n, &body);
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len());
            }
        });
        (base, bodies)
    }

    fn adapter(url: String, rich: bool) -> OpenAi {
        OpenAi { url, key: Some("k".into()), rich }
    }

    #[test]
    fn a_streamed_reply_grows_as_it_arrives_and_a_whole_one_or_an_error_is_read_too() {
        const SSE: &str = ": keep-alive\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"# Re\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"cap\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"cost\":0.01}}\n\ndata: [DONE]\n\n";
        let (base, bodies) = server(|_, _, _| (200, SSE.into()));
        let mut seen = vec![];
        let r = adapter(format!("{base}/v1"), true).chat_stream("m", Some("sys"), "hi", 10, &mut |t| seen.push(t.to_string())).unwrap();
        assert_eq!(seen, ["# Re", "# Recap"]);
        assert_eq!((r.text.as_str(), r.tokens, r.cost), ("# Recap", Some((5, 2)), Cost::Usd { usd: 0.01 }));
        let sent: Value = serde_json::from_slice(&bodies.lock().unwrap()[0]).unwrap();
        assert_eq!(sent["stream"], true);

        let whole = stub(vec![("/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"All at once"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"cost":0.02}}"#)]);
        let mut seen = vec![];
        let r = adapter(format!("{whole}/v1"), true).chat_stream("m", None, "hi", 10, &mut |t| seen.push(t.to_string())).unwrap();
        assert_eq!((seen, r.text.as_str(), r.cost), (vec!["All at once".to_string()], "All at once", Cost::Usd { usd: 0.02 }));

        let (broken, _) = server(|_, _, _| (200, "data: {\"choices\":[{\"delta\":{\"content\":\"# Re\"}}]}\n\ndata: {\"error\":{\"message\":\"Provider died\"}}\n\n".into()));
        let e = adapter(format!("{broken}/v1"), true).chat_stream("m", None, "hi", 10, &mut |_| {}).unwrap_err();
        assert!(e.ends_with("Provider died"), "{e}");
        let refused = stub(vec![("/v1/chat/completions", 402, r#"{"error":{"message":"Insufficient credits"}}"#)]);
        let e = adapter(format!("{refused}/v1"), true).chat_stream("m", None, "hi", 10, &mut |_| {}).unwrap_err();
        assert!(e.ends_with("Insufficient credits"), "{e}");
    }

    #[test]
    fn plain_endpoint_lists_ids_and_chats_for_free() {
        let base = stub(vec![
            ("/v1/models", 200, r#"{"data":[{"id":"llama3","pricing":{"prompt":"1"}},{"id":"qwen"}]}"#),
            ("/v1/chat/completions", 200, r#"{"choices":[{"message":{"content":"Hello there my good friend"}}],"usage":{"prompt_tokens":5,"completion_tokens":6}}"#),
        ]);
        let p = adapter(format!("{base}/v1"), false);
        assert_eq!(p.models().unwrap(), vec![Model { id: "llama3".into(), ..Default::default() }, Model { id: "qwen".into(), ..Default::default() }]);
        let r = p.chat("llama3", None, "hi", 60).unwrap();
        assert_eq!((r.text.as_str(), r.tokens, r.cost), ("Hello there my good friend", Some((5, 6)), Cost::Free));
    }

    #[test]
    fn openrouter_list_maps_prices_context_images_and_reported_cost_wins() {
        let base = stub(vec![
            ("/models", 200, r#"{"data":[
                {"id":"a/vision","context_length":128000,"pricing":{"prompt":"0.000003","completion":"0.000015"},"architecture":{"input_modalities":["text","image"]}},
                {"id":"b/text","context_length":8000,"pricing":{"prompt":"0","completion":"0"},"architecture":{"input_modalities":["text"]}},
                {"id":"openrouter/auto","pricing":{"prompt":"-1","completion":"-1"}}]}"#),
            ("/chat/completions", 200, r#"{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":1000,"completion_tokens":2000,"cost":0.0042}}"#),
        ]);
        let p = adapter(base.clone(), true);
        let m = p.models().unwrap();
        assert_eq!((m[0].input_per_m.map(|x| x.round()), m[0].output_per_m.map(|x| x.round()), m[0].context, m[0].images), (Some(3.0), Some(15.0), Some(128000), Some(true)));
        assert_eq!((m[1].input_per_m, m[1].images), (Some(0.0), Some(false)));
        assert_eq!((m[2].input_per_m, m[2].context, m[2].images), (None, None, None));
        assert_eq!(p.chat("a/vision", None, "hi", 60).unwrap().cost, Cost::Usd { usd: 0.0042 });
    }

    #[test]
    fn openrouter_without_reported_cost_is_priced_from_the_list() {
        let base = stub(vec![
            ("/models", 200, r#"{"data":[{"id":"a/m","pricing":{"prompt":"0.000001","completion":"0.000002"}}]}"#),
            ("/chat/completions", 200, r#"{"choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":1000000,"completion_tokens":500000}}"#),
        ]);
        let Cost::Usd { usd } = adapter(base, true).chat("a/m", None, "hi", 60).unwrap().cost else { panic!("not priced") };
        assert!((usd - 2.0).abs() < 1e-9, "{usd}");
    }

    #[test]
    fn errors_read_as_plain_sentences() {
        let base = stub(vec![("/models", 401, r#"{"error":{"message":"Invalid API key"}}"#), ("/key", 403, "{}"), ("/chat/completions", 500, "oops")]);
        let p = adapter(base.clone(), false);
        let e = p.models().unwrap_err();
        assert!(e.ends_with("refused the key (HTTP 401)"), "{e}");
        assert!(adapter(base.clone(), true).key_usage().unwrap_err().ends_with("refused the key (HTTP 403)"));
        assert!(p.chat("m", None, "hi", 60).unwrap_err().ends_with("HTTP 500"));
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap(); // closed again at once
        assert!(adapter(format!("http://{dead}"), false).models().unwrap_err().starts_with("Couldn't reach"));
    }

    fn db_with(url: String, kind: &str) -> (tempfile::TempDir, Mutex<Connection>, Memory, i64) {
        let (tmp, c) = db();
        let keys = Memory::default();
        let id = add_ok(&c, &keys, kind, "P", &url, if kind == "openrouter" { "sk-or-x" } else { "" }).id;
        (tmp, Mutex::new(c), keys, id)
    }

    const LIST: &str = r#"{"data":[{"id":"a"},{"id":"b"}]}"#;

    #[test]
    fn models_are_cached_until_a_fresh_fetch_replaces_them() {
        let base = stub(vec![("/v1/models", 200, LIST)]);
        let (_t, db, keys, id) = db_with(format!("{base}/v1"), "openai");
        let ids = |l: Vec<Model>| l.into_iter().map(|m| m.id).collect::<Vec<_>>();
        assert!(cached(&db.lock().unwrap(), id).is_none());
        assert_eq!(ids(models(&db, &keys, id, false).unwrap()), ["a", "b"]); // miss: fetched and stored
        db.lock().unwrap().execute("UPDATE model_list SET json = '[{\"id\":\"old\"}]'", []).unwrap();
        assert_eq!(ids(models(&db, &keys, id, false).unwrap()), ["old"]); // hit: no fetch
        assert_eq!(ids(models(&db, &keys, id, true).unwrap()), ["a", "b"]); // fresh: fetched and replaced
        assert_eq!(ids(cached(&db.lock().unwrap(), id).unwrap()), ["a", "b"]);
    }

    #[test]
    fn a_failed_fresh_fetch_keeps_the_cache() {
        let (_t, db, keys, id) = db_with(format!("{}/v1", stub(vec![])), "openai"); // every path 404s
        store(&db.lock().unwrap(), id, &[Model { id: "kept".into(), ..Default::default() }]).unwrap();
        assert!(models(&db, &keys, id, true).unwrap_err().starts_with("Couldn't reach"));
        assert_eq!(models(&db, &keys, id, false).unwrap()[0].id, "kept");
    }

    #[test]
    fn remove_deletes_the_cache() {
        let base = stub(vec![("/models", 200, LIST)]);
        let (_t, db, keys, id) = db_with(base, "openai");
        models(&db, &keys, id, false).unwrap();
        remove(&mut db.lock().unwrap(), &keys, id).unwrap();
        let n: i64 = db.lock().unwrap().query_row("SELECT count(*) FROM model_list", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(store(&db.lock().unwrap(), id, &[]).is_ok() && cached(&db.lock().unwrap(), id).is_none()); // a late fetch can't resurrect it
    }

    #[test]
    fn test_counts_models_and_caches_them() {
        let base = stub(vec![("/models", 200, LIST)]);
        let (_t, db, _k, id) = db_with(base.clone(), "openai");
        let c = test(&db, id, &adapter(base, false)).unwrap();
        assert_eq!((c.models, c.used, c.limit), (Some(2), None, None));
        assert_eq!(cached(&db.lock().unwrap(), id).unwrap().len(), 2);
    }

    #[test]
    fn test_reads_the_openrouter_key_usage_and_limit() {
        let base = stub(vec![("/key", 200, r#"{"data":{"usage":0.12,"limit":5,"limit_remaining":4.88}}"#)]);
        let (_t, db, _k, id) = db_with(base.clone(), "openrouter");
        let c = test(&db, id, &adapter(base.clone(), true)).unwrap();
        assert_eq!((c.models, c.used, c.limit), (None, Some(0.12), Some(5.0)));
        let base = stub(vec![("/key", 200, r#"{"data":{"usage":0,"limit":null}}"#)]);
        let c = test(&db, id, &adapter(base, true)).unwrap();
        assert_eq!((c.used, c.limit), (Some(0.0), None));
        assert!(cached(&db.lock().unwrap(), id).is_none()); // OpenRouter's Test doesn't fetch the list
    }

    #[test]
    fn test_failures_read_plainly() {
        let base = stub(vec![("/key", 401, "{}")]);
        let (_t, db, _k, id) = db_with(base.clone(), "openrouter");
        assert!(test(&db, id, &adapter(base, true)).unwrap_err().contains("refused the key"));
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert!(test(&db, id, &adapter(format!("http://{dead}"), false)).unwrap_err().starts_with("Couldn't reach"));
    }

    #[test]
    fn a_draft_is_tested_as_typed_and_saves_nothing() {
        let base = stub(vec![("/models", 200, LIST)]);
        let (_t, db) = db();
        let db = Mutex::new(db);
        let c = test_draft(&db, "openai", &format!(" {base}/ "), "").unwrap();
        assert_eq!((c.models, c.used), (Some(2), None));
        let base = stub(vec![("/key", 200, r#"{"data":{"usage":0.5,"limit":null}}"#)]);
        assert_eq!(test_draft(&db, "openrouter", &base, "sk-or-x").unwrap().used, Some(0.5));
        assert!(test_draft(&db, "openai", "not a url", "").is_err());
    }

    const OPTS: [&str; 5] = ["Inbox", "Acme", "Platform team", "1:1s", "None of these"];
    const OPTS_Q: &str = "Which folder should “Acme pricing follow-up” go in?";
    fn ask(db: &Mutex<Connection>, keys: &Memory) -> Result<Decision, String> {
        decide(db, keys, OPTS_Q, &OPTS.map(String::from), "Pick the folder this meeting belongs in.")
    }
    const PICKED: &str = r#"{"id":"x","model":"typesafe/jev-1.14-20261014","answers":{"pick":{"type":"choice","choice":"Acme","confidence":0.98,
        "probabilities":{"Acme":0.98,"Inbox":0.02}}},"usage":{"input_tokens":420,"output_tokens":0,"cost":0.000018}}"#;
    const FAST: &str = r#"{"choices":[{"message":{"content":"{\"choice\":\"Acme\"}"}}],"usage":{"prompt_tokens":80,"completion_tokens":6}}"#;

    /// An OpenRouter Endpoint (id 1) serving `or`, and a local server on Fast (id 2) that would answer "Acme".
    fn decide_db(or: Vec<(&'static str, u16, &'static str)>) -> (tempfile::TempDir, Mutex<Connection>, Memory) {
        let (tmp, c) = db();
        let keys = Memory::default();
        add_ok(&c, &keys, "openrouter", "OpenRouter", &format!("{}/api/v1", stub(or)), "sk-or-x");
        add_ok(&c, &keys, "openai", "Local", &format!("{}/v1", stub(vec![("/v1/chat/completions", 200, FAST)])), "");
        set(&c, "fast", 2, "small").unwrap();
        (tmp, Mutex::new(c), keys)
    }

    #[test]
    fn the_decision_model_picks_with_confidence_and_cost() {
        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 200, PICKED)]);
        decide_set(&db.lock().unwrap(), "1|typesafe/jev-1.14").unwrap();
        assert_eq!(get(&db.lock().unwrap(), &keys).unwrap().decide, "1|typesafe/jev-1.14");
        let d = ask(&db, &keys).unwrap();
        assert_eq!((d.choice.as_str(), d.confidence, d.cost), ("Acme", Some(0.98), Cost::Usd { usd: 0.000018 }));
    }

    #[test]
    fn with_none_set_the_fast_model_decides_through_a_json_schema() {
        let (_t, db, keys) = decide_db(vec![]);
        assert_eq!(get(&db.lock().unwrap(), &keys).unwrap().decide, "fast");
        let d = ask(&db, &keys).unwrap();
        assert_eq!((d.choice.as_str(), d.confidence, d.cost), ("Acme", None, Cost::Free));
        db.lock().unwrap().execute("DELETE FROM route", []).unwrap();
        assert_eq!(ask(&db, &keys).unwrap_err(), "Fast model: Not set");
    }

    #[test]
    fn a_failed_decision_returns_the_reason_and_never_falls_back() {
        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 402, r#"{"error":{"code":402,"message":"Insufficient credits"}}"#)]);
        decide_set(&db.lock().unwrap(), "1|typesafe/jev-1.14").unwrap();
        let e = ask(&db, &keys).unwrap_err();
        assert!(e.ends_with("Insufficient credits"), "{e}"); // Fast would have answered
        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 200, r#"{"answers":{"pick":{"type":"choice","choice":"Mars"}}}"#)]);
        decide_set(&db.lock().unwrap(), "1|m").unwrap();
        assert_eq!(ask(&db, &keys).unwrap_err(), "m picked “Mars”, which isn't one of the options");
        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 200, r#"{"answers":{"pick":{"type":"refusal"}}}"#)]);
        decide_set(&db.lock().unwrap(), "1|m").unwrap();
        assert_eq!(ask(&db, &keys).unwrap_err(), "m refused to answer");
    }

    #[test]
    fn removing_its_provider_makes_the_decision_model_not_set() {
        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 200, PICKED)]);
        decide_set(&db.lock().unwrap(), "1|typesafe/jev-1.14").unwrap();
        store(&db.lock().unwrap(), 1, &[Model { id: "a/chat".into(), ..Default::default() }]).unwrap();
        assert_eq!(ask(&db, &keys).unwrap().choice, "Acme", "a decision model isn't gone for missing from the chat list");
        assert_eq!(remove(&mut db.lock().unwrap(), &keys, 1).unwrap(), ["Decision model"]);
        assert_eq!(get(&db.lock().unwrap(), &keys).unwrap().decide, "none");
        assert_eq!(ask(&db, &keys).unwrap_err(), "Decision model: Not set", "never back on the Fast model");
    }

    #[test]
    fn openrouter_lists_its_decision_models_and_plain_endpoints_have_none() {
        let base = stub(vec![("/models?output_modalities=decisions", 200, r#"{"data":[{"id":"typesafe/jev-1.14","context_length":131072,"pricing":{"prompt":"0.000000042","completion":"0"}}]}"#)]);
        let l = adapter(base.clone(), true).decision_models().unwrap();
        assert_eq!((l[0].id.as_str(), l[0].output_per_m), ("typesafe/jev-1.14", Some(0.0)));
        assert!(adapter(base, false).decision_models().unwrap().is_empty());
    }

    const HEARD: &str = r#"{"text":"Hello from Myna.","segments":[{"id":0,"start":0.0,"end":1.4,"text":" Hello from Myna."},{"id":1,"start":1.5,"end":4.6,"text":" This short clip."}],"usage":{"seconds":5,"cost":0.0002}}"#;

    #[test]
    fn transcription_goes_as_multipart_with_timestamps_asked_for() {
        let (base, bodies) = server(|_, _, _| (200, HEARD.into()));
        let (segs, cost) = adapter(base, true).transcribe("openai/whisper-large-v3", b"RIFFfake", 30).unwrap();
        assert_eq!((segs.len(), segs[1].start, segs[1].text.as_str(), cost), (2, 1.5, " This short clip.", Cost::Usd { usd: 0.0002 }));
        let body = String::from_utf8_lossy(&bodies.lock().unwrap()[0]).into_owned();
        for part in ["name=\"model\"\r\n\r\nopenai/whisper-large-v3", "name=\"response_format\"\r\n\r\nverbose_json", "name=\"timestamp_granularities[]\"\r\n\r\nsegment", "filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFfake"] {
            assert!(body.contains(part), "{part} missing from {body}");
        }
    }

    #[test]
    fn the_stt_test_picks_only_a_model_whose_reply_has_segment_timestamps() {
        let (_t, c) = db();
        let keys = Memory::default();
        let replies = |text: &'static str| server(move |_, _, _| (200, text.into())).0;
        let untimed = add_ok(&c, &keys, "openai", "Untimed", &replies(r#"{"text":"Hello from Myna.","usage":{"cost":0.0001}}"#), "").id;
        let refuses = add_ok(&c, &keys, "openai", "Refuses", &stub(vec![("/audio/transcriptions", 400, r#"{"error":{"message":"response_format 'verbose_json' is not compatible with this model"}}"#)]), "").id;
        let good = add_ok(&c, &keys, "openai", "Good", &replies(HEARD), "").id;
        let db = Mutex::new(c);
        assert_eq!(crate::stt::saved(&db.lock().unwrap()).unwrap(), crate::stt::Pick::Local { model: None }, "the Local model until one passes");
        assert_eq!(stt_test(&db, &keys, untimed, "whisper-1").unwrap_err(), "whisper-1 can't transcribe meetings: it sent no segment timestamps");
        assert!(stt_test(&db, &keys, refuses, "openai/gpt-4o-transcribe").unwrap_err().ends_with("verbose_json' is not compatible with this model"));
        assert!(stt_test(&db, &keys, good, "  ").is_err());
        assert_eq!(crate::stt::saved(&db.lock().unwrap()).unwrap(), crate::stt::Pick::Local { model: None }, "a failed Test picks nothing");
        let c = stt_test(&db, &keys, good, "whisper-1").unwrap();
        assert_eq!((c.segments, c.cost), (2, Cost::Usd { usd: 0.0002 }));
        assert_eq!(crate::stt::saved(&db.lock().unwrap()).unwrap(), crate::stt::Pick::Endpoint { provider: good, model: "whisper-1".into() });
        let e = stt_endpoint(&db.lock().unwrap(), &keys).unwrap().unwrap();
        assert_eq!((e.provider.as_str(), e.model.as_str(), e.off), ("Good", "whisper-1", false));
        crate::stt::use_local(&db.lock().unwrap()).unwrap();
        assert!(stt_endpoint(&db.lock().unwrap(), &keys).unwrap().is_none());
    }

    #[test]
    fn local_is_the_default_whatever_providers_and_slots_are_set() {
        let (_t, c) = db();
        let keys = Memory::default();
        let a = add_ok(&c, &keys, "openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "sk-or-x");
        set(&c, "main", a.id, "m").unwrap();
        set(&c, "fast", a.id, "m").unwrap();
        assert!(stt_endpoint(&c, &keys).unwrap().is_none());
    }

    #[test]
    fn removing_its_provider_makes_meeting_stt_not_set_never_local() {
        let (_t, mut c) = db();
        let keys = Memory::default();
        let a = add_ok(&c, &keys, "openrouter", "OpenRouter", "https://openrouter.ai/api/v1", "sk-or-x");
        feature_set(&c, STT, &format!("{}|openai/whisper-large-v3", a.id)).unwrap();
        store(&c, a.id, &[Model { id: "a/chat".into(), ..Default::default() }]).unwrap();
        assert!(stt_endpoint(&c, &keys).unwrap().is_some(), "a transcription model isn't gone for missing from the chat list");
        assert_eq!(remove(&mut c, &keys, a.id).unwrap(), ["Meeting speech-to-text"]);
        assert_eq!(crate::stt::saved(&c).unwrap(), crate::stt::Pick::NotSet);
        assert_eq!(stt_endpoint(&c, &keys).err().unwrap(), "Meeting speech-to-text: Not set");
    }

    #[test]
    fn openrouter_lists_its_transcription_models_and_others_type_one_in() {
        let (_t, c) = db();
        let keys = Memory::default();
        let base = stub(vec![("/models?output_modalities=transcription", 200, r#"{"data":[{"id":"openai/whisper-large-v3"}]}"#)]);
        let or = add_ok(&c, &keys, "openrouter", "OR", &base, "sk-or-x").id;
        let plain = add_ok(&c, &keys, "openai", "Plain", &base, "").id;
        let c = Mutex::new(c);
        assert_eq!(stt_models(&c, &keys, or).unwrap()[0].id, "openai/whisper-large-v3");
        assert!(stt_models(&c, &keys, plain).unwrap().is_empty());
    }

    // ---- AI log ----

    fn log_on(db: &Connection, on: bool) {
        for (k, v) in [("experimental", "on"), ("exp_ailog", if on { "on" } else { "off" })] {
            crate::storage::write_text(db, k, v).unwrap();
        }
    }

    const SSE: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"# Re\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"cap\"}}]}\n\ndata: {\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2,\"cost\":0.01}}\n\ndata: [DONE]\n\n";
    const WHOLE: &str = r#"{"choices":[{"message":{"content":"Part notes"}}],"usage":{"prompt_tokens":7,"completion_tokens":3,"cost":0.02}}"#;

    /// A server that streams when asked to and answers whole otherwise.
    fn streams() -> String {
        server(|_, _, body| (200, if String::from_utf8_lossy(body).contains("\"stream\":true") { SSE.into() } else { WHOLE.into() })).0
    }

    #[test]
    fn the_ai_log_keeps_each_call_with_its_feature_step_prompt_and_raw_reply() {
        let (tmp, db, keys, id) = db_with(format!("{}/v1", streams()), "openrouter");
        log_on(&db.lock().unwrap(), true);
        let b = provider(&db.lock().unwrap(), &keys, id, "Summary").unwrap();
        b.step(Some("part 1/2"));
        assert_eq!(b.chat("m", Some("Take notes"), "<transcript>hi</transcript>", 10).unwrap().text, "Part notes");
        b.step(None);
        let mut seen = vec![];
        assert_eq!(b.chat_stream("m", Some("Merge"), "<notes>n</notes>", 10, &mut |t| seen.push(t.to_string())).unwrap().text, "# Recap");
        assert_eq!(seen, ["# Re", "# Recap"], "the feature still gets the text as it grows");

        let l = crate::ai_log::list(tmp.path(), None, false);
        assert_eq!(l.len(), 2);
        let (s, p) = (crate::ai_log::get(tmp.path(), &l[0].id).unwrap(), crate::ai_log::get(tmp.path(), &l[1].id).unwrap());
        assert_eq!((p.feature.as_str(), p.step.as_deref(), p.provider.as_deref(), p.kind.as_deref(), p.model.as_deref(), p.method.as_deref()), ("Summary", Some("part 1/2"), Some("P"), Some("endpoint"), Some("m"), Some("chat")));
        assert_eq!((p.system.as_deref(), p.prompt.as_deref(), p.text.as_deref(), p.raw.as_deref()), (Some("Take notes"), Some("<transcript>hi</transcript>"), Some("Part notes"), Some(WHOLE)));
        assert_eq!((p.outcome.as_deref(), p.ttft_ms, p.limit_s, p.tokens_in, p.tokens_out, p.cost.clone()), (Some("ok"), None, Some(10), Some(7), Some(3), Some(Cost::Usd { usd: 0.02 })));
        assert!(p.total_ms.is_some());
        assert_eq!((s.step, s.method.as_deref(), s.text.as_deref(), s.cost), (None, Some("chat_stream"), Some("# Recap"), Some(Cost::Usd { usd: 0.01 })));
        assert!(s.ttft_ms.is_some() && s.raw.unwrap().contains("data: [DONE]"));
        assert_eq!((l[0].preview.as_deref(), l[0].outcome.as_str()), (Some("# Recap"), "ok"));
    }

    #[test]
    fn nothing_is_logged_while_the_ai_log_or_experimental_is_off() {
        let (tmp, db, keys, id) = db_with(format!("{}/v1", streams()), "openai");
        let call = || provider(&db.lock().unwrap(), &keys, id, "LLM cleanup").unwrap().chat("m", None, "hi", 10).unwrap();
        call();
        log_on(&db.lock().unwrap(), false);
        call();
        log_on(&db.lock().unwrap(), true);
        crate::storage::write_text(&db.lock().unwrap(), "experimental", "off").unwrap();
        call();
        assert!(crate::ai_log::list(tmp.path(), None, false).is_empty());
    }

    #[test]
    fn failures_are_logged_as_error_timeout_refused_or_blocked() {
        let (base, _) = server(|path, _, _| match path {
            "/v1/chat/completions" => (500, "oops".into()),
            _ => {
                std::thread::sleep(Duration::from_millis(2500));
                (200, "{}".into())
            }
        });
        let (tmp, db, keys, id) = db_with(format!("{base}/v1"), "openai");
        log_on(&db.lock().unwrap(), true);
        let b = provider(&db.lock().unwrap(), &keys, id, "Title").unwrap();
        assert!(b.chat("m", None, "hi", 10).is_err());
        let slow = add_ok(&db.lock().unwrap(), &keys, "openai", "Slow", &format!("{base}/slow"), "").id;
        assert!(provider(&db.lock().unwrap(), &keys, slow, "Title").unwrap().chat("m", None, "hi", 1).is_err());
        let l = crate::ai_log::list(tmp.path(), None, true);
        assert_eq!(l.iter().map(|s| s.outcome.as_str()).collect::<Vec<_>>(), ["timeout", "error"]);
        let e = crate::ai_log::get(tmp.path(), &l[1].id).unwrap();
        assert!(e.error.unwrap().ends_with("HTTP 500"));
        assert_eq!(e.raw.as_deref(), Some("oops"), "the body that failed to parse is kept");

        let (_t, db, keys) = decide_db(vec![("/api/alpha/decisions", 200, r#"{"answers":{"pick":{"type":"choice","choice":"Mars"}}}"#)]);
        log_on(&db.lock().unwrap(), true);
        decide_set(&db.lock().unwrap(), "1|m").unwrap();
        assert!(ask(&db, &keys).is_err());
        crate::storage::write_text(&db.lock().unwrap(), "exp_agents", "on").unwrap();
        let agent = add_ok(&db.lock().unwrap(), &keys, "claude-code", "Claude Code", "", "").id;
        crate::storage::write_text(&db.lock().unwrap(), "exp_agents", "off").unwrap();
        assert_eq!(provider(&db.lock().unwrap(), &keys, agent, "LLM cleanup").err().unwrap(), AGENTS_OFF);
        let dir = std::path::PathBuf::from(db.lock().unwrap().path().unwrap()).parent().unwrap().to_path_buf();
        let l = crate::ai_log::list(&dir, None, false);
        assert_eq!(l.iter().map(|s| (s.feature.as_str(), s.outcome.as_str())).collect::<Vec<_>>(), [("LLM cleanup", "blocked"), ("Decision", "refused")]);
        let d = crate::ai_log::get(&dir, &l[1].id).unwrap();
        let dd = d.decision.unwrap();
        assert_eq!((dd.question.as_str(), dd.options.len(), dd.via.as_str(), dd.choice), (OPTS_Q, OPTS.len(), "decisions", None));
        assert_eq!(crate::ai_log::get(&dir, &l[0].id).unwrap().error.as_deref(), Some(AGENTS_OFF));
    }

    #[test]
    fn a_huge_reply_is_cut_images_become_placeholders_and_no_key_is_kept() {
        let big = format!(r#"{{"choices":[{{"message":{{"content":"ok"}}}}],"pad":"{}"}}"#, "x".repeat(300 * 1024));
        let (base, _) = server(move |_, _, _| (200, big.clone()));
        let (tmp, db, keys, id) = db_with(format!("{base}/v1"), "openrouter");
        keys.set(id, "sk-or-secret-123").unwrap();
        log_on(&db.lock().unwrap(), true);
        let png = "A".repeat(4096);
        let b = provider(&db.lock().unwrap(), &keys, id, "LLM cleanup").unwrap();
        b.chat("m", None, &format!("look: data:image/png;base64,{png} done"), 10).unwrap();
        let e = crate::ai_log::get(tmp.path(), &crate::ai_log::list(tmp.path(), None, false)[0].id).unwrap();
        assert_eq!(e.prompt.as_deref(), Some("look: [image, 3 KB] done"));
        let raw = e.raw.unwrap();
        assert!(raw.len() <= 256 * 1024 + 20 && raw.ends_with("[cut at 256 KB]") && e.raw_cut == Some(true));
        let file = std::fs::read_to_string(tmp.path().join("ai-log.jsonl")).unwrap();
        assert!(!file.contains("sk-or-secret") && !file.to_lowercase().contains("authorization"));
    }

    #[test]
    fn a_log_that_cant_be_written_doesnt_fail_the_call() {
        let (tmp, db, keys, id) = db_with(format!("{}/v1", streams()), "openai");
        std::fs::create_dir(tmp.path().join("ai-log.jsonl")).unwrap(); // a folder where the file goes
        log_on(&db.lock().unwrap(), true);
        assert_eq!(provider(&db.lock().unwrap(), &keys, id, "Summary").unwrap().chat("m", None, "hi", 10).unwrap().text, "Part notes");
    }

    #[test]
    fn llm_cleanup_keeps_its_time_budget_while_logging() {
        let (base, _) = server(|_, _, _| {
            std::thread::sleep(Duration::from_millis(1500));
            (200, WHOLE.into())
        });
        let (tmp, db, keys, id) = db_with(format!("{base}/v1"), "openai");
        log_on(&db.lock().unwrap(), true);
        let b = provider(&db.lock().unwrap(), &keys, id, "LLM cleanup").unwrap();
        let start = Instant::now();
        let o = crate::cleanup::run(b, "m", crate::cleanup::DEFAULT, "hi there", &Default::default(), Duration::from_millis(500));
        assert!(start.elapsed() < Duration::from_millis(900), "{:?}", start.elapsed());
        assert_eq!(o.fallback.unwrap().reason, "AI took too long");
        std::thread::sleep(Duration::from_millis(1500));
        assert_eq!(crate::ai_log::list(tmp.path(), Some("LLM cleanup"), false).len(), 1, "the late reply is still logged");
    }

    #[test]
    fn meeting_speech_to_text_is_logged_with_segments_and_clip_length_but_no_audio() {
        let (base, _) = server(|_, _, _| (200, HEARD.into()));
        let (tmp, c) = db();
        let keys = Memory::default();
        let id = add_ok(&c, &keys, "openai", "Cloud", &base, "").id;
        feature_set(&c, STT, &format!("{id}|whisper-1")).unwrap();
        log_on(&c, true);
        let e = stt_endpoint(&c, &keys).unwrap().unwrap();
        e.send(CLIP).unwrap();
        let l = crate::ai_log::list(tmp.path(), None, false);
        let x = crate::ai_log::get(tmp.path(), &l[0].id).unwrap();
        assert_eq!((x.feature.as_str(), x.method.as_deref(), x.provider.as_deref(), x.model.as_deref(), x.cost), ("Meeting speech-to-text", Some("transcribe"), Some("Cloud"), Some("whisper-1"), Some(Cost::Usd { usd: 0.0002 })));
        assert!(x.segments.unwrap().contains("This short clip."));
        assert!((4000..7000).contains(&x.audio_ms.unwrap()), "{:?}", x.audio_ms);
        let file = std::fs::read(tmp.path().join("ai-log.jsonl")).unwrap();
        assert!(!file.windows(4).any(|w| w == b"RIFF"), "no audio");
    }

    #[test]
    fn listing_decision_models_leaves_the_database_free_while_it_waits() {
        let (_t, c) = db();
        let keys = Memory::default();
        let db = std::sync::Arc::new(Mutex::new(c));
        let free = db.clone();
        let base = server(move |_, _, _| match free.try_lock() {
            Ok(_) => (200, r#"{"data":[{"id":"typesafe/jev-1.14"}]}"#.into()),
            Err(_) => (500, "the database was held".into()),
        })
        .0;
        let or = add_ok(&db.lock().unwrap(), &keys, "openrouter", "OR", &base, "sk-or-x").id;
        assert_eq!(decision_models(&db, &keys, or).unwrap()[0].id, "typesafe/jev-1.14");
    }
}
