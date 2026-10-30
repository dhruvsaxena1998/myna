//! The one AI provider: settings, API key in the Keychain, and a chat-completions client that
//! OpenAI, OpenRouter, Anthropic's compatibility endpoint and local servers such as Ollama all speak.
use crate::{settings, Db};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};
use url::{Host, Url};

#[derive(Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    /// Placeholder for the model field only; the user types the model.
    pub model: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "openrouter", name: "OpenRouter", base_url: "https://openrouter.ai/api/v1", model: "openai/gpt-5-mini" },
    Preset { id: "openai", name: "OpenAI", base_url: "https://api.openai.com/v1", model: "gpt-5-mini" },
    Preset { id: "anthropic", name: "Anthropic", base_url: "https://api.anthropic.com/v1", model: "claude-sonnet-5-5" },
    Preset { id: "custom", name: "Custom (OpenAI-compatible)", base_url: "http://localhost:11434/v1", model: "llama3.1" },
];

#[derive(Clone)]
pub struct Config {
    pub preset: &'static Preset,
    pub base_url: String,
    pub model: String,
    /// None for a local server that needs no key.
    pub key: Option<String>,
    /// Per 1M tokens, entered by the user for this model; None when left blank.
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub context: Option<u64>,
}

pub struct Message {
    pub role: &'static str,
    pub content: String,
}

/// A screenshot to attach to the last message, with the words that say when it was taken.
pub struct Image {
    pub caption: String,
    pub jpeg: Vec<u8>,
}

#[derive(Debug)]
pub struct Reply {
    pub text: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

// The key lives in the macOS Keychain, one item per preset so switching presets never sends one
// provider's key to another provider's host.
#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
    const SERVICE: &str = "com.dhruvsaxena.myna.ai";
    const NOT_FOUND: i32 = -25300; // errSecItemNotFound

