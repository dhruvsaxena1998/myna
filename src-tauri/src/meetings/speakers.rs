//! Who spoke in a meeting: a row per speaker on the system track, renaming and merging them, the voices the
//! user asked Myna to remember, and which of those a later meeting's speakers sound like. Names and voice
//! prints stay in this database. The transcript an AI provider gets says "Speaker N", never a name.
use super::{
    store::{self, Segment, THEM},
    voiceprint::{self, Print},
};
use crate::Db;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::cmp::Reverse;

/// Two voices closer than this are, for a suggestion, the same person. Set on `say` voices (see the ignored test
/// below): with a minute of speech a side, the same voice read different passages 1.5 to 4.3 apart and different
/// voices at least 9.2 apart. Real people, rooms and codecs are noisier than that.
const MATCH: f32 = 6.0;
const CLIPS: usize = 3;
/// A clip plays at most this long, so a long monologue doesn't turn "who is this?" into a minute of listening.
pub const CLIP_MAX_MS: u64 = 15_000;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Speaker {
    /// Numbered from 1 within the meeting, in order of first appearance; `Segment::cluster` points here.
    pub cluster: u32,
    /// What the user called them; None reads as "Speaker N".
    pub name: Option<String>,
    /// A remembered voice this one sounds like, until the user confirms or dismisses it.
    pub suggested: Option<String>,
    #[serde(skip)]
    pub print: Option<Print>,
}

pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS speakers (
            meeting_id INTEGER NOT NULL,
            cluster INTEGER NOT NULL,
            name TEXT,
            suggested TEXT,
            print BLOB,
            PRIMARY KEY (meeting_id, cluster)
        );
        CREATE TABLE IF NOT EXISTS voice_profiles (name TEXT PRIMARY KEY COLLATE NOCASE, print BLOB NOT NULL);",
    )
}

/// The speakers of a meeting, in cluster order.
pub fn list(db: &Connection, meeting: i64) -> rusqlite::Result<Vec<Speaker>> {
    db.prepare("SELECT cluster, name, suggested, print FROM speakers WHERE meeting_id = ?1 ORDER BY cluster")?
        .query_map([meeting], |r| {
            Ok(Speaker {
                cluster: r.get(0)?,
                name: r.get(1)?,
                suggested: r.get(2)?,
                print: r.get::<_, Option<Vec<u8>>>(3)?.and_then(|b| voiceprint::from_bytes(&b)),
            })
        })?
        .collect()
}

/// Stores a meeting's speakers in place of any from an earlier run.
pub fn replace(db: &Connection, meeting: i64, speakers: &[Speaker]) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("DELETE FROM speakers WHERE meeting_id = ?1", [meeting])?;
    for s in speakers {
        tx.execute(
            "INSERT INTO speakers (meeting_id, cluster, name, suggested, print) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![meeting, s.cluster, s.name, s.suggested, s.print.as_ref().map(voiceprint::to_bytes)],
        )?;
    }
    tx.commit()
}

/// The voices the user asked Myna to remember.
pub fn profiles(db: &Connection) -> Vec<(String, Print)> {
    let read = || -> rusqlite::Result<_> {
        db.prepare("SELECT name, print FROM voice_profiles ORDER BY name")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()
    };
    read().unwrap_or_default().into_iter().filter_map(|(n, b)| Some((n, voiceprint::from_bytes(&b)?))).collect()
}

