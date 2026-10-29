//! The mic's mute state during a meeting, and which transcript segments it lets in.
//! The mic is always recorded to disk; the state only decides what reads as transcript.
use super::store;
use crate::settings;
use rusqlite::Connection;
use serde::Serialize;

/// Setting that holds the apps (a JSON array of tapped-app names) whose mic is always included.
pub const ALWAYS_KEY: &str = "mic_always_apps";

/// Who reported a mute change: Myna's own toggle, or the browser extension (ticket 10), which feeds the same timeline.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Manual,
    Extension,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Extension => "extension",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "extension" { Self::Extension } else { Self::Manual }
    }
}

/// The mic went muted or live `at_ms` after the recording started, and stays so until the next event.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub at_ms: u64,
    pub muted: bool,
    pub source: Source,
}

/// Where a mic segment stands. Stored and sent to the UI as "unmuted", "muted" or "unknown".
#[derive(Serialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Unmuted,
    Muted,
    Unknown,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unmuted => "unmuted",
            Self::Muted => "muted",
            Self::Unknown => "unknown",
        }
    }

    /// Anything unrecognised is `Unknown`, so a bad value keeps the line out rather than letting it in.
    pub fn parse(s: &str) -> Self {
        match s {
            "unmuted" => Self::Unmuted,
            "muted" => Self::Muted,
            _ => Self::Unknown,
        }
    }
}

/// The state of the mic across a segment spanning `start_ms..end_ms`, from the toggles in time order.
/// Only a segment unmuted from start to end is `Unmuted`. Any muted moment makes it `Muted`. Time before the
/// first toggle is unknown, so with no mute source every line stays out of the transcript. `always` (the per-app
/// opt-out) reads unknown as unmuted but never overrides a mute.
pub fn state(events: &[Event], start_ms: u64, end_ms: u64, always: bool) -> State {
    let at = |t: u64| events.iter().rev().find(|e| e.at_ms <= t).map(|e| e.muted);
    let inside = events.iter().filter(|e| e.at_ms > start_ms && e.at_ms < end_ms).map(|e| Some(e.muted));
    let seen: Vec<Option<bool>> = std::iter::once(at(start_ms)).chain(inside).collect();
    if seen.contains(&Some(true)) {
        State::Muted
    } else if seen.contains(&None) && !always {
        State::Unknown
    } else {
        State::Unmuted
    }
}

/// What the popover and the pill say about the mic: always text plus a dot, never colour alone.
#[derive(Serialize, PartialEq, Debug)]
pub struct Mic {
    pub live: bool,
    /// Where the state comes from: "manual", "mirrored" or "always on".
    pub how: &'static str,
}

pub fn mic_now(events: &[Event], always: bool) -> Mic {
    match events.last() {
        Some(e) => Mic { live: !e.muted, how: if e.source == Source::Manual { "manual" } else { "mirrored" } },
        None => Mic { live: always, how: if always { "always on" } else { "manual" } },
    }
}

// shortcut: matches the whole tapped-app label, so two apps on the mic at once ("zoom.us, Google Chrome") read as one name.
/// Whether the meeting's app is on the always-record list.
pub fn always_record(db: &Connection, meeting: i64) -> bool {
    let apps: Vec<String> = settings::get(db, ALWAYS_KEY).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    store::get(db, meeting).ok().and_then(|m| m.app).is_some_and(|a| apps.contains(&a))
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1000;

    fn ev(at_s: u64, muted: bool) -> Event {
        Event { at_ms: at_s * S, muted, source: Source::Manual }
    }

    #[test]
    fn segment_state_follows_the_timeline_and_fails_safe() {
        use State::*;
        // Live from 0 s, muted 10-20 s, live again from 20 s.
        let gap = [ev(0, false), ev(10, true), ev(20, false)];
        // (name, events, start s, end s, always, want)
        let table: [(&str, &[Event], u64, u64, bool, State); 15] = [
            ("no mute source at all", &[], 1, 3, false, Unknown),
            ("no mute source, always record", &[], 1, 3, true, Unmuted),
            ("live throughout", &gap, 2, 4, false, Unmuted),
            ("inside the muted span", &gap, 11, 15, false, Muted),
            ("muted wins over always record", &gap, 11, 15, true, Muted),
            ("starts live, runs into the mute", &gap, 8, 12, false, Muted),
            ("starts muted, ends after unmute", &gap, 19, 22, false, Muted),
            ("live again after the unmute", &gap, 20, 25, false, Unmuted),
            ("ends exactly when the mute starts", &gap, 5, 10, false, Unmuted),
            ("begins exactly when the unmute happens", &gap, 20, 21, false, Unmuted),
            ("zero-length segment while muted", &gap, 12, 12, false, Muted),
            ("before the first toggle", &[ev(5, false)], 1, 3, false, Unknown),
            ("before the first toggle, always record", &[ev(5, false)], 1, 3, true, Unmuted),
            ("spans unknown then live", &[ev(5, false)], 3, 8, false, Unknown),
            ("muted and unknown in one span", &[ev(5, true)], 3, 8, false, Muted),
        ];
        for (name, events, start, end, always, want) in table {
            assert_eq!(state(events, start * S, end * S, always), want, "{name}");
        }
    }

    #[test]
    fn an_extension_event_counts_like_a_manual_one() {
        let events = [Event { at_ms: 0, muted: false, source: Source::Extension }];
        assert_eq!(state(&events, 1000, 2000, false), State::Unmuted);
        assert_eq!(mic_now(&events, false), Mic { live: true, how: "mirrored" });
    }

    #[test]
    fn the_indicator_says_what_the_last_toggle_or_the_policy_says() {
        let table = [
            ("nothing yet", vec![], false, Mic { live: false, how: "manual" }),
            ("nothing yet, always record", vec![], true, Mic { live: true, how: "always on" }),
            ("unmuted", vec![ev(1, false)], false, Mic { live: true, how: "manual" }),
            ("muted", vec![ev(1, false), ev(2, true)], false, Mic { live: false, how: "manual" }),
            ("muted despite always record", vec![ev(1, true)], true, Mic { live: false, how: "manual" }),
        ];
        for (name, events, always, want) in table {
            assert_eq!(mic_now(&events, always), want, "{name}");
        }
    }

    #[test]
    fn stored_state_round_trips_and_a_bad_value_stays_out() {
        for s in [State::Unmuted, State::Muted, State::Unknown] {
            assert_eq!(State::parse(s.as_str()), s);
        }
        assert_eq!(State::parse("garbage"), State::Unknown);
        assert_eq!(Source::parse(Source::Extension.as_str()), Source::Extension);
    }

    #[test]
    fn always_record_is_per_app_and_never_for_a_mic_only_meeting() {
        let db = settings::open(std::path::Path::new(":memory:")).unwrap();
        store::init(&db).unwrap();
        let (zoom, meet, mic_only) = (store::create(&db, 1).unwrap(), store::create(&db, 2).unwrap(), store::create(&db, 3).unwrap());
        store::set_source(&db, zoom, Some("zoom.us"), None).unwrap();
        store::set_source(&db, meet, Some("Google Chrome"), None).unwrap();
        let all = |db: &Connection| [zoom, meet, mic_only].map(|id| always_record(db, id));
        assert_eq!(all(&db), [false, false, false]);
        settings::set(&db, ALWAYS_KEY, r#"["zoom.us"]"#).unwrap();
        assert_eq!(all(&db), [true, false, false]);
        settings::set(&db, ALWAYS_KEY, "not json").unwrap();
        assert_eq!(all(&db), [false, false, false]);
    }
}
