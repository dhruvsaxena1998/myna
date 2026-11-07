//! The feedback log: a 👍/👎 per dictation, the text the user says they meant on a 👎, and the fixes Myna suggests
//! once the same correction comes up twice. It stays on the Mac; Export feedback writes a file and sends nothing.

use crate::rules::{self, Word};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashMap, HashSet};

/// How many dictations must show the same correction before Myna suggests it.
const REPEATS: usize = 2;

/// A suggested fix. `kind` is "alias" (`from` heard as `to`), "word" (a Words to remember spelling) or "keep" (a Never remove word).
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Fix {
    pub key: String,
    pub kind: &'static str,
    pub from: Option<String>,
    pub to: String,
    pub count: usize,
}

fn err(e: rusqlite::Error) -> String {
    format!("Couldn't use the feedback log: {e}")
}

/// Rate a dictation; `None` takes the rating back. The corrected text is kept only with "down".
pub fn set(conn: &Connection, trace: i64, rating: Option<&str>, corrected: Option<&str>, now_ms: i64) -> Result<(), String> {
    match rating {
        None => conn.execute("DELETE FROM feedback WHERE trace = ?1", [trace]).map(drop).map_err(err),
        Some(r @ ("up" | "down")) => {
            let corrected = corrected.map(str::trim).filter(|c| !c.is_empty()).filter(|_| r == "down");
            conn.execute(
                "INSERT INTO feedback(trace, rating, corrected, created) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(trace) DO UPDATE SET rating = ?2, corrected = ?3, created = ?4",
                (trace, r, corrected, now_ms),
            )
            .map(drop)
            .map_err(err)
        }
        Some(r) => Err(format!("A rating is \"up\" or \"down\", not \"{r}\".")),
    }
}

/// Words with the punctuation around them trimmed; a lone punctuation mark is no word.
fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric())).filter(|w| !w.is_empty()).collect()
}

