//! What a model costs, taken from the model and never from the user: OpenRouter's own `/models` list when that is
//! the provider, otherwise `prices.json` (bundled, and refreshed from the repo once a day like extension/selectors.json).
//! A model in neither is unknown; a price is never guessed.
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// Dollars per 1M tokens, (input, output), by model id.
type Table = HashMap<String, (f64, f64)>;

// shortcut: the bundled prices are typed in by hand and go stale; re-check them when a preset's default model
// changes, and bump "version" in prices.json so the daily fetch can replace what shipped.
const BUNDLED: &str = include_str!("../prices.json");
const URL: &str = "https://raw.githubusercontent.com/dhruvsaxena1998/myna/main/src-tauri/prices.json";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn parse_prices(text: &str) -> Option<(u64, Table)> {
    let j: Value = serde_json::from_str(text).ok()?;
    let models = j["models"].as_object()?;
    let table = models
        .iter()
        .map(|(id, p)| Some((id.clone(), (p["in"].as_f64()?, p["out"].as_f64()?))))
        .collect::<Option<Table>>()?;
    Some((j["version"].as_u64()?, table))
}

fn bundled() -> (u64, Table) {
    parse_prices(BUNDLED).expect("src-tauri/prices.json is valid")
}

/// The fetched file counts only if its version beats the bundled one.
fn parse_fetched(text: &str) -> Option<Table> {
    parse_prices(text).filter(|(v, _)| *v > bundled().0).map(|(_, t)| t)
}

/// OpenRouter lists per-token dollars as strings; a negative one ("openrouter/auto") means it varies, so unknown.
fn parse_openrouter(text: &str) -> Option<Table> {
    let j: Value = serde_json::from_str(text).ok()?;
    let per_m = |p: &Value| p.as_str()?.parse::<f64>().ok().filter(|v| *v >= 0.0).map(|v| v * 1e6);
    Some(
        j["data"]
            .as_array()?
            .iter()
            .filter_map(|m| Some((m["id"].as_str()?.to_string(), (per_m(&m["pricing"]["prompt"])?, per_m(&m["pricing"]["completion"])?))))
            .collect(),
    )
}

/// `url` fetched at most once a day per run. A failed fetch is not retried until tomorrow: the last table (or none) stands.
// shortcut: in memory only, so a restart fetches again; persist it if that ever matters.
fn cached(url: &str, parse: fn(&str) -> Option<Table>) -> Table {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Table)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let old = match cache.lock().unwrap().get(url) {
        Some((at, t)) if at.elapsed() < DAY => return t.clone(),
        Some((_, t)) => t.clone(),
        None => Table::new(),
    };
    let fetched = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .build()
        .get(url)
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .and_then(|t| parse(&t));
    let table = fetched.unwrap_or(old);
    cache.lock().unwrap().insert(url.into(), (Instant::now(), table.clone()));
    table
}

/// Per 1M tokens, (input, output) for `model`: from OpenRouter at `openrouter_base` when given, else the price
/// table. None for a model nobody lists.
pub fn lookup(openrouter_base: Option<&str>, model: &str) -> Option<(f64, f64)> {
    let live = match openrouter_base {
        Some(base) => cached(&format!("{}/models", base.trim_end_matches('/')), parse_openrouter),
        None => cached(URL, parse_fetched),
    };
    live.get(model).copied().or_else(|| bundled().1.get(model).copied())
}

/// Dollars for a request at `price` per 1M tokens.
pub fn cost((input, output): (f64, f64), tokens_in: u64, tokens_out: u64) -> f64 {
    (tokens_in as f64 * input + tokens_out as f64 * output) / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{stub, PRESETS};

    #[test]
    fn the_bundled_table_covers_the_hosted_presets_default_models() {
        let (_, t) = bundled();
        for p in PRESETS.iter().filter(|p| matches!(p.id, "openai" | "anthropic")) {
            assert!(t.contains_key(p.model), "no bundled price for {}", p.model);
        }
    }

    #[test]
    fn estimate_math_is_tokens_times_price_per_million() {
        assert!((cost((0.25, 2.0), 1_000_000, 500_000) - 1.25).abs() < 1e-9);
        assert_eq!(cost((0.25, 2.0), 0, 0), 0.0);
    }

    #[test]
    fn the_table_gives_a_price_and_a_model_it_lacks_has_none() {
        assert_eq!(parse_prices(r#"{"version":1,"models":{"m":{"in":1,"out":2.5}}}"#).unwrap().1["m"], (1.0, 2.5));
        assert_eq!(bundled().1.get("no-such-model"), None);
        assert!(parse_prices(r#"{"version":1,"models":{"m":{"in":1}}}"#).is_none()); // half a price is no price
    }

    #[test]
    fn a_fetched_table_wins_only_with_a_higher_version() {
        let v = bundled().0;
        let file = |v: u64| format!(r#"{{"version":{v},"models":{{"m":{{"in":1,"out":2}}}}}}"#);
        assert!(parse_fetched(&file(v)).is_none());
        assert_eq!(parse_fetched(&file(v + 1)).unwrap()["m"], (1.0, 2.0));
    }

    #[test]
    fn openrouter_prices_are_per_token_strings_and_variable_ones_are_unknown() {
        let t = parse_openrouter(
            r#"{"data":[
                {"id":"openai/gpt-5-mini","pricing":{"prompt":"0.00000025","completion":"0.000002"}},
                {"id":"x/free","pricing":{"prompt":"0","completion":"0"}},
                {"id":"openrouter/auto","pricing":{"prompt":"-1","completion":"-1"}}]}"#,
        )
        .unwrap();
        let (i, o) = t["openai/gpt-5-mini"];
        assert!((i - 0.25).abs() < 1e-9 && (o - 2.0).abs() < 1e-9);
        assert_eq!(t["x/free"], (0.0, 0.0));
        assert!(!t.contains_key("openrouter/auto"));
    }

    #[test]
    fn openrouter_is_asked_once_and_an_unlisted_model_is_unknown() {
        let list = r#"{"data":[{"id":"a/b","pricing":{"prompt":"0.000001","completion":"0.000003"}}]}"#;
        let (base, seen) = stub::serve(vec![(200, list.into())]);
        let (i, o) = lookup(Some(&base), "a/b").unwrap();
        assert!((i - 1.0).abs() < 1e-9 && (o - 3.0).abs() < 1e-9);
        assert_eq!(seen.recv().unwrap().path, "/v1/models");
        assert_eq!(lookup(Some(&base), "a/c"), None);
        assert!(seen.try_recv().is_err()); // cached
    }

    #[test]
    fn a_failed_fetch_is_not_retried_and_leaves_the_model_unknown() {
        let (base, seen) = stub::serve(vec![(500, "{}".into())]);
        assert_eq!(lookup(Some(&base), "a/b"), None);
        assert_eq!(lookup(Some(&base), "a/b"), None);
        seen.recv().unwrap();
        assert!(seen.try_recv().is_err());
    }
}
