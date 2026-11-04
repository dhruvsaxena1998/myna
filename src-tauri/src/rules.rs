//! Rules cleanup: the deterministic pass between speech-to-text and paste.
//! It deletes listed filler words and repeats, writes spoken numbers as digits and respells the user's
//! words to remember, then repairs the gap. It never adds, reorders or rephrases.

use rusqlite::{Connection, OptionalExtension};

/// A name or special word the user lists, with the wrong form the model tends to write (heard as), if any.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub spelling: String,
    pub heard: Option<String>,
}

pub struct Settings {
    /// The Dictation language, as a code like "en".
    pub language: String,
    /// The Tidy up switch: fillers, repeats and numbers. Words to remember always apply.
    pub tidy: bool,
    /// Oldest first.
    pub words: Vec<Word>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { language: "en".into(), tidy: true, words: vec![] }
    }
}

#[derive(Clone, Debug)]
struct Tok {
    text: String,
    /// Whitespace came before it in the original.
    sp: bool,
}

impl Tok {
    fn is_word(&self) -> bool {
        self.text.chars().next().is_some_and(char::is_alphanumeric)
    }
    fn is(&self, t: &str) -> bool {
        self.text == t
    }
    fn is_end(&self) -> bool {
        matches!(self.text.as_str(), "." | "?" | "!")
    }
    fn lower(&self) -> String {
        self.text.to_lowercase()
    }
}

/// Splits into words (with inner apostrophes and hyphens, "th-" cut-offs and numbers like 4:30 kept whole)
/// and single punctuation marks, remembering where whitespace was.
fn tokenize(text: &str) -> Vec<Tok> {
    let cs: Vec<char> = text.chars().collect();
    let (mut out, mut i, mut sp) = (Vec::new(), 0, false);
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            sp = true;
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if c.is_alphanumeric() {
            while j < cs.len() {
                let (d, next) = (cs[j], cs.get(j + 1));
                let inner = match d {
                    '\'' | '\u{2019}' => next.is_some_and(|n| n.is_alphanumeric()),
                    '-' => next.is_none_or(|n| n.is_alphanumeric() || n.is_whitespace()),
                    '.' | ',' | ':' => cs[j - 1].is_ascii_digit() && next.is_some_and(|n| n.is_ascii_digit()),
                    _ => d.is_alphanumeric(),
                };
                if !inner {
                    break;
                }
                j += 1;
            }
        }
        out.push(Tok { text: cs[i..j].iter().collect(), sp });
        sp = false;
        i = j;
    }
    out
}

fn join(toks: &[Tok]) -> String {
    let mut s = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 && t.sp {
            s.push(' ');
        }
        s.push_str(&t.text);
    }
    s
}

fn is_closing(t: &Tok) -> bool {
    matches!(t.text.as_str(), "," | "." | "?" | "!" | ";" | ":" | ")" | "%")
}

/// Removes `toks[i..j]` and repairs the seam: spacing, and a capital if a capitalised word began a sentence there.
fn cut(toks: &mut Vec<Tok>, i: usize, j: usize) {
    let any_sp = toks[i..j].iter().any(|t| t.sp);
    let was_cap = toks[i].text.starts_with(char::is_uppercase);
    toks.drain(i..j);
    if i >= toks.len() {
        return;
    }
    let starts = i == 0 || toks[i - 1].is_end();
    let t = &mut toks[i];
    t.sp = i > 0 && !is_closing(t) && (t.sp || any_sp);
    if starts && was_cap && t.is_word() {
        let mut c = t.text.chars();
        t.text = c.next().unwrap().to_uppercase().chain(c).collect();
    }
}

const FILLERS: [&str; 10] = ["um", "uh", "uhm", "er", "erm", "hmm", "mm", "mhm", "ah", "oh"];

/// A filler word in any case with its last letter stretched ("Ummm"); "err" and "hm" are real words.
fn is_filler(w: &str) -> bool {
    let w = w.to_lowercase();
    w != "err" && FILLERS.iter().any(|f| w.strip_prefix(f).is_some_and(|rest| rest.chars().all(|c| Some(c) == f.chars().last())))
}

/// Number words and digits, for the places a neighbour being a number changes what a word means.
fn is_number(t: &Tok) -> bool {
    const WORDS: [&str; 41] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
        "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen", "twenty", "thirty",
        "forty", "fifty", "sixty", "seventy", "eighty", "ninety", "hundred", "thousand", "million", "billion",
        "point", "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
    ];
    t.text.starts_with(|c: char| c.is_ascii_digit()) || WORDS.contains(&t.lower().as_str())
}