/// The hunks that turn `a` into `b`: (removed, inserted), from a longest-common-subsequence diff.
// shortcut: O(n*m) table; dictations are short, so cap or switch to Myers if texts grow past a few thousand words.
fn hunks<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(Vec<&'a str>, Vec<&'a str>)> {
    let mut l = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            l[i][j] = if a[i] == b[j] { l[i + 1][j + 1] + 1 } else { l[i + 1][j].max(l[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    let (mut r, mut n): (Vec<&str>, Vec<&str>) = (vec![], vec![]);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            if !r.is_empty() || !n.is_empty() {
                out.push((std::mem::take(&mut r), std::mem::take(&mut n)));
            }
            i += 1;
            j += 1;
        } else if j == b.len() || (i < a.len() && l[i + 1][j] >= l[i][j + 1]) {
            r.push(a[i]);
            i += 1;
        } else {
            n.push(b[j]);
            j += 1;
        }
    }
    if !r.is_empty() || !n.is_empty() {
        out.push((r, n));
    }
    out
}

/// What the rules step removed (a filler or repeat), as the trimmed raw words, lowercase.
fn removed(heard: &str, edits: &str) -> HashSet<String> {
    let chars: Vec<char> = heard.chars().collect();
    let edits: Vec<serde_json::Value> = serde_json::from_str(edits).unwrap_or_default();
    edits
        .iter()
        .filter(|e| matches!(e["kind"].as_str(), Some("filler" | "repeat")) && e["to"] == "")
        .filter_map(|e| {
            let (s, t) = (e["start"].as_u64()? as usize, e["end"].as_u64()? as usize);
            let raw: String = chars.get(s..t.min(chars.len()))?.iter().collect();
            Some(raw.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        })
        .collect()
}

/// A fix's kind, from, to and the dictations it came from.
type Group = (&'static str, Option<String>, String, HashSet<i64>);

/// The fixes one 👎 shows: (key, kind, from, to).
fn found(pasted: &str, corrected: &str, heard: &str, edits: &str) -> Vec<(String, &'static str, Option<String>, String)> {
    let (a, b) = (words(pasted), words(corrected));
    let gone = removed(heard, edits);
    let mut out = vec![];
    for (r, i) in hunks(&a, &b) {
        let (from, to) = (r.join(" "), i.join(" "));
        if !r.is_empty() && !i.is_empty() && r.len() <= 3 && i.len() <= 3 {
            if from.to_lowercase() == to.to_lowercase() {
                // One fix per respelled word, so a re-capitalised neighbour isn't glued on.
                // A capital the user only added or dropped at a sentence start is grammar, not a spelling.
                let start = |y: &str| corrected.trim_start().starts_with(y) || [". ", "? ", "! "].iter().any(|p| corrected.contains(&format!("{p}{y}")));
                let first_only = |x: &str, y: &str| x.chars().skip(1).eq(y.chars().skip(1));
                for (_, y) in r.iter().zip(&i).filter(|(x, y)| x != y && !(first_only(x, y) && start(y))) {
                    out.push((format!("word:{y}"), "word", None, y.to_string()));
                }
            } else {
                out.push((format!("alias:{from}→{to}"), "alias", Some(from.clone()), to));
            }
        } else if r.is_empty() && i.len() == 1 && gone.contains(&to.to_lowercase()) {
            let to = to.to_lowercase();
            out.push((format!("keep:{to}"), "keep", None, to));
        }
    }
    out
}

pub fn fixes(conn: &Connection) -> Result<Vec<Fix>, String> {
    let mut q = conn
        .prepare(
            "SELECT f.trace, f.corrected,
                (SELECT text FROM step WHERE trace = f.trace ORDER BY rowid DESC LIMIT 1),
                (SELECT text FROM step WHERE trace = f.trace AND kind = 'speech-to-text'),
                (SELECT edits FROM step WHERE trace = f.trace AND kind = 'rules')
             FROM feedback f WHERE f.rating = 'down' AND f.corrected IS NOT NULL",
        )
        .map_err(err)?;
    let rows = q
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?)))
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for (trace, corrected, pasted, heard, edits) in rows {
        for (key, kind, from, to) in found(&pasted.unwrap_or_default(), &corrected, &heard.unwrap_or_default(), &edits.unwrap_or_default()) {
            groups.entry(key).or_insert((kind, from, to, HashSet::new())).3.insert(trace);
        }
    }
    let settings = rules::load(conn)?;
    let seen: HashMap<String, usize> = conn
        .prepare("SELECT key, seen FROM fix_dismissed")
        .map_err(err)?
        .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;
    let mut out: Vec<Fix> = groups
        .into_iter()
        .map(|(key, (kind, from, to, traces))| Fix { key, kind, from, to, count: traces.len() })
        .filter(|f| f.count >= REPEATS && seen.get(&f.key).is_none_or(|s| *s < f.count))
        .filter(|f| match f.kind {
            "alias" => !settings.words.iter().any(|w| w.spelling == f.to && w.heard == f.from),
            "word" => !settings.words.iter().any(|w| w.spelling.to_lowercase() == f.to.to_lowercase()),
            _ => !settings.keep.iter().any(|k| k.to_lowercase() == f.to),
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    Ok(out)
}

/// Apply a suggested fix: an alias or a word joins Words to remember, a keep joins Never remove.
pub fn add(conn: &Connection, key: &str) -> Result<(), String> {
    let fix = fixes(conn)?.into_iter().find(|f| f.key == key).ok_or("That fix isn't suggested any more.")?;
    let mut s = rules::load(conn)?;
    match fix.kind {
        "alias" => s.words.push(Word { spelling: fix.to, heard: fix.from }),
        "word" => s.words.push(Word { spelling: fix.to, heard: None }),
        _ => s.keep.push(fix.to),
    }
    rules::save(conn, &s)
}

/// "Not now": the fix stays hidden until it comes up in more dictations than now.
pub fn dismiss(conn: &Connection, key: &str) -> Result<(), String> {
    let count = fixes(conn)?.into_iter().find(|f| f.key == key).map_or(0, |f| f.count) as i64;
    conn.execute("INSERT INTO fix_dismissed(key, seen) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET seen = ?2", (key, count)).map(drop).map_err(err)
}

/// Every rating with its trace, newest first, for Export feedback.
pub fn export(conn: &Connection, now_ms: i64) -> Result<serde_json::Value, String> {
    let rows = conn
        .prepare("SELECT trace, rating, corrected, created FROM feedback ORDER BY created DESC, trace DESC")
        .map_err(err)?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?)))
        .map_err(err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(err)?;
    let mut feedback = vec![];
    for (trace, rating, corrected, created) in rows {
        feedback.push(serde_json::json!({ "rating": rating, "corrected": corrected, "created": created, "trace": crate::trace::get(conn, trace)? }));
    }
    Ok(serde_json::json!({ "exported": now_ms, "app_version": env!("CARGO_PKG_VERSION"), "feedback": feedback }))
}