    pub fn get(account: &str) -> Option<String> {
        String::from_utf8(get_generic_password(SERVICE, account).ok()?).ok().filter(|k| !k.is_empty())
    }
    pub fn set(account: &str, key: &str) -> Result<(), String> {
        set_generic_password(SERVICE, account, key.as_bytes()).map_err(|e| format!("Keychain refused the key: {e}"))
    }
    pub fn remove(account: &str) -> Result<(), String> {
        match delete_generic_password(SERVICE, account) {
            Err(e) if e.code() != NOT_FOUND => Err(format!("Keychain refused: {e}")),
            _ => Ok(()),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod keychain {
    pub fn get(_: &str) -> Option<String> { None }
    pub fn set(_: &str, _: &str) -> Result<(), String> { Err("The Keychain is only on macOS".into()) }
    pub fn remove(_: &str) -> Result<(), String> { Ok(()) }
}

fn preset(id: Option<&str>) -> &'static Preset {
    PRESETS.iter().find(|p| Some(p.id) == id).unwrap_or(&PRESETS[0])
}

/// Price and context window are per model: they live in settings as `ai_in:<model>` and so on.
/// The Settings page writes the same keys.
fn per_model(db: &Connection, kind: &str, model: &str) -> Option<String> {
    settings::get(db, &format!("ai_{kind}:{model}")).map(|v| v.trim().trim_start_matches('$').to_string())
}

impl Config {
    /// Everything but the key, which only `load` fetches from the Keychain.
    pub fn from_settings(db: &Connection) -> Config {
        let preset = preset(settings::get(db, "ai_preset").as_deref());
        let model = settings::get(db, "ai_model").unwrap_or_default().trim().to_string();
        Config {
            preset,
            base_url: settings::get(db, "ai_base_url").filter(|u| !u.trim().is_empty()).unwrap_or(preset.base_url.into()),
            input_price: per_model(db, "in", &model).and_then(|v| v.parse().ok()),
            output_price: per_model(db, "out", &model).and_then(|v| v.parse().ok()),
            context: per_model(db, "ctx", &model).and_then(|v| v.parse().ok()),
            model,
            key: None,
        }
    }

    /// Settings under the lock, then the Keychain outside it: a Keychain prompt must not hold up the database.
    pub fn load(db: &Mutex<Connection>) -> Config {
        let mut c = Config::from_settings(&db.lock().unwrap());
        c.key = keychain::get(c.preset.id);
        c
    }

    /// Why a request cannot be sent yet, in words for the user.
    pub fn problem(&self) -> Option<&'static str> {
        if self.model.is_empty() {
            Some("Add a model name under AI provider in Settings.")
        } else if !matches!(Url::parse(&self.base_url), Ok(u) if matches!(u.scheme(), "http" | "https") && u.host().is_some()) {
            Some("The base URL under AI provider in Settings is not valid.")
        } else if self.key.is_none() && self.preset.id != "custom" {
            Some("Add an API key under AI provider in Settings.")
        } else {
            None
        }
    }

    /// Whether a request would leave this Mac: false only for a loopback host such as local Ollama.
    pub fn off_device(&self) -> bool {
        match Url::parse(&self.base_url).ok().and_then(|u| u.host().map(|h| h.to_owned())) {
            Some(Host::Domain(d)) => d != "localhost",
            Some(Host::Ipv4(ip)) => !ip.is_loopback(),
            Some(Host::Ipv6(ip)) => !ip.is_loopback(),
            None => true,
        }
    }

    /// The one request that leaves for a provider: a model and text messages, nothing else. Window video is never
    /// sent, so no path from a recording reaches this body (`Message::content` is a String, and the tests in
    /// this module, summary.rs and suggest.rs check the wire).
    pub fn complete(&self, messages: &[Message]) -> Result<Reply, String> {
        self.complete_within(messages, Duration::from_secs(180)) // long transcripts take a while to answer
    }

    /// Dictated or transcribed text tidied by the provider, or None if it can't be done in `within`: a slow
    /// network, an error or an empty reply all mean "keep your local text", never a wait.
    pub fn polish(&self, text: &str, within: Duration) -> Option<String> {
        let (config, text) = (self.clone(), text.to_string());
        let (tx, rx) = std::sync::mpsc::channel();
        // The deadline is on the reply, not the request: a DNS lookup or a connect that hangs is left behind.
        std::thread::spawn(move || {
            let messages = [
                Message { role: "system", content: POLISH_PROMPT.into() },
                Message { role: "user", content: text },
            ];
            let _ = tx.send(config.complete_within(&messages, within));
        });
        rx.recv_timeout(within).ok()?.ok().map(|r| r.text)
    }

    fn complete_within(&self, messages: &[Message], within: Duration) -> Result<Reply, String> {
        self.complete_images(messages, &[], within)
    }

    /// Like `complete`, with `images` attached to the last message. Only callers that checked `images_on` pass any;
    /// with none the body is exactly the text-only one.
    pub fn complete_images(&self, messages: &[Message], images: &[Image], within: Duration) -> Result<Reply, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = request_body(&self.model, messages, images);
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10).min(within))
            .timeout(within)
            .build();
        let mut req = agent.post(&url);
        if let Some(key) = &self.key {
            req = req.set("Authorization", &format!("Bearer {key}"));
        }
        let reply: Value = match req.send_json(body) {
            Ok(r) => r.into_json().map_err(|e| format!("The provider's reply was not JSON: {e}"))?,
            Err(ureq::Error::Status(code, r)) => {
                let body: Value = r.into_json().unwrap_or_default();
                let said = body["error"]["message"].as_str().or(body["error"].as_str()).unwrap_or("no details");
                return Err(format!("The provider answered {code}: {said}"));
            }
            Err(e) => return Err(format!("Could not reach {}: {e}", self.preset.name)),
        };
        let text = reply["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() {
            return Err("The provider sent an empty reply.".into());
        }
        // Providers that omit `usage` get the characters ÷ 4 estimate.
        let sent: usize = messages.iter().map(|m| m.content.chars().count()).sum();
        Ok(Reply {
            tokens_in: reply["usage"]["prompt_tokens"].as_u64().unwrap_or(sent as u64 / 4),
            tokens_out: reply["usage"]["completion_tokens"].as_u64().unwrap_or(text.chars().count() as u64 / 4),
            text,
        })
    }

    /// Dollars for one request; None unless both prices are filled in.
    pub fn cost(&self, tokens_in: u64, tokens_out: u64) -> Option<f64> {
        Some((tokens_in as f64 * self.input_price? + tokens_out as f64 * self.output_price?) / 1e6)
    }
}

