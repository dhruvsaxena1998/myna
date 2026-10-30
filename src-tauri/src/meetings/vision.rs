//! Experimental: screenshots going to the AI provider. Two uses, each off by default and each needing the model's
//! "supports images" box: a relevance check while recording, and kept frames attached to summaries and chat.
//! What may be sent is decided here and nowhere else. Window video is never sent, only these JPEG frames.
use super::{
    mute::{self, State},
    shots::Shot,
    store::{self, Segment},
    transcript::clock,
};
use crate::ai::{self, Config, Image, Message};
use rusqlite::Connection;
use std::{path::Path, process::Command, time::Duration};

/// Most frames attached to one request.
pub const MAX_IMAGES: usize = 20;
/// Longest side of an attached frame, in px.
const SIDE: &str = "768";
/// A relevance check is a short answer; past this the frame is kept and the capture moves on.
const RELEVANCE_WITHIN: Duration = Duration::from_secs(20);

const RELEVANCE_PROMPT: &str = "You decide which screenshots of a meeting window are worth keeping. The first image is the last one kept and the second is the new one. Answer only yes if the new one shows different content, such as a new slide, document, chart, code or shared screen. Answer only no if it is the same content, a small change such as a cursor or a notification, or has nothing to read.";

/// The frames that may go to a provider with a summary or chat request, at most `MAX_IMAGES`, spread over the meeting.
/// A frame taken while a line that is not in the transcript was being spoken (`Segment::shown`) is left out, so
/// muted or hidden time never sends a picture. A line the triage pass pulled in (`auto`) blocks its frames too: the user
/// has not agreed to it yet, so only their own inclusion lets a picture from that moment out.
// shortcut: a muted stretch with nobody speaking has no hidden segment, so its frames still go; there is no speech
// there to protect. Tighten to the mute timeline if a frame itself should be held back during mute.
pub fn allowed(shots: &[Shot], segments: &[Segment]) -> Vec<Shot> {
    let kept: Vec<&Shot> = shots.iter().filter(|s| !segments.iter().any(|g| (!g.shown() || g.auto) && g.start_ms <= s.at_ms && s.at_ms <= g.end_ms)).collect();
    let n = kept.len().min(MAX_IMAGES);
    (0..n).map(|i| kept[i * kept.len() / n].clone()).collect()
}

/// The frames to attach for this meeting: none unless the "shots" toggle and the model's images box are both on.
pub fn chosen(db: &Connection, meeting: i64, frames: &Path, segments: &[Segment], model: &str) -> Vec<Shot> {
    if !ai::images_on(db, "shots", model) {
        return vec![];
    }
    super::shots::list(db, meeting, frames).map(|(shots, _)| allowed(&shots, segments)).unwrap_or_default()
}

/// Reads and downscales the frames. A frame that cannot be read is skipped.
pub fn load(shots: &[Shot]) -> Vec<Image> {
    shots
        .iter()
        .filter_map(|s| Some(Image { caption: format!("Screenshot at {}", clock(s.at_ms)), jpeg: scaled(Path::new(&s.path))? }))
        .collect()
}

