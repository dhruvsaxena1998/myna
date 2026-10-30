//! Speaker names from the call's own captions. The companion extension reports who the captions show as speaking
//! ("name X spoke at time T", stamped on the recording's clock by the relay); after diarization each speaker is
//! suggested the caption name that overlaps their lines most. Names are only ever suggestions, under ticket 09's
//! rules: they stay in this database and never reach an AI provider.
use super::speakers::Speaker;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub const MAX_NAME: usize = 80;
/// The extension reads the captions every 2 s, so a name seen at T was speaking in the 2 s before it.
const BEAT_MS: u64 = 2000;
/// Less shared time than this is as likely a caption lingering past a turn as a match.
const MIN_OVERLAP_MS: u64 = 1000;

#[derive(Debug, PartialEq, Clone)]
pub struct Caption {
    pub at_ms: u64,
    /// None: the extension was there but the captions were off or unreadable.
    pub name: Option<String>,
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS captions (meeting_id INTEGER NOT NULL, at_ms INTEGER NOT NULL, name TEXT);
        CREATE INDEX IF NOT EXISTS captions_meeting ON captions (meeting_id, at_ms);",
    )
}

/// A name worth keeping: trimmed, not empty, not long, no control characters. The relay refuses anything else.
pub fn valid(name: &str) -> Option<&str> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control)).then_some(name)
}

/// Records a report. "Unreadable" is kept once per meeting: it only has to say the extension was there.
pub fn add(db: &Connection, meeting: i64, at_ms: u64, name: Option<&str>) -> rusqlite::Result<()> {
    match name {
        Some(n) => db.execute("INSERT INTO captions (meeting_id, at_ms, name) VALUES (?1, ?2, ?3)", params![meeting, at_ms, n]),
        None => db.execute(
            "INSERT INTO captions (meeting_id, at_ms, name) SELECT ?1, ?2, NULL WHERE NOT EXISTS (SELECT 1 FROM captions WHERE meeting_id = ?1)",
            params![meeting, at_ms],
        ),
    }
    .map(drop)
}

pub fn list(db: &Connection, meeting: i64) -> rusqlite::Result<Vec<Caption>> {
    db.prepare("SELECT at_ms, name FROM captions WHERE meeting_id = ?1 ORDER BY at_ms, rowid")?
        .query_map([meeting], |r| Ok(Caption { at_ms: r.get(0)?, name: r.get(1)? }))?
        .collect()
}

/// The extension reported, and never once with a name: captions were off or its selector broke.
pub fn unavailable(events: &[Caption]) -> bool {
    !events.is_empty() && events.iter().all(|e| e.name.is_none())
}

/// Which caption name goes with which speaker. `lines` are (start_ms, end_ms) of the system track, `clusters`
/// parallel to them. A name goes to the speaker whose lines share the most time with the 2 s before each report of
/// it, and each name and speaker is used once, the largest overlap first.
pub fn align(lines: &[(u64, u64)], clusters: &[Option<u32>], events: &[Caption]) -> Vec<(u32, String)> {
    // shortcut: "You" is Meet's English label for the user, whose voice is on the mic track; other locales' labels
    // just earn no overlap on the system track. Skip it by name only if a locale ever misnames someone.
    let (mut shown, mut shared): (HashMap<String, &str>, HashMap<(u32, String), u64>) = Default::default();
    for (at, name) in events.iter().filter_map(|e| Some((e.at_ms, e.name.as_deref()?))).filter(|(_, n)| !n.eq_ignore_ascii_case("you")) {
        let key = name.to_lowercase();
        shown.entry(key.clone()).or_insert(name);
        for (&(start, end), cluster) in lines.iter().zip(clusters) {
            let Some(c) = cluster else { continue };
            let both = end.min(at).saturating_sub(start.max(at.saturating_sub(BEAT_MS)));
            if both > 0 {
                *shared.entry((*c, key.clone())).or_default() += both;
            }
        }
    }
    let mut pairs: Vec<_> = shared.into_iter().filter(|(_, ms)| *ms >= MIN_OVERLAP_MS).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let (mut out, mut taken): (Vec<(u32, String)>, Vec<String>) = Default::default();
    for ((cluster, key), _) in pairs {
        if !taken.contains(&key) && !out.iter().any(|(c, _)| *c == cluster) {
            out.push((cluster, shown[&key].to_string()));
            taken.push(key);
        }
    }
    out.sort();
    out
}

