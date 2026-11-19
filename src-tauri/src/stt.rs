//! Meeting speech-to-text: which transcriber writes a meeting up, from meeting notes' setting or a one-off pick
//! (Transcribe again). The setting is stored as before: `ai::STT` for the Endpoint, `models::MEETING_STT` for the
//! Local model.

use crate::ai::{self, Keys, Own, SttEndpoint};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Meeting speech-to-text as Settings and Transcribe again see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pick {
    /// A model on this Mac; none is meeting notes' own (Settings › Meeting notes › Speech-to-text).
    Local {
        #[serde(default)]
        model: Option<String>,
    },
    Endpoint { provider: i64, model: String },
    /// The setting's Endpoint lost its provider: new meetings wait until a model is picked again.
    NotSet,
}

/// What a write-up transcribes with.
pub enum Transcriber {
    /// On this Mac; none is meeting notes' own model.
    Local(Option<String>),
    Endpoint(SttEndpoint),
    /// The setting picks an Endpoint that can't be used, and why: its chunks fail with this, and the meeting offers
    /// Transcribe locally.
    Unavailable(String),
}

/// The setting for new meetings.
pub fn saved(db: &Connection) -> Result<Pick, String> {
    Ok(match ai::own(db, ai::STT)? {
        Own::Slot => Pick::Local { model: None },
        Own::NotSet => Pick::NotSet,
        Own::Pick(provider, model) => Pick::Endpoint { provider, model },
    })
}

/// Back to the Local model. An Endpoint is only picked through `ai::stt_test`.
pub fn use_local(db: &Connection) -> Result<(), String> {
    ai::feature_set(db, ai::STT, "local")
}

/// The setting's transcriber, or `pick`'s for this once. A setting that can't be used still writes up (Unavailable);
/// a pick that can't is refused.
pub fn resolve(db: &Connection, keys: &dyn Keys, pick: Option<&Pick>) -> Result<Transcriber, String> {
    match pick {
        None => Ok(match ai::stt_endpoint(db, keys) {
            Ok(None) => Transcriber::Local(None),
            Ok(Some(e)) => Transcriber::Endpoint(e),
            Err(e) => Transcriber::Unavailable(e),
        }),
        Some(Pick::Local { model }) => Ok(Transcriber::Local(model.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(Into::into))),
        Some(Pick::Endpoint { provider, model }) if !model.trim().is_empty() => ai::stt_endpoint_for(db, keys, *provider, model.trim()).map(Transcriber::Endpoint),
        Some(_) => Err("Pick a model to transcribe with.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::{add_ok, db, Memory};

    fn endpoint(t: Result<Transcriber, String>) -> (String, String) {
        match t {
            Ok(Transcriber::Endpoint(e)) => (e.provider, e.model),
            _ => panic!("not an Endpoint"),
        }
    }

    #[test]
    fn the_setting_is_the_local_model_until_an_endpoint_is_picked_and_not_set_once_its_provider_goes() {
        let (_t, mut c) = db();
        let keys = Memory::default();
        assert_eq!(saved(&c).unwrap(), Pick::Local { model: None });
        assert!(matches!(resolve(&c, &keys, None), Ok(Transcriber::Local(None))));
        let p = add_ok(&c, &keys, "openai", "Whisper", "http://localhost:8000/v1", "").id;
        ai::feature_set(&c, ai::STT, &format!("{p}|whisper-1")).unwrap();
        assert_eq!(saved(&c).unwrap(), Pick::Endpoint { provider: p, model: "whisper-1".into() });
        assert_eq!(endpoint(resolve(&c, &keys, None)), ("Whisper".into(), "whisper-1".into()));
        ai::remove(&mut c, &keys, p).unwrap();
        assert_eq!(saved(&c).unwrap(), Pick::NotSet);
        assert!(matches!(resolve(&c, &keys, None), Ok(Transcriber::Unavailable(e)) if e == "Meeting speech-to-text: Not set"), "the write-up still runs, offering Transcribe locally");
        use_local(&c).unwrap();
        assert_eq!(saved(&c).unwrap(), Pick::Local { model: None });
    }

    #[test]
    fn a_pick_for_this_once_wins_over_the_setting_and_a_bad_one_is_refused() {
        let (_t, c) = db();
        let keys = Memory::default();
        let p = add_ok(&c, &keys, "openai", "Whisper", "http://localhost:8000/v1", "").id;
        let local = |m: &str| resolve(&c, &keys, Some(&Pick::Local { model: Some(m.into()) }));
        assert!(matches!(local(" whisper-turbo "), Ok(Transcriber::Local(Some(m))) if m == "whisper-turbo"));
        assert!(matches!(local(" "), Ok(Transcriber::Local(None))), "a blank local model is meeting notes' own");
        assert_eq!(endpoint(resolve(&c, &keys, Some(&Pick::Endpoint { provider: p, model: " large-v3 ".into() }))), ("Whisper".into(), "large-v3".into()));
        assert_eq!(saved(&c).unwrap(), Pick::Local { model: None }, "the setting for new meetings stays");
        let refused = |pick: Pick| resolve(&c, &keys, Some(&pick)).err().unwrap();
        assert_eq!(refused(Pick::Endpoint { provider: p, model: "  ".into() }), "Pick a model to transcribe with.");
        assert_eq!(refused(Pick::NotSet), "Pick a model to transcribe with.");
        assert_eq!(refused(Pick::Endpoint { provider: p + 1, model: "m".into() }), "That provider no longer exists");
    }

    #[test]
    fn a_pick_reads_and_writes_as_the_page_sends_it() {
        let read = |s: &str| serde_json::from_str::<Pick>(s).unwrap();
        assert_eq!(read(r#"{"kind":"local"}"#), Pick::Local { model: None });
        assert_eq!(read(r#"{"kind":"local","model":"a"}"#), Pick::Local { model: Some("a".into()) });
        assert_eq!(read(r#"{"kind":"endpoint","provider":3,"model":"m"}"#), Pick::Endpoint { provider: 3, model: "m".into() });
        assert_eq!(serde_json::to_string(&Pick::NotSet).unwrap(), r#"{"kind":"not_set"}"#);
    }
}