fn fillers(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        let near_number = (i > 0 && is_number(&toks[i - 1])) || toks.get(i + 1).is_some_and(is_number);
        let t = &toks[i];
        if !is_filler(&t.text) || (near_number && matches!(t.lower().as_str(), "oh" | "mm")) {
            i += 1;
            continue;
        }
        let (mut a, mut b) = (i, i + 1);
        if a > 0 && toks[a - 1].is(",") {
            a -= 1;
        }
        if toks.get(b).is_some_and(|t| t.is(",")) {
            b += 1;
        }
        // A sentence of only filler words goes with its full stop.
        let alone = (a == 0 || toks[a - 1].is_end()) && toks.get(b).is_some_and(Tok::is_end);
        cut(toks, a, b + alone as usize);
        i = a;
    }
}

impl text2num::Token for &Tok {
    fn text(&self) -> std::borrow::Cow<'_, str> {
        self.text.as_str().into()
    }
    fn text_lowercase(&self) -> std::borrow::Cow<'_, str> {
        self.lower().into()
    }
}

/// Writes spoken numbers as digits, except next to another number or "point", and "a hundred":
/// those would paste "20 27" or "4 30"; LLM cleanup handles dates, times, phones and versions.
fn numbers(toks: &mut Vec<Tok>) {
    let found = text2num::find_numbers(toks.iter(), &text2num::Language::english(), 10.0);
    for n in found.into_iter().rev() {
        let touches = (n.start > 0 && is_number(&toks[n.start - 1])) || toks.get(n.end).is_some_and(is_number);
        let a_hundred = n.start > 0 && toks[n.start - 1].lower() == "a" && toks[n.start].lower() == "hundred";
        if touches || a_hundred {
            continue;
        }
        let sp = toks[n.start].sp;
        toks.splice(n.start..n.end, [Tok { text: n.text, sp }]);
    }
}

const REPEATABLE: [&str; 26] = [
    "i", "we", "you", "she", "they", "it", "my", "our", "your", "the", "a", "an", "to", "of", "in", "on", "at", "for",
    "with", "and", "but", "or", "if", "can", "would", "should",
];

/// Collapses runs of listed words and two-word repeats led by a listed word to their first copy, and drops cut-offs.
/// Commas between copies go with them; a full stop or question mark between them blocks the collapse.
fn repeats(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        // "th- the": a word ending in a hyphen that starts the next word.
        let stem = t.text.strip_suffix('-').filter(|s| !s.is_empty()).map(str::to_lowercase);
        if stem.is_some_and(|s| toks.get(i + 1).is_some_and(|n| n.is_word() && n.lower().starts_with(&s))) {
            cut(toks, i, i + 1);
            continue;
        }
        if !t.is_word() || !REPEATABLE.contains(&t.lower().as_str()) {
            i += 1;
            continue;
        }
        // Copies of the same words (1 or 2 long) after `from`, with commas allowed between.
        let len = if toks.get(i + 1).is_some_and(|n| n.is_word() && n.lower() != t.lower()) { 2 } else { 1 };
        let same = |at: usize| (0..len).all(|k| toks.get(at + k).is_some_and(|c| c.is_word() && c.lower() == toks[i + k].lower()));
        let (mut end, mut commas) = (i + len, false);
        loop {
            let mut at = end;
            while toks.get(at).is_some_and(|c| c.is(",")) {
                at += 1;
            }
            if !same(at) {
                break;
            }
            commas |= at > end;
            end = at + len;
        }
        if end > i + len {
            if commas && toks.get(end).is_some_and(|c| c.is(",")) {
                end += 1;
            }
            cut(toks, i + len, end);
        }
        i += 1;
    }
}

fn split_possessive(w: &str) -> (&str, &str) {
    for suffix in ["'s", "\u{2019}s"] {
        if let Some(stem) = w.strip_suffix(suffix) {
            return (stem, suffix);
        }
    }
    (w, "")
}