// shortcut: shells out to sips, which ships with macOS, rather than decoding the JPEG here. A frame sips cannot
// shrink goes at its stored size (1280 px wide). Use ImageIO in screen.rs if sips goes away.
fn scaled(path: &Path) -> Option<Vec<u8>> {
    let tmp = std::env::temp_dir().join(format!("myna-scaled-{}-{}.jpg", std::process::id(), path.file_stem()?.to_string_lossy()));
    let ok = Command::new("sips").args(["-Z", SIDE]).arg(path).arg("--out").arg(&tmp).output().is_ok_and(|o| o.status.success());
    let bytes = if ok { std::fs::read(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    bytes.or_else(|| std::fs::read(path).ok())
}

/// Whether the live mic is known to be unmuted at `at_ms`: the only time a new frame may be sent for a relevance
/// check, since there is no transcript yet to say the moment is shown. Unknown or muted means no request.
pub fn live(db: &Connection, meeting: i64, at_ms: u64) -> bool {
    let events = store::mute_events(db, meeting).unwrap_or_default();
    mute::state(&events, at_ms, at_ms, mute::always_record(db, meeting)) == State::Unmuted
}

/// "yes" keeps, "no" drops. Anything else keeps: an unclear answer must not lose a slide.
fn verdict(text: &str) -> bool {
    text.split(|c: char| !c.is_alphabetic()).find(|w| !w.is_empty()).map_or(true, |w| !w.eq_ignore_ascii_case("no"))
}

/// Asks the provider whether `new` adds anything over `last_kept`. A failed or slow request keeps the frame.
pub fn relevant(cfg: &Config, last_kept: &[u8], new: &[u8]) -> bool {
    let images = [
        Image { caption: "Last kept screenshot".into(), jpeg: last_kept.to_vec() },
        Image { caption: "New screenshot".into(), jpeg: new.to_vec() },
    ];
    let messages = [Message { role: "system", content: RELEVANCE_PROMPT.into() }, Message { role: "user", content: "Keep the new screenshot?".into() }];
    cfg.complete_images(&messages, &images, RELEVANCE_WITHIN).map_or(true, |r| verdict(&r.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ai::stub, settings};
    use serde_json::Value;

    fn seg(start_ms: u64, end_ms: u64, mute: State, included: bool) -> Segment {
        Segment { start_ms, end_ms, mute, included, text: "x".into(), speaker: store::ME.into(), ..Default::default() }
    }

    fn shot(at_ms: u64) -> Shot {
        Shot { at_ms, path: format!("/shots/{at_ms}.jpg") }
    }

    fn at(shots: &[Shot]) -> Vec<u64> {
        shots.iter().map(|s| s.at_ms).collect()
    }

    fn config(base: &str) -> Config {
        Config { preset: &ai::PRESETS[3], base_url: base.into(), model: "vis".into(), key: None, context: None }
    }

    #[test]
    fn frames_in_muted_or_unknown_lines_are_never_allowed_but_included_and_live_ones_are() {
        let segments = [
            seg(0, 10_000, State::Unmuted, false),
            seg(10_000, 20_000, State::Muted, false),
            seg(20_000, 30_000, State::Unknown, true), // pulled in by the user
            seg(30_000, 40_000, State::Unknown, false),
        ];
        let shots: Vec<Shot> = [5_000, 15_000, 25_000, 35_000, 45_000].map(shot).into();
        // 45 s is past every line: nobody was speaking, so it is not a hidden span.
        assert_eq!(at(&allowed(&shots, &segments)), [5_000, 25_000, 45_000]);
        // The segment boundary belongs to the hidden line.
        assert_eq!(at(&allowed(&[shot(10_000), shot(40_000)], &segments)), Vec::<u64>::new());
    }

    #[test]
    fn frames_in_auto_included_lines_are_held_back_until_the_user_confirms_them() {
        let auto = Segment { auto: true, ..seg(10_000, 20_000, State::Muted, true) };
        let user = seg(30_000, 40_000, State::Muted, true);
        assert_eq!(at(&allowed(&[shot(5_000), shot(15_000), shot(35_000)], &[auto, user])), [5_000, 35_000]);
    }

    #[test]
    fn a_hidden_line_overlapping_a_live_one_blocks_the_frame() {
        let segments = [seg(0, 30_000, State::Unmuted, false), seg(10_000, 20_000, State::Muted, false)];
        assert_eq!(at(&allowed(&[shot(5_000), shot(15_000), shot(25_000)], &segments)), [5_000, 25_000]);
    }

    #[test]
    fn at_most_twenty_frames_spread_over_the_meeting() {
        let shots: Vec<Shot> = (0..100).map(|i| shot(i * 1000)).collect();
        let got = at(&allowed(&shots, &[]));
        assert_eq!(got.len(), MAX_IMAGES);
        assert_eq!((got[0], got[19]), (0, 95_000)); // first to near the last, not just the first twenty
        assert!(got.windows(2).all(|w| w[0] < w[1]));
        let few: Vec<Shot> = (0..7).map(|i| shot(i * 1000)).collect();
        assert_eq!(allowed(&few, &[]).len(), 7);
        assert!(allowed(&[], &[]).is_empty());
    }

    #[test]
    fn verdict_keeps_on_yes_and_anything_unclear_and_drops_only_on_no() {
        for (said, keep) in [("yes", true), ("Yes.", true), (" NO", false), ("no, same slide", false), ("No.", false), ("", true), ("maybe", true), ("Not sure", true)] {
            assert_eq!(verdict(said), keep, "{said:?}");
        }
    }

    #[test]
    fn nothing_is_chosen_until_the_toggle_and_the_models_box_are_both_on() {
        let db = stub::db();
        super::super::shots::init(&db).unwrap();
        db.execute("INSERT INTO screenshots (meeting_id, at_ms, file) VALUES (1, 0, '0.jpg')", []).unwrap();
        let pick = |db: &Connection| chosen(db, 1, Path::new("/s/1"), &[], "vis");
        assert!(pick(&db).is_empty());
        settings::set(&db, "ai_shots", "true").unwrap();
        assert!(pick(&db).is_empty(), "the model has not been marked as accepting images");
        settings::set(&db, "ai_img:vis", "true").unwrap();
        assert_eq!(at(&pick(&db)), [0]);
        settings::set(&db, "ai_shots", "false").unwrap();
        assert!(pick(&db).is_empty());
        settings::set(&db, "ai_shots", "true").unwrap();
        assert!(chosen(&db, 1, Path::new("/s/1"), &[], "other").is_empty(), "the box is per model");
    }

    #[test]
    fn the_relevance_request_carries_both_frames_and_a_yes_or_no_decides() {
        let (base, seen) = stub::serve(vec![stub::ok("yes"), stub::ok("No."), (500, r#"{"error":{"message":"down"}}"#.into())]);
        let cfg = config(&base);
        assert!(relevant(&cfg, b"old", b"new"));
        let body: Value = serde_json::from_str(&seen.recv().unwrap().body).unwrap();
        let parts = body["messages"][1]["content"].as_array().unwrap();
        let urls: Vec<_> = parts.iter().filter_map(|p| p["image_url"]["url"].as_str()).collect();
        assert_eq!(urls, ["data:image/jpeg;base64,b2xk", "data:image/jpeg;base64,bmV3"]);
        assert!(!relevant(&cfg, b"old", b"new"));
        assert!(relevant(&cfg, b"old", b"new"), "a provider error keeps the frame");
    }

    #[test]
    fn a_relevance_frame_is_only_sent_while_the_mic_is_known_to_be_live() {
        use super::super::mute::{Event, Source};
        let db = stub::db();
        store::init(&db).unwrap();
        let id = store::create(&db, 1).unwrap();
        assert!(!live(&db, id, 5_000), "no mute source: unknown, so nothing is sent");
        let ev = |at_ms, muted| Event { at_ms, muted, source: Source::Manual };
        for e in [ev(0, false), ev(10_000, true), ev(20_000, false)] {
            store::add_mute_event(&db, id, &e).unwrap();
        }
        assert_eq!([5_000, 15_000, 25_000].map(|t| live(&db, id, t)), [true, false, true]);
    }
}
