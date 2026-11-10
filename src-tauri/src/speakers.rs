//! Who said each line on the Them track: the diarized voice, named from the call's live captions when they agree.

use crate::runtime::Turn;
use std::collections::BTreeMap;

// Teams shows a caption a moment after the words, so a caption at T speaks for [T - 3 s, T + 0.5 s].
const BEFORE: i64 = 3000;
const AFTER: i64 = 500;
/// A name is taken when it holds this share of the speaker's caption overlap, over at least `MIN_EVENTS` captions.
const CLEAR: f64 = 0.6;
const MIN_EVENTS: usize = 3;

fn overlap(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0)
}

/// A label for each Them line `(start_ms, end_ms)`, in order: "Speaker 2", "Speaker 3"… by first appearance
/// (Me is Speaker 1), or a caption's name. None where no voice was heard (the line stays "Them").
/// `me` are the Me lines, `caps` are `(ms from the meeting's start, name)`.
// shortcut: a line takes the one voice that covers most of it; a line two people share isn't split.
pub fn label(them: &[(i64, i64)], me: &[(i64, i64)], turns: &[Turn], caps: &[(i64, String)]) -> Vec<Option<String>> {
    let ms = |s: f32| (s * 1000.0).round() as i64;
    let turns: Vec<(usize, (i64, i64))> = turns.iter().map(|t| (t.speaker, (ms(t.start), ms(t.end)))).collect();
    let voice: Vec<Option<usize>> = them
        .iter()
        .map(|&l| {
            let mut by: BTreeMap<usize, i64> = BTreeMap::new();
            turns.iter().for_each(|&(s, t)| *by.entry(s).or_default() += overlap(l, t));
            by.into_iter().filter(|e| e.1 > 0).max_by_key(|e| (e.1, std::cmp::Reverse(e.0))).map(|e| e.0)
        })
        .collect();
    let mut rank: BTreeMap<usize, usize> = BTreeMap::new();
    for v in voice.iter().flatten() {
        let n = rank.len() + 2;
        rank.entry(*v).or_insert(n);
    }

    // For each voice, the time and count of the captions of each name that fell on it.
    let mut tally: BTreeMap<usize, BTreeMap<String, (i64, usize)>> = BTreeMap::new();
    for (at, name) in caps {
        let w = (at - BEFORE, at + AFTER);
        // The user's own words show up as captions too: they happen while the mic speaks.
        let sum = |ls: &[(i64, i64)]| ls.iter().map(|&l| overlap(w, l)).sum::<i64>();
        if name.eq_ignore_ascii_case("me") || sum(me) > sum(them) {
            continue;
        }
        for &v in rank.keys() {
            let ms: i64 = turns.iter().filter(|t| t.0 == v).map(|t| overlap(w, t.1)).sum();
            if ms > 0 {
                let e = tally.entry(v).or_default().entry(name.clone()).or_default();
                *e = (e.0 + ms, e.1 + 1);
            }
        }
    }
    // Each voice's clear favourite, the strongest claim on a name first; a name goes to one voice only.
    let mut claims = vec![];
    for (v, names) in &tally {
        let total: i64 = names.values().map(|e| e.0).sum();
        match names.iter().max_by_key(|e| e.1 .0) {
            Some((name, &(ms, n))) if n >= MIN_EVENTS && ms as f64 >= CLEAR * total as f64 => claims.push((ms, *v, name.to_lowercase(), name.clone())),
            _ => {}
        }
    }
    claims.sort_by_key(|c| std::cmp::Reverse(c.0));
    let (mut named, mut taken): (BTreeMap<usize, String>, Vec<String>) = Default::default();
    for (_, v, key, name) in claims {
        if !taken.contains(&key) {
            taken.push(key);
            named.insert(v, name);
        }
    }
    voice.iter().map(|v| v.map(|v| named.get(&v).cloned().unwrap_or_else(|| format!("Speaker {}", rank[&v])))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 1000;
    fn turn(a: i64, b: i64, speaker: usize) -> Turn {
        Turn { start: (a / S) as f32, end: (b / S) as f32, speaker }
    }
    /// Voice 4 talks 0 to 10 s and 30 to 40 s, voice 1 talks 10 to 30 s. Lines sit inside the turns.
    fn call() -> ([(i64, i64); 3], Vec<Turn>) {
        ([(S, 9 * S), (11 * S, 29 * S), (31 * S, 39 * S)], vec![turn(0, 10 * S, 4), turn(10 * S, 30 * S, 1), turn(30 * S, 40 * S, 4)])
    }
    /// A caption for each of `at` seconds, just after the words.
    fn caps(name: &str, at: &[i64]) -> Vec<(i64, String)> {
        at.iter().map(|a| (a * S, name.to_string())).collect()
    }
    fn labels(caps: &[(i64, String)], me: &[(i64, i64)]) -> Vec<String> {
        let (them, turns) = call();
        label(&them, me, &turns, caps).into_iter().map(|l| l.unwrap_or("Them".into())).collect()
    }

    #[test]
    fn voices_are_numbered_in_order_of_first_appearance_without_captions() {
        assert_eq!(labels(&[], &[]), ["Speaker 2", "Speaker 3", "Speaker 2"]);
    }

    #[test]
    fn a_line_no_voice_covers_stays_them() {
        let l = label(&[(50 * S, 52 * S)], &[], &[turn(0, 10 * S, 0)], &[]);
        assert_eq!(l, [None]);
    }

    #[test]
    fn a_voice_takes_the_name_its_captions_overlap_most() {
        // Ana's captions land on voice 4, Bo's on voice 1.
        let mut c = caps("Ana", &[4, 8, 36]);
        c.extend(caps("Bo", &[14, 20, 26]));
        assert_eq!(labels(&c, &[]), ["Ana", "Bo", "Ana"]);
    }

    #[test]
    fn a_tie_is_not_clear_so_it_stays_numbered() {
        let mut c = caps("Ana", &[4, 6, 8]);
        c.extend(caps("Bo", &[5, 7, 9]));
        assert_eq!(labels(&c, &[])[0], "Speaker 2");
    }

    #[test]
    fn too_few_captions_stay_numbered() {
        assert_eq!(labels(&caps("Ana", &[4, 6]), &[])[0], "Speaker 2");
    }

    #[test]
    fn two_voices_fighting_for_a_name_give_it_to_the_stronger() {
        // Both voices read as Ana, voice 4 more strongly: Ana is voice 4's, voice 1 is numbered.
        let mut c = caps("Ana", &[5, 9, 33, 38, 39]);
        c.extend(caps("Ana", &[13, 14, 15, 16]));
        assert_eq!(labels(&c, &[]), ["Ana", "Speaker 3", "Ana"]);
    }

    #[test]
    fn the_users_own_captions_are_ignored() {
        // Voice 0 is on the line Them 1 to 9 s; Me speaks 12 to 20 s, and Teams captions it under the user's name.
        let (them, turns, own) = ([(S, 9 * S)], [turn(0, 30 * S, 0)], caps("Dhruv", &[14, 16, 18, 20]));
        assert_eq!(label(&them, &[], &turns, &own), [Some("Dhruv".to_string())], "without Me lines it would be taken");
        assert_eq!(label(&them, &[(12 * S, 20 * S)], &turns, &own), [Some("Speaker 2".to_string())]);
        assert_eq!(label(&them, &[], &turns, &caps("Me", &[4, 6, 8])), [Some("Speaker 2".to_string())], "nobody is named Me");
    }
}
