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
    const SERVICE: &str = "com.myna.app.ai";
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

    pub fn complete(&self, messages: &[Message]) -> Result<Reply, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "messages": messages.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        });
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(180)) // long transcripts take a while to answer
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

/// A per-feature switch ("summaries" and so on); off until the user turns it on.
pub fn enabled(db: &Connection, feature: &str) -> bool {
    settings::get(db, &format!("ai_{feature}")).as_deref() == Some("true")
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
    /// Why a request cannot be sent yet.
    problem: Option<&'static str>,
    /// Whether a request would leave this Mac; drives the "sends text off this Mac" pill.
    off_device: bool,
}

#[tauri::command(async)]
pub fn ai_state(app: AppHandle) -> State {
    let db = app.state::<Db>();
    let c = Config::load(&db.0);
    let summaries = enabled(&db.0.lock().unwrap(), "summaries");
    State {
        presets: PRESETS,
        preset: c.preset.id,
        has_key: c.key.is_some(),
        input_price: c.input_price,
        output_price: c.output_price,
        context: c.context,
        summaries,
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