/// How many tokens from `at` the entry covers, and the 's it ends with. An alias (heard as) matches its words
/// exactly as typed; a plain entry matches ignoring case, spaces and hyphens.
fn matched(w: &Word, toks: &[Tok], at: usize) -> Option<(usize, String)> {
    let mut seen = String::new();
    let (heard, key) = (w.heard.as_ref().map(|h| h.split_whitespace().collect::<Vec<_>>()), alnum(&w.spelling));
    for (n, t) in toks[at..].iter().enumerate() {
        let last = n + 1 == heard.as_ref().map_or(usize::MAX, Vec::len);
        let (stem, suffix) = split_possessive(&t.text);
        if let Some(h) = &heard {
            if if last { stem != h[n] } else { t.text != h[n] } {
                return None;
            }
            if last {
                return Some((n + 1, suffix.into()));
            }
        } else {
            if !t.is_word() {
                return None;
            }
            seen += &alnum(stem);
            if seen == key {
                return Some((n + 1, suffix.into()));
            }
            if !key.starts_with(&seen) {
                return None;
            }
        }
    }
    None
}

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Respells the user's words to remember. The entry's spelling always wins, sentence start included.
fn remember(toks: &mut Vec<Tok>, words: &[Word]) {
    // Aliases first, then longer entries, so "Open AI Codex" isn't half-eaten by "Open AI".
    let mut order: Vec<&Word> = words.iter().filter(|w| !alnum(&w.spelling).is_empty()).collect();
    order.sort_by_key(|w| (w.heard.is_none(), std::cmp::Reverse(alnum(&w.spelling).len())));
    let mut i = 0;
    while i < toks.len() {
        if let Some((w, (n, suffix))) = order.iter().find_map(|w| matched(w, toks, i).map(|m| (w, m))) {
            let sp = toks[i].sp;
            toks.splice(i..i + n, [Tok { text: format!("{}{suffix}", w.spelling), sp }]);
        }
        i += 1;
    }
}

fn read(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| format!("Couldn't read your words: {e}"))
}

pub fn load(conn: &Connection) -> Result<Settings, String> {
    let err = |e: rusqlite::Error| format!("Couldn't read your words: {e}");
    let mut q = conn.prepare("SELECT spelling, heard FROM word ORDER BY id").map_err(err)?;
    let words = q.query_map([], |r| Ok(Word { spelling: r.get(0)?, heard: r.get(1)? })).map_err(err)?;
    Ok(Settings {
        language: read(conn, "dictation_language")?.unwrap_or("en".into()),
        tidy: read(conn, "dictation_tidy")?.is_none_or(|v| v == "1"),
        words: words.collect::<Result<_, _>>().map_err(err)?,
    })
}