/// Writes `Myna feedback <date>.json` into `dir`, or `... 2.json`, `... 3.json` if that name is taken. Returns its path.
pub fn write_export(dir: &std::path::Path, date: &str, json: &serde_json::Value) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    let text = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    for n in 1.. {
        let name = if n == 1 { format!("Myna feedback {date}.json") } else { format!("Myna feedback {date} {n}.json") };
        let path = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => return f.write_all(text.as_bytes()).map(|_| path).map_err(|e| format!("Couldn't write the feedback file: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("Couldn't write the feedback file in {}: {e}", dir.display())),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{self, Step, Trace};

    /// A dictation heard as `heard`, pasted as `pasted`, with the rules step's edits.
    fn dictation(conn: &Connection, started: i64, heard: &str, pasted: &str, edits: Option<&str>) -> i64 {
        let step = |kind, text: &str, edits: Option<&str>| Step { kind, text: text.into(), ms: 1.0, settings: None, edits: edits.map(Into::into) };
        let steps = vec![step("speech-to-text", heard, None), step("rules", pasted, edits)];
        trace::save(conn, &Trace { started, mode: "hold", outcome: "pasted", steps, ..Default::default() }).unwrap();
        conn.query_row("SELECT max(id) FROM trace", [], |r| r.get(0)).unwrap()
    }

    fn open() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        (tmp, conn)
    }

    fn down(conn: &Connection, t: i64, corrected: &str) {
        set(conn, t, Some("down"), Some(corrected), 10).unwrap();
    }

    fn shown(conn: &Connection) -> Vec<String> {
        fixes(conn).unwrap().into_iter().map(|f| format!("{} x{}", f.key, f.count)).collect()
    }

    #[test]
    fn a_diff_gives_an_alias_a_word_a_keep_or_nothing() {
        let um = r#"[{"kind":"filler","start":0,"end":3,"to":""}]"#;
        let rows = [
            ("Ask Mina today", "Ask Myna today", "Ask Mina today", None, Some("alias:Mina→Myna")),
            ("ask myna today.", "Ask Myna today", "ask myna today.", None, Some("word:Myna")),
            ("Done. ask later", "Done. Ask later", "Done. ask later", None, None),
            ("we use iphone", "We use iPhone", "we use iphone", None, Some("word:iPhone")),
            ("Oh that works", "Oh, that works", "Oh that works", None, None),
            ("that works", "Um that works", "Um that works", Some(um), Some("keep:um")),
            ("one two three four", "five six seven eight nine", "one two three four", None, None),
            ("one two", "one two three", "one two", None, None),
        ];
        for (pasted, corrected, heard, edits, want) in rows {
            let got = found(pasted, corrected, heard, edits.unwrap_or("[]"));
            assert_eq!(got.first().map(|f| f.0.as_str()), want, "{pasted} → {corrected}");
        }
    }

    #[test]
    fn a_fix_needs_two_dictations_and_is_hidden_once_applied_or_dismissed() {
        let (_t, conn) = open();
        let a = dictation(&conn, 1, "Ask Mina", "Ask Mina", None);
        down(&conn, a, "Ask Myna");
        assert!(fixes(&conn).unwrap().is_empty(), "one is not enough");
        down(&conn, a, "Ask Myna"); // rating the same dictation again is still one
        assert!(fixes(&conn).unwrap().is_empty());
        let b = dictation(&conn, 2, "Mina is here", "Mina is here", None);
        down(&conn, b, "Myna is here");
        let f = fixes(&conn).unwrap();
        assert_eq!(f, [Fix { key: "alias:Mina→Myna".into(), kind: "alias", from: Some("Mina".into()), to: "Myna".into(), count: 2 }]);
        dismiss(&conn, &f[0].key).unwrap();
        assert!(fixes(&conn).unwrap().is_empty());
        let c = dictation(&conn, 3, "Hi Mina", "Hi Mina", None);
        down(&conn, c, "Hi Myna");
        assert_eq!(shown(&conn), ["alias:Mina→Myna x3"], "comes back with a new occurrence");
        add(&conn, "alias:Mina→Myna").unwrap();
        assert!(fixes(&conn).unwrap().is_empty());
        assert_eq!(rules::load(&conn).unwrap().words, [Word { spelling: "Myna".into(), heard: Some("Mina".into()) }]);
        assert!(add(&conn, "alias:Mina→Myna").is_err(), "not a current fix");
    }

    #[test]
    fn a_word_and_a_keep_are_added_to_the_lists() {
        let (_t, conn) = open();
        let um = r#"[{"kind":"filler","start":0,"end":3,"to":""}]"#;
        for n in 0..2 {
            let t = dictation(&conn, n, "Um go", "go", Some(um));
            down(&conn, t, "Um go");
            let t = dictation(&conn, 10 + n, "see myna", "see myna", None);
            down(&conn, t, "see Myna");
        }
        assert_eq!(shown(&conn), ["keep:um x2", "word:Myna x2"]);
        add(&conn, "keep:um").unwrap();
        add(&conn, "word:Myna").unwrap();
        let s = rules::load(&conn).unwrap();
        assert_eq!((s.keep, s.words), (vec!["um".to_string()], vec![Word { spelling: "Myna".into(), heard: None }]));
        assert!(fixes(&conn).unwrap().is_empty());
    }

    #[test]
    fn ratings_validate_clear_and_go_with_their_dictation() {
        let (_t, conn) = open();
        let t = dictation(&conn, 1, "a", "a", None);
        assert!(set(&conn, t, Some("meh"), None, 1).is_err());
        set(&conn, t, Some("up"), Some("ignored"), 1).unwrap();
        let corrected: Option<String> = conn.query_row("SELECT corrected FROM feedback", [], |r| r.get(0)).unwrap();
        assert_eq!(corrected, None);
        set(&conn, t, Some("down"), Some("  b  "), 2).unwrap();
        let corrected: Option<String> = conn.query_row("SELECT corrected FROM feedback", [], |r| r.get(0)).unwrap();
        assert_eq!(corrected.as_deref(), Some("b"));
        assert_eq!(trace::list(&conn).unwrap()[0].rating.as_deref(), Some("down"));
        assert_eq!(trace::get(&conn, t).unwrap()["rating"], "down");
        assert_eq!(trace::all(&conn).unwrap()[0]["rating"], "down");
        set(&conn, t, None, None, 3).unwrap();
        assert_eq!(trace::get(&conn, t).unwrap()["rating"], serde_json::Value::Null);
        set(&conn, t, Some("up"), None, 4).unwrap();
        trace::delete(&conn, t).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM feedback", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let t = dictation(&conn, 2, "a", "a", None);
        set(&conn, t, Some("up"), None, 1).unwrap();
        trace::clear(&conn).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM feedback", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn export_lists_newest_first_and_the_file_never_overwrites() {
        let (t, conn) = open();
        let a = dictation(&conn, 1, "a", "a", None);
        let b = dictation(&conn, 2, "b", "b", None);
        set(&conn, a, Some("up"), None, 5).unwrap();
        set(&conn, b, Some("down"), Some("c"), 6).unwrap();
        let j = export(&conn, 99).unwrap();
        assert_eq!((j["exported"].as_i64(), j["feedback"][0]["rating"].as_str(), j["feedback"][1]["trace"]["id"].as_i64()), (Some(99), Some("down"), Some(a)));
        let one = write_export(t.path(), "2026-11-07", &j).unwrap();
        let two = write_export(t.path(), "2026-11-07", &j).unwrap();
        assert_eq!(one.file_name().unwrap(), "Myna feedback 2026-11-07.json");
        assert_eq!(two.file_name().unwrap(), "Myna feedback 2026-11-07 2.json");
    }
}