/// The request body: a model and messages. Images ride as extra parts of the last message, each after a text part
/// that says when it was taken; that is the only way anything but text gets in.
fn request_body(model: &str, messages: &[Message], images: &[Image]) -> Value {
    use base64::Engine;
    let last = messages.len().saturating_sub(1);
    let messages: Vec<Value> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            if i != last || images.is_empty() {
                return json!({"role": m.role, "content": m.content});
            }
            let mut parts = vec![json!({"type": "text", "text": m.content})];
            for img in images {
                let url = format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(&img.jpeg));
                parts.push(json!({"type": "text", "text": img.caption}));
                parts.push(json!({"type": "image_url", "image_url": {"url": url}}));
            }
            json!({"role": m.role, "content": parts})
        })
        .collect();
    json!({"model": model, "messages": messages})
}

/// How long dictation waits on the provider before it pastes the local text.
pub const POLISH_TIMEOUT: Duration = Duration::from_secs(3);

const POLISH_PROMPT: &str = "You tidy dictated speech. Fix grammar and punctuation, drop false starts and \
    rambling, and tighten the phrasing, without changing the meaning, the language or any names. Reply with only \
    the revised text.";

/// A per-feature switch ("summaries" and so on); off until the user turns it on.
pub fn enabled(db: &Connection, feature: &str) -> bool {
    settings::get(db, &format!("ai_{feature}")).as_deref() == Some("true")
}

/// Whether screenshots may go to the provider for `feature` ("relevance" or "shots"): its Experimental toggle is on
/// and the model's "supports images" box is checked. Off unless both are, so images never go out by default.
pub fn images_on(db: &Connection, feature: &str, model: &str) -> bool {
    enabled(db, feature) && per_model(db, "img", model).as_deref() == Some("true")
}

/// What the Settings page and the Summary tab render; the shape is the contract with ui/ai.js.
#[derive(Serialize)]
pub struct State {
    presets: &'static [Preset],
    preset: &'static str,
    base_url: String,
    model: String,
    has_key: bool,
    input_price: Option<f64>,
    output_price: Option<f64>,
    context: Option<u64>,
    summaries: bool,
    folders: bool,
    chat: bool,
    /// The model's "supports images" box, and the two Experimental toggles that use it.
    images: bool,
    relevance: bool,
    shots: bool,
    /// Why a request cannot be sent yet.
    problem: Option<&'static str>,
    /// Whether a request would leave this Mac; drives the "sends text off this Mac" pill.
    off_device: bool,
}

#[tauri::command(async)]
pub fn ai_state(app: AppHandle) -> State {
    let db = app.state::<Db>();
    let c = Config::load(&db.0);
    let (summaries, folders, chat, relevance, shots, images) = {
        let conn = db.0.lock().unwrap();
        (
            enabled(&conn, "summaries"),
            enabled(&conn, "folders"),
            enabled(&conn, "chat"),
            enabled(&conn, "relevance"),
            enabled(&conn, "shots"),
            per_model(&conn, "img", &c.model).as_deref() == Some("true"),
        )
    };
    State {
        presets: PRESETS,
        preset: c.preset.id,
        has_key: c.key.is_some(),
        input_price: c.input_price,
        output_price: c.output_price,
        context: c.context,
        summaries,
        folders,
        chat,
        images,
        relevance,
        shots,
        problem: c.problem(),
        off_device: c.off_device(),
        model: c.model,
        base_url: c.base_url,
    }
}

/// An empty key removes it.
#[tauri::command(async)]
pub fn ai_set_key(preset: String, key: String) -> Result<(), String> {
    let id = PRESETS.iter().find(|p| p.id == preset).ok_or("unknown preset")?.id;
    match key.trim() {
        "" => keychain::remove(id),
        key => keychain::set(id, key),
    }
}

