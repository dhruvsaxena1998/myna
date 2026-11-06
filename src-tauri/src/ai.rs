//! AI providers: where Myna sends text for an LLM to answer, and which model each slot uses.
//! Records live in SQLite, keys in the macOS Keychain (never in SQLite, never sent to the frontend).
//! Every call goes through the `Provider` trait, like `runtime::Transcriber`; one adapter, OpenAI-compatible chat.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const KINDS: [&str; 2] = ["openai", "openrouter"];
const SLOTS: [&str; 2] = ["main", "fast"];
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
    id: i64,
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
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Model {
    pub id: String,
    pub input_per_m: Option<f64>,
    pub output_per_m: Option<f64>,
    pub context: Option<u64>,
    pub images: Option<bool>,
}

#[cfg_attr(not(test), allow(dead_code))] // first caller is a feature (stories 16, 17)
#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Cost {
    Usd { usd: f64 },
    Free,
    Unknown,
}

#[cfg_attr(not(test), allow(dead_code))] // tokens are first read by Summary and Chat (stories 16, 17)
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    pub text: String,
    pub tokens: Option<(u64, u64)>,
    pub cost: Cost,
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
        let unreachable = |e: String| format!("Couldn't reach {}: {e}", self.url);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(secs)))
            .http_status_as_error(false)
            .build()
            .into();
        let mut req = ureq::http::Request::builder()
            .method(if body.is_some() { "POST" } else { "GET" })
            .uri(format!("{}{path}", self.url))
            .header("Content-Type", "application/json");
        if let Some(k) = &self.key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        let req = req.body(body.unwrap_or_default()).map_err(|_| unreachable("that key can't be sent".into()))?;
        let mut res = agent.run(req).map_err(|e| unreachable(e.to_string()))?;
        let status = res.status().as_u16();
        let text = res.body_mut().read_to_string().map_err(|e| unreachable(e.to_string()))?;
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