/// Replaces the saved settings; `words` keep their order, oldest first.
// The settings page (ticket 04.10) is the first caller.
#[cfg_attr(not(test), allow(dead_code))]
pub fn save(conn: &Connection, s: &Settings) -> Result<(), String> {
    let err = |e: rusqlite::Error| format!("Couldn't save your words: {e}");
    let tx = conn.unchecked_transaction().map_err(err)?;
    for (k, v) in [("dictation_language", s.language.as_str()), ("dictation_tidy", if s.tidy { "1" } else { "0" })] {
        tx.execute("INSERT INTO setting(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [k, v]).map_err(err)?;
    }
    tx.execute("DELETE FROM word", []).map_err(err)?;
    for w in &s.words {
        tx.execute("INSERT INTO word(spelling, heard) VALUES (?1, ?2)", (&w.spelling, &w.heard)).map_err(err)?;
    }
    tx.commit().map_err(err)
}

/// Whisper's initial prompt: the entries, newest first, as many as fit in its ~224 tokens.
// shortcut: counts a token as 3 characters, a safe overestimate for names; count with Whisper's tokenizer if prompts get cut.
pub fn prompt(s: &Settings) -> String {
    let mut out = String::new();
    for w in s.words.iter().rev() {
        let add = if out.is_empty() { w.spelling.clone() } else { format!(", {}", w.spelling) };
        if (out.len() + add.len()).div_ceil(3) > 224 {
            break;
        }
        out += &add;
    }
    out
}

pub fn clean(raw: &str, s: &Settings) -> String {
    let mut toks = tokenize(raw);
    if s.tidy && s.language == "en" {
        fillers(&mut toks);
        repeats(&mut toks);
        numbers(&mut toks);
    }
    remember(&mut toks, &s.words);
    join(&toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(spelling: &str, heard: Option<&str>) -> Word {
        Word { spelling: spelling.into(), heard: heard.map(Into::into) }
    }

    /// One row per case: (in, out, why).
    #[test]
    fn fillers() {
        let rows = [
            ("Uh so I think we should uh ship it on Friday.", "So I think we should ship it on Friday.", "fillers, capital"),
            ("Um, so we ship.", "So we ship.", "comma goes"),
            ("I think, uh, we should", "I think we should", "both commas go"),
            ("It's very good, uh.", "It's very good.", "full stop stays"),
            ("Hmm, um.", "", "only filler words"),
            ("Ummm, errr, ok", "Ok", "stretched spellings, capital"),
            ("Error four oh four.", "Error four oh four.", "oh next to a number"),
            ("It's 5 mm wide.", "It's 5 mm wide.", "mm next to a number"),
            ("Uh-huh, that works.", "Uh-huh, that works.", "yes/no sound"),
            ("It was like really fast, you know.", "It was like really fast, you know.", "discourse words stay"),
        ];
        for (input, want, why) in rows {
            assert_eq!(clean(input, &Settings::default()), want, "{why}: {input}");
        }
    }

    #[test]
    fn repeats() {
        let rows = [
            ("I I think the the build is broken", "I think the build is broken", "listed repeats"),
            ("I, I, think the, the build", "I think the build", "Apple's commas"),
            ("I think uh I think it works.", "I think it works.", "filler, then two-word repeat"),
            ("We should, we should talk.", "We should talk.", "two-word repeat"),
            ("He had had enough. I know that that is true.", "He had had enough. I know that that is true.", "unlisted repeats stay"),
            ("I know that. That is true.", "I know that. That is true.", "full stop blocks"),
            ("It's very very good. Bye bye.", "It's very very good. Bye bye.", "unlisted"),
            ("one two one two", "one two one two", "first word unlisted"),
            ("th- the build", "the build", "cut-off"),
            ("e-mail me", "e-mail me", "not a cut-off"),
        ];
        for (input, want, why) in rows {
            assert_eq!(clean(input, &Settings::default()), want, "{why}: {input}");
        }
    }

    #[test]
    fn numbers() {
        let rows = [
            ("We need twenty five servers.", "We need 25 servers.", "number"),
            ("about fifteen percent more", "about 15 percent more", "number"),
            ("give me five minutes", "give me five minutes", "under threshold"),
            ("March third twenty twenty seven at four thirty pm", "March third twenty twenty seven at four thirty pm", "guard: adjacent numbers"),
            ("version two point four point one", "version two point four point one", "guard: point"),
            ("a hundred things", "a hundred things", "guard"),
            ("the one I want", "the one I want", "not a number"),
        ];
        for (input, want, why) in rows {
            assert_eq!(clean(input, &Settings::default()), want, "{why}: {input}");
        }
    }

    fn with_words(language: &str, tidy: bool) -> Settings {
        let words = vec![word("Postgres", None), word("Kubernetes", None), word("iPhone", None), word("Dhruv", Some("Drove"))];
        Settings { language: language.into(), tidy, words }
    }

    #[test]
    fn words_to_remember_and_the_switches() {
        let rows = [
            ("en", true, "post gres and kubernetes's pods", "Postgres and Kubernetes's pods", "Words to remember"),
            ("en", true, "iphone is great", "iPhone is great", "entry spelling wins"),
            ("en", true, "Ask Drove to deploy. I drove home.", "Ask Dhruv to deploy. I drove home.", "alias, case-sensitive"),
            ("en", true, "Then post the post-gres migration.", "Then post the Postgres migration.", "hyphen ignored"),
            ("de", true, "Ich glaube, er, wir sollten das das machen.", "Ich glaube, er, wir sollten das das machen.", "not English"),
            ("en", false, "Uh I I think", "Uh I I think", "switch off"),
            ("en", false, "kubernetes", "Kubernetes", "Words to remember always"),
            ("de", true, "um kubernetes", "um Kubernetes", "Words to remember in every language"),
        ];
        for (lang, tidy, input, want, why) in rows {
            assert_eq!(clean(input, &with_words(lang, tidy)), want, "{why}: {input}");
        }
        assert_eq!(clean("um um um um", &Settings::default()), "", "whisper's only-filler fixture");
    }

    /// The raw output of each model for each recording, segments joined as dictation does.
    fn fixtures() -> Vec<(String, String)> {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rules");
        let mut all = vec![];
        for model in ["parakeet", "whisper", "apple"] {
            for f in std::fs::read_dir(format!("{root}/{model}")).unwrap() {
                let f = f.unwrap().path();
                let raw = std::fs::read_to_string(&f).unwrap();
                let joined: Vec<_> = raw.split('\u{23d0}').map(str::trim).filter(|s| !s.is_empty()).collect();
                all.push((format!("{model}/{}", f.file_name().unwrap().to_string_lossy()), joined.join(" ")));
            }
        }
        assert_eq!(all.len(), 33);
        all
    }

    #[test]
    fn cleaning_twice_changes_nothing_on_every_fixture() {
        for (name, raw) in fixtures() {
            for s in [Settings::default(), with_words("en", true), with_words("de", true)] {
                let once = clean(&raw, &s);
                assert_eq!(clean(&once, &s), once, "{name}: {raw}");
            }
        }
    }
}