/// Suggests each speaker its caption name, in place of a remembered-voice suggestion, and drops a voice
/// suggestion that a caption gave to someone else.
pub fn suggest(speakers: &mut [Speaker], lines: &[(u64, u64)], clusters: &[Option<u32>], events: &[Caption]) {
    let named = align(lines, clusters, events);
    for s in speakers.iter_mut() {
        if let Some((_, name)) = named.iter().find(|(c, _)| *c == s.cluster) {
            (s.suggested, s.from_captions) = (Some(name.clone()), true);
        } else if s.suggested.as_ref().is_some_and(|v| named.iter().any(|(_, n)| n.eq_ignore_ascii_case(v))) {
            s.suggested = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(at_ms: u64, name: &str) -> Caption {
        Caption { at_ms, name: Some(name.into()) }
    }

    /// Two speakers: 1 talks 0-10 s and 20-30 s, 2 talks 10-20 s.
    const LINES: [(u64, u64); 3] = [(0, 10_000), (10_000, 20_000), (20_000, 30_000)];
    const CLUSTERS: [Option<u32>; 3] = [Some(1), Some(2), Some(1)];

    /// A report every 2 s naming `who` from `from` to `to`.
    fn run(who: &str, from: u64, to: u64) -> Vec<Caption> {
        (from / 2000..=to / 2000).map(|i| at(i * 2000, who)).collect()
    }

    #[test]
    fn each_speaker_gets_the_caption_name_that_overlaps_their_lines_most() {
        let table: [(&str, Vec<Caption>, Vec<(u32, &str)>); 8] = [
            ("two people", [run("Lena", 2000, 10_000), run("Tom", 12_000, 20_000), run("Lena", 22_000, 30_000)].concat(), vec![(1, "Lena"), (2, "Tom")]),
            ("no captions at all", vec![], vec![]),
            ("a name that only ever lingers past a turn", [run("Lena", 2000, 10_000), run("Lena", 12_000, 12_000), run("Tom", 14_000, 20_000)].concat(), vec![(1, "Lena"), (2, "Tom")]),
            ("too little shared time", vec![at(500, "Lena")], vec![]),
            ("one name for two speakers goes to the larger overlap", [run("Lena", 2000, 10_000), run("Lena", 12_000, 14_000)].concat(), vec![(1, "Lena")]),
            ("the same person in different case", [run("Lena", 2000, 6000), run("LENA", 8000, 10_000)].concat(), vec![(1, "Lena")]),
            ("the user's own name never names a speaker", run("You", 2000, 10_000), vec![]),
            ("an unreadable report names nobody", vec![Caption { at_ms: 4000, name: None }], vec![]),
        ];
        for (name, events, want) in table {
            let want: Vec<_> = want.into_iter().map(|(c, n)| (c, n.to_string())).collect();
            assert_eq!(align(&LINES, &CLUSTERS, &events), want, "{name}");
        }
    }

    #[test]
    fn a_name_the_diarizer_could_not_place_names_nobody() {
        // Lines with no cluster (the mic, or unseparated speech) are never matched.
        assert!(align(&[(0, 10_000)], &[None], &run("Lena", 2000, 10_000)).is_empty());
        assert!(align(&[], &[], &run("Lena", 2000, 10_000)).is_empty());
    }

    fn speaker(cluster: u32, voice: Option<&str>) -> Speaker {
        Speaker { cluster, name: None, suggested: voice.map(Into::into), from_captions: false, print: None }
    }

    #[test]
    fn a_caption_name_is_a_suggestion_that_replaces_a_voice_one_and_clears_a_duplicate() {
        let events = [run("Lena", 2000, 10_000), run("Tom", 12_000, 20_000)].concat();
        let mut people = [speaker(1, Some("Maria")), speaker(2, None), speaker(3, Some("Lena"))];
        suggest(&mut people, &LINES, &CLUSTERS, &events);
        let got: Vec<_> = people.iter().map(|p| (p.name.as_deref(), p.suggested.as_deref(), p.from_captions)).collect();
        assert_eq!(got, [(None, Some("Lena"), true), (None, Some("Tom"), true), (None, None, false)]);
        // A voice suggestion nobody contradicts stays.
        let mut people = [speaker(1, Some("Maria"))];
        suggest(&mut people, &LINES, &CLUSTERS, &[]);
        assert_eq!((people[0].suggested.as_deref(), people[0].from_captions), (Some("Maria"), false));
    }

    #[test]
    fn names_are_checked_before_they_are_kept() {
        assert_eq!(valid("  Lena Ström "), Some("Lena Ström"));
        for bad in ["", "   ", "a\nb", "a\u{7}b", &"x".repeat(MAX_NAME + 1)] {
            assert_eq!(valid(bad), None, "{bad:?}");
        }
        assert!(valid(&"x".repeat(MAX_NAME)).is_some());
    }

    #[test]
    fn reports_are_stored_per_meeting_and_unreadable_is_kept_once() {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        for (m, t, n) in [(1, 100, None), (1, 2100, None), (1, 4100, Some("Lena")), (1, 6100, None), (2, 50, Some("Tom"))] {
            add(&db, m, t, n).unwrap();
        }
        assert_eq!(list(&db, 1).unwrap(), [Caption { at_ms: 100, name: None }, at(4100, "Lena")]);
        assert_eq!(list(&db, 2).unwrap(), [at(50, "Tom")]);
        assert!(list(&db, 3).unwrap().is_empty());
    }

    #[test]
    fn captions_are_unavailable_only_when_the_extension_reported_and_never_had_a_name() {
        assert!(!unavailable(&[]), "no extension, nothing to say");
        assert!(unavailable(&[Caption { at_ms: 0, name: None }]));
        assert!(!unavailable(&[Caption { at_ms: 0, name: None }, at(2000, "Lena")]));
    }
}

/// Plants a caption-suggested speaker and its caption rows, so each AI path's test can prove the name stays here.
#[cfg(test)]
pub fn plant(db: &Connection, meeting: i64, name: &str) {
    use super::speakers;
    init(db).unwrap();
    speakers::init(db).unwrap();
    add(db, meeting, 2000, Some(name)).unwrap();
    speakers::replace(db, meeting, &[Speaker { cluster: 1, name: None, suggested: Some(name.into()), from_captions: true, print: None }]).unwrap();
}