impl Backend for OpenAi {
    fn models(&self) -> Result<Vec<Model>, String> {
        let json = self.send("/models", None, 15)?;
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

    fn key_usage(&self) -> Result<Option<(f64, Option<f64>)>, String> {
        if !self.rich {
            return Ok(None);
        }
        let d = &self.send("/key", None, 15)?["data"];
        let used = d["usage"].as_f64().ok_or_else(|| format!("Couldn't read the key's usage from {}", self.url))?;
        Ok(Some((used, d["limit"].as_f64())))
    }

    fn chat(&self, model: &str, system: Option<&str>, prompt: &str, secs: u64) -> Result<Reply, String> {
        let messages: Vec<Value> = system.map(|s| json!({"role": "system", "content": s})).into_iter().chain([json!({"role": "user", "content": prompt})]).collect();
        let body = json!({"model": model, "messages": messages}).to_string();
        let json = self.send("/chat/completions", Some(body), secs)?;
        let text = json["choices"][0]["message"]["content"].as_str().ok_or_else(|| format!("{} sent no reply", self.url))?.to_string();
        let tokens = json["usage"]["prompt_tokens"].as_u64().zip(json["usage"]["completion_tokens"].as_u64());
        let reported = json["usage"]["cost"].as_f64();
        // Only fetch prices when the reply didn't carry its own cost.
        let price = match (reported, self.rich) {
            (None, true) => self.models().ok().and_then(|l| l.into_iter().find(|m| m.id == model)).and_then(|m| m.input_per_m.zip(m.output_per_m)),
            _ => None,
        };
        Ok(Reply { text, tokens, cost: cost_of(reported, price, tokens, !off_device(&self.url)) })
    }
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
    // No saved list means unknown, not gone: a failed fetch never marks a model as gone.
    Ok(r.map(|(provider, model)| Route { gone: cached(db, provider).is_some_and(|l| !l.iter().any(|m| m.id == model)), provider, model }))
}

pub fn get(db: &Connection, keys: &dyn Keys) -> Result<State, String> {
    let mut q = db.prepare("SELECT id, kind, name, url FROM provider ORDER BY id").map_err(|e| e.to_string())?;
    let rows = q.query_map([], record).map_err(|e| e.to_string())?;
    let providers = rows.map(|r| view(r.map_err(|e| e.to_string())?, keys)).collect::<Result<_, _>>()?;
    Ok(State { providers, main: get_route(db, "main")?, fast: get_route(db, "fast")? })
}

pub fn add(db: &Connection, keys: &dyn Keys, kind: &str, name: &str, url: &str, key: &str) -> Result<Provider, String> {
    let (name, url, key) = (name.trim(), url.trim().trim_end_matches('/'), key.trim());
    if !KINDS.contains(&kind) {
        return Err("Pick a kind of provider".into());
    }
    if name.is_empty() || name.chars().count() > 40 {
        return Err("Give the provider a name of up to 40 characters".into());
    }
    let taken = db.query_row("SELECT 1 FROM provider WHERE name = ?1 COLLATE NOCASE", [name], |_| Ok(())).optional().map_err(|e| e.to_string())?;
    if taken.is_some() {
        return Err(format!("There is already a provider called {name}"));
    }
    if host(url).is_none() {
        return Err("Enter a web address starting with http:// or https://".into());
    }
    if kind == "openrouter" && !key.starts_with("sk-or-") {
        return Err("An OpenRouter key starts with sk-or-".into());
    }
    db.execute("INSERT INTO provider(kind, name, url) VALUES (?1, ?2, ?3)", params![kind, name, url]).map_err(|e| e.to_string())?;
    let id = db.last_insert_rowid();
    if !key.is_empty() && let Err(e) = keys.set(id, key) {
        let _ = db.execute("DELETE FROM provider WHERE id = ?1", [id]);
        return Err(e);
    }
    view(find(db, id)?, keys)
}

/// Deletes the provider, its key and the routes to it; returns the slots that are now Not set.
pub fn remove(db: &mut Connection, keys: &dyn Keys, id: i64) -> Result<Vec<String>, String> {
    find(db, id)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let slots = {
        let mut q = tx.prepare("SELECT slot FROM route WHERE provider_id = ?1 ORDER BY slot DESC").map_err(|e| e.to_string())?; // main before fast
        q.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<Result<Vec<String>, _>>().map_err(|e| e.to_string())?
    };
    tx.execute("DELETE FROM route WHERE provider_id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM model_list WHERE provider_id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM provider WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    keys.delete(id)?;
    Ok(slots)
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

/// The adapter for provider `id`, holding its key.
pub fn provider(db: &Connection, keys: &dyn Keys, id: i64) -> Result<Box<dyn Backend>, String> {
    let r = find(db, id)?;
    Ok(Box::new(OpenAi { key: keys.get(id)?, rich: r.kind == "openrouter", url: r.url }))
}

fn cached(db: &Connection, id: i64) -> Option<Vec<Model>> {
    let json: String = db.query_row("SELECT json FROM model_list WHERE provider_id = ?1", [id], |r| r.get(0)).optional().ok()??;
    serde_json::from_str(&json).ok()
}

fn store(db: &Connection, id: i64, list: &[Model]) -> Result<(), String> {
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
    let p = provider(&db.lock().unwrap(), keys, id)?; // the lock is not held while waiting on the network
    let list = p.models()?;
    store(&db.lock().unwrap(), id, &list)?;
    Ok(list)
}

/// Checks the address and key without spending tokens: OpenRouter's key endpoint, else the model list (cached on the way).
pub fn test(db: &Mutex<Connection>, id: i64, p: &dyn Backend) -> Result<Check, String> {
    let start = Instant::now();
    let (mut count, mut used, mut limit) = (None, None, None);
    match p.key_usage()? {
        Some((u, l)) => (used, limit) = (Some(u), l),
        None => {
            let list = p.models()?;
            store(&db.lock().unwrap(), id, &list)?;
            count = Some(list.len());
        }
    }
    Ok(Check { ms: start.elapsed().as_millis() as u64, models: count, used, limit })
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
            Ok(Reply { text: format!("hi from {model}"), tokens: Some((3, 4)), cost: Cost::Free })
        }
    }

    fn db() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let c = crate::storage::open(tmp.path()).unwrap();
        (tmp, c)
    }

    fn add_ok(db: &Connection, keys: &Memory, kind: &str, name: &str, url: &str, key: &str) -> Provider {
        add(db, keys, kind, name, url, key).unwrap()
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

    /// Serves each `(path, status, body)` and records nothing; unknown paths get 404.
    pub(crate) fn stub(routes: Vec<(&'static str, u16, &'static str)>) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
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
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let (status, body) = routes.iter().find(|r| r.0 == path).map(|r| (r.1, r.2)).unwrap_or((404, "{}"));
                let _ = write!(s, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        base
    }

    fn adapter(url: String, rich: bool) -> OpenAi {
        OpenAi { url, key: Some("k".into()), rich }
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
}