/// A local HTTP server that answers canned replies and records what it was sent, so provider tests
/// never leave the machine.
#[cfg(test)]
pub mod stub {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc::{channel, Receiver},
    };

    pub struct Seen {
        pub path: String,
        pub head: String,
        pub body: String,
    }

    /// Answers one request per `(status, body)`, in order. Returns the base URL and the requests seen.
    pub fn serve(replies: Vec<(u16, String)>) -> (String, Receiver<Seen>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut s, _) = listener.accept().unwrap();
                let mut buf = Vec::new();
                let split = loop {
                    let mut chunk = [0u8; 4096];
                    let n = s.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = String::from_utf8_lossy(&buf[..split]).to_string();
                let len = head
                    .lines()
                    .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                    .unwrap_or(0);
                while buf.len() < split + len {
                    let mut chunk = [0u8; 4096];
                    let n = s.read(&mut chunk).unwrap();
                    buf.extend_from_slice(&chunk[..n]);
                }
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let _ = tx.send(Seen { path, head, body: String::from_utf8_lossy(&buf[split..]).to_string() });
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (base, rx)
    }

    /// An in-memory database with the settings table.
    pub fn db() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
        c
    }

    /// Asserts a request body is only a model and text messages: no video, image or file parts, whatever the meeting
    /// had on disk. Every request builder's test runs its stub request through this.
    pub fn assert_text_only(body: &str) {
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        let keys: Vec<_> = body.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["messages", "model"], "unexpected parts in the request: {body}");
        for m in body["messages"].as_array().unwrap() {
            assert_eq!(m.as_object().unwrap().len(), 2, "a message with more than a role and text: {m}");
            assert!(m["role"].is_string() && m["content"].is_string(), "not plain text: {m}");
        }
        let text = body.to_string();
        assert!(!text.contains(".mp4") && !text.contains("window.mp4") && !text.contains("\"video"), "{text}");
    }

    /// For a request that may carry screenshots (the experimental toggle is on): the same checks as `assert_text_only`,
    /// except that the last message's content may be parts, and the only non-text part allowed is an inline JPEG.
    /// Returns the JPEG bytes in order. Video and file parts, and any extra key, still fail.
    pub fn assert_images_only(body: &str) -> Vec<Vec<u8>> {
        use base64::Engine;
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        let keys: Vec<_> = body.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["messages", "model"], "unexpected parts in the request: {body}");
        let mut jpegs = vec![];
        for m in body["messages"].as_array().unwrap() {
            assert_eq!(m.as_object().unwrap().len(), 2, "a message with more than a role and content: {m}");
            let Some(parts) = m["content"].as_array() else { assert!(m["content"].is_string(), "not text or parts: {m}"); continue };
            for p in parts {
                match p["type"].as_str() {
                    Some("text") => assert!(p["text"].is_string()),
                    Some("image_url") => {
                        let url = p["image_url"]["url"].as_str().unwrap();
                        let data = url.strip_prefix("data:image/jpeg;base64,").unwrap_or_else(|| panic!("not an inline JPEG: {url}"));
                        jpegs.push(base64::engine::general_purpose::STANDARD.decode(data).unwrap());
                    }
                    other => panic!("a {other:?} part in the request: {p}"),
                }
            }
        }
        assert!(!body.to_string().contains(".mp4"), "{body}");
        jpegs
    }

    /// A chat-completions reply carrying `text`, with token counts.
    pub fn ok(text: &str) -> (u16, String) {
        (200, format!(r#"{{"choices":[{{"message":{{"content":{text:?}}}}}],"usage":{{"prompt_tokens":1000,"completion_tokens":200}}}}"#))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(base_url: &str, key: Option<&str>) -> Config {
        Config {
            preset: &PRESETS[0],
            base_url: base_url.into(),
            model: "m".into(),
            key: key.map(Into::into),
            input_price: None,
            output_price: None,
            context: None,
        }
    }

    fn user(text: &str) -> Vec<Message> {
        vec![Message { role: "system", content: "Be brief.".into() }, Message { role: "user", content: text.into() }]
    }

    #[test]
    fn request_goes_to_chat_completions_with_the_key() {
        let (base, seen) = stub::serve(vec![stub::ok("  Hello there  ")]);
        let reply = config(&base, Some("sk-test")).complete(&user("hi")).unwrap();
        assert_eq!((reply.text.as_str(), reply.tokens_in, reply.tokens_out), ("Hello there", 1000, 200));
        let req = seen.recv().unwrap();
        assert_eq!(req.path, "/v1/chat/completions");
        assert!(req.head.to_lowercase().contains("authorization: bearer sk-test"));
        let body: Value = serde_json::from_str(&req.body).unwrap();
        assert_eq!(body["model"], "m");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
        stub::assert_text_only(&req.body);
    }

    #[test]
    fn a_request_never_carries_video_even_when_the_transcript_mentions_one() {
        // Text that talks about a recording is still just text; the body has no other part for a video to ride in.
        let (base, seen) = stub::serve(vec![stub::ok("ok")]);
        config(&base, None).complete(&user("We watched the recording of the call.")).unwrap();
        stub::assert_text_only(&seen.recv().unwrap().body);
        let result = std::panic::catch_unwind(|| stub::assert_text_only(r#"{"model":"m","messages":[{"role":"user","content":[{"type":"video","url":"/x/window.mp4"}]}]}"#));
        assert!(result.is_err(), "the check must reject a message with a video part");
        let result = std::panic::catch_unwind(|| stub::assert_text_only(r#"{"model":"m","messages":[],"video":"/x/window.mp4"}"#));
        assert!(result.is_err(), "the check must reject a video beside the messages");
    }

    #[test]
    fn images_ride_on_the_last_message_with_a_caption_and_text_only_stays_text_only() {
        let (base, seen) = stub::serve(vec![stub::ok("ok"), stub::ok("ok")]);
        let images = [Image { caption: "Screenshot at 00:05".into(), jpeg: b"abc".to_vec() }];
        config(&base, None).complete_images(&user("hi"), &images, Duration::from_secs(5)).unwrap();
        let body = seen.recv().unwrap().body;
        assert_eq!(stub::assert_images_only(&body), [b"abc".to_vec()]);
        assert!(std::panic::catch_unwind(|| stub::assert_text_only(&body)).is_err(), "the text-only check must reject images");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["messages"][0]["content"], "Be brief."); // earlier messages stay plain text
        assert_eq!(v["messages"][1]["content"][1]["text"], "Screenshot at 00:05");
        // No images: the body is the plain one.
        config(&base, None).complete_images(&user("hi"), &[], Duration::from_secs(5)).unwrap();
        stub::assert_text_only(&seen.recv().unwrap().body);
    }

    #[test]
    fn images_are_on_only_with_the_toggle_and_the_models_box() {
        let db = stub::db();
        assert!(!images_on(&db, "shots", "m"));
        settings::set(&db, "ai_shots", "true").unwrap();
        assert!(!images_on(&db, "shots", "m"));
        settings::set(&db, "ai_img:m", "true").unwrap();
        assert!(images_on(&db, "shots", "m") && !images_on(&db, "relevance", "m") && !images_on(&db, "shots", "n"));
    }

    #[test]
    fn polish_returns_the_tidied_text() {
        let (base, seen) = stub::serve(vec![stub::ok("Ship it.")]);
        assert_eq!(config(&base, None).polish("um ship it", POLISH_TIMEOUT).as_deref(), Some("Ship it."));
        let sent = seen.recv().unwrap().body;
        stub::assert_text_only(&sent); // the cleanup path is a provider request too: text only, never video
        let body: Value = serde_json::from_str(&sent).unwrap();
        assert_eq!(body["messages"][1]["content"], "um ship it");
    }

    /// Accepts connections and sits on them, the way a provider with a dead network behind it does.
    fn slow_stub() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in listener.incoming().flatten() {
                std::thread::sleep(Duration::from_secs(5));
                drop(s);
            }
        });
        base
    }

    #[test]
    fn a_slow_provider_times_out_and_the_caller_keeps_its_text() {
        let started = std::time::Instant::now();
        assert_eq!(config(&slow_stub(), None).polish("um ship it", Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(2), "waited {:?}", started.elapsed());
    }

    #[test]
    fn dictation_pastes_the_rules_text_when_the_provider_is_slow() {
        let c = config(&slow_stub(), None);
        let out = crate::cleanup::clean("um ship it", true, &[], &|t| c.polish(t, Duration::from_millis(300)));
        assert_eq!(out, "Ship it");
    }

    #[test]
    fn a_failing_provider_also_gives_none() {
        let (base, _seen) = stub::serve(vec![(500, r#"{"error":{"message":"down"}}"#.into())]);
        assert_eq!(config(&base, None).polish("hi", POLISH_TIMEOUT), None);
    }

    #[test]
    fn a_local_server_gets_no_authorization_header() {
        let (base, seen) = stub::serve(vec![stub::ok("ok")]);
        config(&base, None).complete(&user("hi")).unwrap();
        assert!(!seen.recv().unwrap().head.to_lowercase().contains("authorization"));
    }

    #[test]
    fn provider_errors_come_back_readable_and_without_the_key() {
        let (base, _seen) = stub::serve(vec![(401, r#"{"error":{"message":"No auth credentials found"}}"#.into())]);
        let err = config(&base, Some("sk-secret")).complete(&user("hi")).unwrap_err();
        assert_eq!(err, "The provider answered 401: No auth credentials found");
        assert!(!err.contains("sk-secret"));
    }

    #[test]
    fn an_empty_reply_is_an_error_and_a_missing_usage_is_estimated() {
        let (base, _seen) = stub::serve(vec![(200, r#"{"choices":[{"message":{"content":null}}]}"#.into())]);
        assert!(config(&base, None).complete(&user("hi")).is_err());
        let (base, _seen) = stub::serve(vec![(200, r#"{"choices":[{"message":{"content":"12345678"}}]}"#.into())]);
        let reply = config(&base, None).complete(&user(&"x".repeat(40))).unwrap();
        // "Be brief." (9) + 40 characters in, 8 out, each ÷ 4.
        assert_eq!((reply.tokens_in, reply.tokens_out), (12, 2));
    }

    #[test]
    fn the_off_device_flag_follows_whether_a_request_would_leave_the_mac() {
        for (url, off) in [
            ("https://api.openai.com/v1", true),
            ("https://openrouter.ai/api/v1", true),
            ("http://192.168.1.20:11434/v1", true),
            ("http://localhost:11434/v1", false),
            ("http://127.0.0.1:8080/v1", false),
            ("http://[::1]:8080/v1", false),
            ("not a url", true),
        ] {
            assert_eq!(config(url, None).off_device(), off, "{url}");
        }
        // The flag is about the address, not the preset: the stub the tests talk to is on loopback.
        let (base, _seen) = stub::serve(vec![]);
        assert!(!config(&base, Some("k")).off_device());
    }

    #[test]
    fn cost_needs_both_prices_and_problem_names_what_is_missing() {
        let mut c = config("https://x.test/v1", None);
        assert_eq!(c.cost(1000, 1000), None);
        c.input_price = Some(0.25);
        assert_eq!(c.cost(1000, 1000), None);
        c.output_price = Some(2.0);
        assert!((c.cost(1_000_000, 500_000).unwrap() - 1.25).abs() < 1e-9);

        assert!(c.problem().unwrap().contains("API key")); // hosted preset, no key
        c.key = Some("k".into());
        assert_eq!(c.problem(), None);
        c.model.clear();
        assert!(c.problem().unwrap().contains("model"));
        c.model = "m".into();
        c.base_url = "ftp://x".into();
        assert!(c.problem().unwrap().contains("base URL"));
        // A custom (local) preset works without a key.
        let local = Config { preset: &PRESETS[3], ..config("http://localhost:11434/v1", None) };
        assert_eq!(local.problem(), None);
    }

    #[test]
    fn settings_become_a_config_with_per_model_prices() {
        let db = stub::db();
        assert_eq!(Config::from_settings(&db).base_url, PRESETS[0].base_url);
        settings::set(&db, "ai_preset", "openai").unwrap();
        settings::set(&db, "ai_model", "gpt-x").unwrap();
        settings::set(&db, "ai_in:gpt-x", "$0.25").unwrap();
        settings::set(&db, "ai_out:gpt-x", "2").unwrap();
        settings::set(&db, "ai_ctx:gpt-x", "400000").unwrap();
        settings::set(&db, "ai_in:other", "9").unwrap();
        let c = Config::from_settings(&db);
        assert_eq!((c.preset.id, c.base_url.as_str()), ("openai", "https://api.openai.com/v1"));
        assert_eq!((c.input_price, c.output_price, c.context), (Some(0.25), Some(2.0), Some(400_000)));
        settings::set(&db, "ai_model", "other").unwrap();
        let c = Config::from_settings(&db);
        assert_eq!((c.input_price, c.output_price, c.context), (Some(9.0), None, None));
    }

    /// Writes a throwaway item to the login Keychain, so it stays out of the default run:
    /// `cargo test keychain -- --ignored`.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn keychain_round_trip() {
        let account = format!("test-{}", std::process::id());
        assert_eq!(keychain::get(&account), None);
        keychain::set(&account, "sk-one").unwrap();
        keychain::set(&account, "sk-two").unwrap();
        assert_eq!(keychain::get(&account).as_deref(), Some("sk-two"));
        keychain::remove(&account).unwrap();
        keychain::remove(&account).unwrap(); // already gone is fine
        assert_eq!(keychain::get(&account), None);
    }
}