/// For each print, the remembered voice it is nearest to, if near enough. A remembered voice goes to one
/// speaker only, the nearest.
pub fn suggest(prints: &[Option<Print>], profiles: &[(String, Print)]) -> Vec<Option<String>> {
    let mut pairs: Vec<(f32, usize, usize)> = vec![];
    for (i, p) in prints.iter().enumerate().filter_map(|(i, p)| Some((i, p.as_ref()?))) {
        for (j, (_, q)) in profiles.iter().enumerate() {
            let d = voiceprint::distance(p, q);
            if d <= MATCH {
                pairs.push((d, i, j));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let (mut out, mut taken) = (vec![None; prints.len()], vec![false; profiles.len()]);
    for (_, i, j) in pairs {
        if out[i].is_none() && !taken[j] {
            out[i] = Some(profiles[j].0.clone());
            taken[j] = true;
        }
    }
    out
}

/// A speaker per cluster in `clusters` (one entry per line, parallel to `lines`), each with the print of
/// its lines' audio and a suggested name when it sounds like a remembered voice. `lines` are (start_ms,
/// end_ms) into `audio`, mono 16 kHz.
pub fn build(audio: &[f32], lines: &[(u64, u64)], clusters: &[Option<u32>], profiles: &[(String, Print)]) -> Vec<Speaker> {
    let count = clusters.iter().flatten().max().copied().unwrap_or(0);
    let at = |ms: u64| (ms as usize * 16).min(audio.len());
    let prints: Vec<Option<Print>> = (1..=count)
        .map(|c| {
            let heard: Vec<f32> =
                lines.iter().zip(clusters).filter(|(_, k)| **k == Some(c)).flat_map(|(&(s, e), _)| audio[at(s)..at(e).max(at(s))].iter().copied()).collect();
            voiceprint::compute(&heard)
        })
        .collect();
    let suggested = suggest(&prints, profiles);
    prints.into_iter().zip(suggested).zip(1..).map(|((print, suggested), cluster)| Speaker { cluster, name: None, suggested, print }).collect()
}

fn speaker_of(db: &Connection, meeting: i64, cluster: u32) -> Result<Speaker, String> {
    list(db, meeting).map_err(|e| e.to_string())?.into_iter().find(|s| s.cluster == cluster).ok_or_else(|| "That speaker is gone.".into())
}

/// Names a speaker, which relabels every segment of theirs in the meeting at once, and settles any suggestion.
/// With `remember`, their voice joins the remembered ones under that name; says whether it did, since a speaker
/// who said too little has no voice print.
pub fn rename(db: &Connection, meeting: i64, cluster: u32, name: &str, remember: bool) -> Result<bool, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the speaker a name.".into());
    }
    let speaker = speaker_of(db, meeting, cluster)?;
    db.execute("UPDATE speakers SET name = ?3, suggested = NULL WHERE meeting_id = ?1 AND cluster = ?2", params![meeting, cluster, name]).map_err(|e| e.to_string())?;
    let Some(print) = speaker.print.filter(|_| remember) else { return Ok(false) };
    // The same name again adds to what is remembered, so a voice sharpens over meetings.
    let print = match profiles(db).into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
        Some((_, old)) => voiceprint::blend(&old, &print),
        None => print,
    };
    db.execute(
        "INSERT INTO voice_profiles (name, print) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET print = excluded.print",
        params![name, voiceprint::to_bytes(&print)],
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Drops a suggested name; the speaker goes back to "Speaker N".
pub fn dismiss(db: &Connection, meeting: i64, cluster: u32) -> Result<(), String> {
    db.execute("UPDATE speakers SET suggested = NULL WHERE meeting_id = ?1 AND cluster = ?2", params![meeting, cluster]).map_err(|e| e.to_string())?;
    Ok(())
}

/// Folds `from` into `into`: their segments become `into`'s, and `into` keeps the name (else takes `from`'s)
/// and the voices of both.
pub fn merge(db: &Connection, meeting: i64, from: u32, into: u32) -> Result<(), String> {
    if from == into {
        return Err("Pick a different speaker to merge into.".into());
    }
    let (a, b) = (speaker_of(db, meeting, from)?, speaker_of(db, meeting, into)?);
    let name = b.name.or(a.name);
    let suggested = if name.is_some() { None } else { b.suggested.or(a.suggested) };
    let print = match (&b.print, &a.print) {
        (Some(x), Some(y)) => Some(voiceprint::blend(x, y)),
        (x, y) => x.clone().or_else(|| y.clone()),
    };
    let e = |e: rusqlite::Error| e.to_string();
    let tx = db.unchecked_transaction().map_err(e)?;
    tx.execute("UPDATE segments SET cluster = ?3 WHERE meeting_id = ?1 AND cluster = ?2", params![meeting, from, into]).map_err(e)?;
    tx.execute(
        "UPDATE speakers SET name = ?3, suggested = ?4, print = ?5 WHERE meeting_id = ?1 AND cluster = ?2",
        params![meeting, into, name, suggested, print.as_ref().map(voiceprint::to_bytes)],
    )
    .map_err(e)?;
    tx.execute("DELETE FROM speakers WHERE meeting_id = ?1 AND cluster = ?2", params![meeting, from]).map_err(e)?;
    tx.commit().map_err(e)
}

/// A stretch of a speaker to listen to: the segment to play, when it starts, and how many seconds play.
#[derive(Serialize, PartialEq, Debug)]
pub struct Clip {
    pub id: i64,
    pub start_ms: u64,
    pub secs: u64,
}

/// The speaker's three longest segments, longest first.
pub fn longest(segments: &[Segment], cluster: u32) -> Vec<Clip> {
    let len = |s: &Segment| s.end_ms.saturating_sub(s.start_ms);
    let mut mine: Vec<&Segment> = segments.iter().filter(|s| s.speaker == THEM && s.cluster == Some(cluster) && s.shown()).collect();
    mine.sort_by_key(|s| (Reverse(len(s)), s.start_ms));
    mine.into_iter().take(CLIPS).map(|s| Clip { id: s.id, start_ms: s.start_ms, secs: len(s).min(CLIP_MAX_MS).div_ceil(1000) }).collect()
}

#[tauri::command]
pub fn speaker_rename(db: tauri::State<Db>, meeting: i64, cluster: u32, name: String, remember: bool) -> Result<bool, String> {
    rename(&db.0.lock().unwrap(), meeting, cluster, &name, remember)
}

#[tauri::command]
pub fn speaker_dismiss(db: tauri::State<Db>, meeting: i64, cluster: u32) -> Result<(), String> {
    dismiss(&db.0.lock().unwrap(), meeting, cluster)
}

#[tauri::command]
pub fn speaker_merge(db: tauri::State<Db>, meeting: i64, from: u32, into: u32) -> Result<(), String> {
    merge(&db.0.lock().unwrap(), meeting, from, into)
}

#[tauri::command]
pub fn speaker_clips(db: tauri::State<Db>, meeting: i64, cluster: u32) -> Result<Vec<Clip>, String> {
    Ok(longest(&store::segments(&db.0.lock().unwrap(), meeting).map_err(|e| e.to_string())?, cluster))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meetings::voiceprint::DIM;

    fn print(x: f32) -> Print {
        Print { frames: 1000, mean: [x; DIM], var: [1.0; DIM] }
    }

    /// Distance between `print(a)` and `print(b)` is |a - b| * sqrt(DIM) ≈ 3.46 |a - b|.
    const STEP: f32 = 0.1;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        store::init(&c).unwrap();
        init(&c).unwrap();
        c
    }

    fn them(start_ms: u64, end_ms: u64, cluster: u32) -> Segment {
        Segment { speaker: THEM.into(), start_ms, end_ms, text: format!("line at {start_ms}"), cluster: Some(cluster), ..Default::default() }
    }

    fn speaker(cluster: u32, x: f32) -> Speaker {
        Speaker { cluster, name: None, suggested: None, print: Some(print(x)) }
    }

    /// A finished meeting with three speakers: 1 and 2 talk twice each, 3 once.
    fn meeting(c: &Connection) -> i64 {
        let id = store::create(c, 1).unwrap();
        store::finish(c, id, 60_000, &[them(0, 5000, 1), them(6000, 9000, 2), them(10_000, 20_000, 1), them(21_000, 22_000, 3), them(23_000, 31_000, 2)]).unwrap();
        replace(c, id, &[speaker(1, 0.0), speaker(2, 5.0), speaker(3, 9.0)]).unwrap();
        id
    }

    fn clusters(c: &Connection, id: i64) -> Vec<Option<u32>> {
        store::segments(c, id).unwrap().iter().map(|s| s.cluster).collect()
    }

    #[test]
    fn a_remembered_voice_is_suggested_to_the_nearest_speaker_only_when_close_enough() {
        let (lena, tom) = (("Lena".to_string(), print(0.0)), ("Tom".to_string(), print(5.0)));
        let table: [(&str, Vec<Option<Print>>, Vec<(String, Print)>, Vec<Option<&str>>); 7] = [
            ("close", vec![Some(print(STEP))], vec![lena.clone()], vec![Some("Lena")]),
            ("too far", vec![Some(print(3.0))], vec![lena.clone()], vec![None]),
            ("no print, no suggestion", vec![None], vec![lena.clone()], vec![None]),
            ("nothing remembered", vec![Some(print(0.0))], vec![], vec![None]),
            ("one voice goes to the nearer speaker", vec![Some(print(0.2)), Some(print(STEP))], vec![lena.clone()], vec![None, Some("Lena")]),
            ("each speaker gets their own", vec![Some(print(5.1)), Some(print(0.1))], vec![lena.clone(), tom.clone()], vec![Some("Tom"), Some("Lena")]),
            ("the far speaker is left alone", vec![Some(print(0.1)), Some(print(9.0))], vec![lena, tom], vec![Some("Lena"), None]),
        ];
        for (name, prints, profiles, want) in table {
            let got = suggest(&prints, &profiles);
            assert_eq!(got.iter().map(|s| s.as_deref()).collect::<Vec<_>>(), want, "{name}");
        }
    }

    #[test]
    fn the_three_longest_clips_of_one_speaker_longest_first_and_capped() {
        let mut hidden = them(100_000, 160_000, 1);
        hidden.speaker = "me".into();
        let mut all = vec![them(0, 5000, 1), them(6000, 7000, 1), them(10_000, 20_000, 1), them(21_000, 28_000, 1), them(30_000, 35_000, 1), them(40_000, 48_000, 2), hidden];
        for (i, s) in all.iter_mut().enumerate() {
            s.id = i as i64 + 1;
        }
        let clip = |id, start_ms, secs| Clip { id, start_ms, secs };
        // 10 s, 7 s, then the earlier of the two 5 s lines; the other speaker's and the mic's are not offered.
        assert_eq!(longest(&all, 1), [clip(3, 10_000, 10), clip(4, 21_000, 7), clip(1, 0, 5)]);
        assert_eq!(longest(&all, 2), [clip(6, 40_000, 8)]);
        assert!(longest(&all, 9).is_empty());
        // A long monologue is the longest clip but plays a capped stretch.
        all.push(them(200_000, 290_000, 1));
        all.last_mut().unwrap().id = 99;
        assert_eq!(longest(&all, 1)[0], clip(99, 200_000, 15));
    }

    #[test]
    fn renaming_one_speaker_relabels_all_their_segments_and_nobody_elses() {
        let c = db();
        let id = meeting(&c);
        let other = meeting(&c);
        assert_eq!(rename(&c, id, 1, "  Priya Nair ", false).unwrap(), false);
        let named: Vec<_> = list(&c, id).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(named, [Some("Priya Nair".into()), None, None]);
        // The segments still point at the speaker; the name is read through it, so all of them follow.
        assert_eq!(clusters(&c, id), [Some(1), Some(2), Some(1), Some(3), Some(2)]);
        assert!(list(&c, other).unwrap().iter().all(|s| s.name.is_none()), "another meeting is untouched");
        assert!(rename(&c, id, 1, "   ", false).is_err());
        assert!(rename(&c, id, 7, "Nobody", false).is_err());
    }

    #[test]
    fn remembering_a_voice_stores_it_by_name_and_the_same_name_adds_to_it() {
        let c = db();
        let id = meeting(&c);
        assert!(rename(&c, id, 2, "Lena", true).unwrap());
        assert_eq!(profiles(&c), [("Lena".to_string(), print(5.0))]);
        assert!(rename(&c, id, 3, "lena", true).unwrap(), "names match ignoring case");
        let all = profiles(&c);
        assert_eq!((all.len(), all[0].0.as_str(), all[0].1.frames, all[0].1.mean[0]), (1, "Lena", 2000, 7.0));
        // Not remembered unless asked, and a speaker with no print has nothing to remember.
        assert!(!rename(&c, id, 1, "Tom", false).unwrap());
        replace(&c, id, &[Speaker { print: None, ..speaker(1, 0.0) }]).unwrap();
        assert!(!rename(&c, id, 1, "Tom", true).unwrap());
        assert_eq!(profiles(&c).len(), 1);
    }

    #[test]
    fn a_later_meeting_suggests_the_remembered_name_until_confirmed_or_dismissed() {
        let c = db();
        let first = meeting(&c);
        rename(&c, first, 2, "Lena", true).unwrap();
        // The next meeting hears the same voice as its speaker 1, and a stranger as 2.
        let next = store::create(&c, 2).unwrap();
        let prints = [Some(print(5.0 + STEP)), Some(print(20.0))];
        let heard: Vec<Speaker> = prints.iter().zip(suggest(&prints, &profiles(&c))).zip(1..).map(|((p, s), cluster)| Speaker { cluster, name: None, suggested: s, print: p.clone() }).collect();
        replace(&c, next, &heard).unwrap();
        let shown = |c: &Connection| list(c, next).unwrap().into_iter().map(|s| (s.name, s.suggested)).collect::<Vec<_>>();
        assert_eq!(shown(&c), [(None, Some("Lena".to_string())), (None, None)]);
        // Confirming is renaming to it: the suggestion is settled and the name is the speaker's.
        rename(&c, next, 1, "Lena", true).unwrap();
        assert_eq!(shown(&c)[0], (Some("Lena".to_string()), None));
        // Dismissing leaves the speaker unnamed and keeps the remembered voice.
        replace(&c, next, &heard).unwrap();
        dismiss(&c, next, 1).unwrap();
        assert_eq!(shown(&c)[0], (None, None));
        assert_eq!(profiles(&c).len(), 1);
    }

    #[test]
    fn merging_moves_the_segments_and_keeps_one_name_and_both_voices() {
        let c = db();
        let id = meeting(&c);
        let other = meeting(&c);
        rename(&c, id, 2, "Priya", false).unwrap();
        // 3 into 2: the segments follow, the name stays, 3 is gone.
        merge(&c, id, 3, 2).unwrap();
        assert_eq!(clusters(&c, id), [Some(1), Some(2), Some(1), Some(2), Some(2)]);
        let left = list(&c, id).unwrap();
        assert_eq!(left.iter().map(|s| (s.cluster, s.name.as_deref())).collect::<Vec<_>>(), [(1, None), (2, Some("Priya"))]);
        assert_eq!(left[1].print.as_ref().unwrap().mean[0], 7.0, "voices of 5 and 9 pooled");
        // 1 into 2 where only the source is named: the name comes along.
        replace(&c, id, &[Speaker { name: Some("Tom".into()), ..speaker(1, 0.0) }, speaker(2, 5.0)]).unwrap();
        merge(&c, id, 1, 2).unwrap();
        assert_eq!(list(&c, id).unwrap()[0].name.as_deref(), Some("Tom"));
        assert_eq!(clusters(&c, id), [Some(2); 5]);
        // Other meetings are not touched, and bad merges say so.
        assert_eq!(clusters(&c, other), [Some(1), Some(2), Some(1), Some(3), Some(2)]);
        assert!(merge(&c, id, 2, 2).is_err());
        assert!(merge(&c, id, 2, 1).is_err(), "1 is gone");
    }

    #[test]
    fn a_confirmed_name_beats_a_suggestion_when_speakers_merge() {
        let c = db();
        let id = store::create(&c, 1).unwrap();
        let suggested = |cluster, who: &str| Speaker { suggested: Some(who.into()), ..speaker(cluster, 0.0) };
        replace(&c, id, &[suggested(1, "Lena"), Speaker { name: Some("Priya".into()), ..speaker(2, 0.0) }, suggested(3, "Tom")]).unwrap();
        merge(&c, id, 1, 2).unwrap();
        assert_eq!(list(&c, id).unwrap().iter().map(|s| (s.name.clone(), s.suggested.clone())).collect::<Vec<_>>(), [(Some("Priya".into()), None), (None, Some("Tom".into()))]);
        merge(&c, id, 3, 2).unwrap();
        assert_eq!(list(&c, id).unwrap()[0].name.as_deref(), Some("Priya"));
    }

    // Real voices, to pick MATCH from: each `say` voice reads six passages; a voice's first three against its other
    // three (about a minute a side, as in a meeting) must be nearer than any pair of different voices. Prints every
    // distance. By hand, in release (needs macOS `say`): `cargo test --release -- --ignored say_voices --nocapture`
    #[test]
    #[ignore]
    fn say_voices_are_nearer_to_themselves_than_to_each_other_by_a_margin_around_match() {
        let passages = [
            "Good morning everyone. Let's start with the roadmap review for the next quarter, and then go through the open risks and the staffing plan for the platform team. After that we can look at the budget and decide what to cut if the numbers do not work out.",
            "Thanks for having me. I want to flag that the launch date depends on the security review, which is not finished yet, so we should not promise anything to customers. The vendor said they need another two weeks, and I would rather be late than ship something we have not tested.",
            "On the data side, the migration finished over the weekend without any downtime. We moved about forty million rows, and the only problem was a slow index that we rebuilt on Sunday night. Reports are now running from the new warehouse, and the dashboards look correct.",
            "I disagree a little. The customers we talked to last week did not care about the new editor at all. They kept asking about permissions and audit logs, and two of them said they would not renew without single sign on. I think we are building the wrong thing first.",
            "Let me summarize what we agreed. Maria owns the pricing proposal, Jonas will write the migration guide, and I will talk to legal about the contract language. We meet again on Thursday, and if anything blocks you before then, please send a message in the channel.",
            "One more thing about hiring. We have three open roles and only two interviewers with free time, so we need to either pause one role or bring in help from the other team. My suggestion is to pause the design role until the spring, because the backend work is the real bottleneck.",
        ];
        let dir = std::env::temp_dir().join(format!("myna-say-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let listed = String::from_utf8(std::process::Command::new("say").args(["-v", "?"]).output().unwrap().stdout).unwrap();
        let voices: Vec<&str> = ["Samantha", "Daniel", "Karen", "Fred", "Alex", "Victoria", "Moira", "Tessa", "Rishi", "Fiona"].into_iter().filter(|v| listed.contains(v)).collect();
        assert!(voices.len() >= 4, "need a few voices, have {voices:?}");
        let say = |voice: &str, texts: &[&str], first: usize| {
            let mut heard: Vec<f32> = vec![];
            for (n, text) in texts.iter().enumerate() {
                let (aiff, wav) = (dir.join(format!("{voice}{}.aiff", first + n)), dir.join(format!("{voice}{}.wav", first + n)));
                assert!(std::process::Command::new("say").args(["-v", voice, "-o"]).arg(&aiff).arg(text).status().unwrap().success());
                assert!(std::process::Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000", "-c", "1"]).arg(&aiff).arg(&wav).status().unwrap().success());
                heard.extend(hound::WavReader::open(&wav).unwrap().samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0));
            }
            voiceprint::compute(&heard).unwrap_or_else(|| panic!("{voice} said too little"))
        };
        let prints: Vec<[Print; 2]> = voices.iter().map(|v| [say(v, &passages[..3], 0), say(v, &passages[3..], 3)]).collect();
        let (mut same, mut other) = (0f32, f32::MAX);
        for (i, a) in prints.iter().enumerate() {
            same = same.max(voiceprint::distance(&a[0], &a[1]));
            for (j, b) in prints.iter().enumerate().filter(|(j, _)| *j != i) {
                let d = voiceprint::distance(&a[0], &b[1]);
                println!("{:>9} vs {:<9} {d:.2}", voices[i], voices[j]);
                other = other.min(d);
            }
            println!("{:>9} vs itself   {:.2}", voices[i], voiceprint::distance(&a[0], &a[1]));
        }
        println!("farthest same voice {same:.2}, nearest other voice {other:.2}, MATCH {MATCH}");
        assert!(same < MATCH && MATCH < other, "MATCH {MATCH} does not separate {same} from {other}");
    }
}
