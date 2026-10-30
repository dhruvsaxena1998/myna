//! Rule-based cleanup of dictated text: fillers, stutters, sentence-start capitals, personal dictionary.
use serde::Deserialize;

/// Dropped wherever they appear.
const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "er", "erm"];
/// Real words too, so they go only when the transcriber set them off with commas ("it was, like, huge").
const HEDGES: &[&str] = &["like", "you know"];
/// Legitimate doubles ("that that is", "had had"), left alone.
const KEEP_DOUBLED: &[&str] = &["had", "that", "very"];
const SENTENCE_END: [char; 3] = ['.', '?', '!'];

/// A dictionary line: `from` as the speech model writes it (blank: same as `to`), `to` as the user spells it.
#[derive(Deserialize, Debug, PartialEq, Clone)]
pub struct Entry {
    #[serde(default)]
    pub from: String,
    pub to: String,
}

/// The `dictionary` setting: a JSON array of entries. Anything unreadable means no dictionary.
pub fn dictionary(setting: Option<String>) -> Vec<Entry> {
    let mut v: Vec<Entry> = setting.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    v.retain(|e| !e.to.trim().is_empty());
    v
}

/// Word without surrounding punctuation, lowercased.
fn core(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()
}

fn lead(w: &str) -> &str {
    &w[..w.len() - w.trim_start_matches(|c: char| !c.is_alphanumeric()).len()]
}

fn tail(w: &str) -> &str {
    &w[w.trim_end_matches(|c: char| !c.is_alphanumeric()).len()..]
}

fn ends_sentence(w: &str) -> bool {
    tail(w).contains(SENTENCE_END)
}

/// How many words from the front of `words` are a hedge set off by punctuation on both sides.
fn hedge_len(words: &[&str], prev: Option<&String>) -> usize {
    if !prev.is_none_or(|p| tail(p).contains([',', '.', '?', '!'])) {
        return 0;
    }
    HEDGES
        .iter()
        .map(|h| h.split(' ').collect::<Vec<_>>())
        .find(|h| {
            let n = h.len();
            words.len() >= n
                && words[..n].iter().zip(h).all(|(w, h)| core(w) == *h)
                && words[..n - 1].iter().all(|w| tail(w).is_empty())
                && (words.len() == n || tail(words[n - 1]).contains([',', '.', '?', '!']))
        })
        .map_or(0, |h| h.len())
}

/// Removes fillers and stutters, then capitalises sentence starts.
// shortcut: single-word repeats only ("I think I think" stays) and no period is added to a sentence
// that lacks one; the speech models already punctuate. Upgrade if real dictation shows otherwise.
pub fn rules(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let c = core(w);
        let n = if FILLERS.contains(&c.as_str()) { 1 } else { hedge_len(&words[i..], out.last()) };
        if n > 0 {
            // The dropped words' sentence end survives: "ship it, um." -> "ship it."
            let end = tail(words[i + n - 1]);
            if let Some(p) = out.last_mut().filter(|p| !ends_sentence(p)) {
                if end.starts_with(SENTENCE_END) {
                    *p = format!("{}{end}", p.trim_end_matches(','));
                }
            }
            i += n;
            continue;
        }
        let repeats = out.last().is_some_and(|p| {
            tail(p).is_empty() && core(p) == c && !c.is_empty() && c.chars().all(|ch| ch.is_alphabetic() || matches!(ch, '\'' | '’'))
        });
        if repeats && !KEEP_DOUBLED.contains(&c.as_str()) {
            out.last_mut().unwrap().push_str(tail(w)); // "think think." -> "think."
        } else {
            out.push(w.to_string());
        }
        i += 1;
    }
    let mut start = true;
    for w in &mut out {
        if start {
            let at = lead(w).len();
            if let Some(c) = w[at..].chars().next() {
                w.replace_range(at..at + c.len_utf8(), &c.to_uppercase().to_string());
            }
        }
        start = ends_sentence(w) && !w[..w.len() - tail(w).len()].contains('.');
    }
    out.join(" ")
}

/// Replaces each entry's `from` (whole words, any case) with its `to`. Longest `from` wins at each spot.
pub fn apply_dictionary(text: &str, dict: &[Entry]) -> String {
    let mut entries: Vec<(Vec<String>, &str)> = dict
        .iter()
        .map(|e| (if e.from.trim().is_empty() { &e.to } else { &e.from }, e.to.trim()))
        .map(|(from, to)| (from.split_whitespace().map(core).collect::<Vec<_>>(), to))
        .filter(|(from, _)| from.iter().all(|w| !w.is_empty()) && !from.is_empty())
        .collect();
    entries.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let hit = entries.iter().find(|(from, _)| {
            words.len() - i >= from.len() && words[i..].iter().zip(from).all(|(w, f)| core(w) == *f)
        });
        match hit {
            Some((from, to)) => {
                let last = words[i + from.len() - 1];
                out.push(format!("{}{to}{}", lead(words[i]), tail(last)));
                i += from.len();
            }
            None => {
                out.push(words[i].to_string());
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// The whole dictation cleanup: rules, then the optional LLM pass, then the dictionary. The dictionary runs last so
/// its spelling beats both the sentence-start capital and whatever the model did to a name. `polish` gets text that
/// has something in it and answers None to keep it as it is (off, slow, or failed).
pub fn clean(text: &str, rules_on: bool, dict: &[Entry], polish: &dyn Fn(&str) -> Option<String>) -> String {
    let text = if rules_on { rules(text) } else { text.to_string() };
    let text = if text.is_empty() { text } else { polish(&text).unwrap_or(text) };
    apply_dictionary(&text, dict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(from: &str, to: &str) -> Entry {
        Entry { from: from.into(), to: to.into() }
    }

    #[test]
    fn rules_table() {
        let cases = [
            // the ticket's example, bare and as a punctuating model would write it
            ("um so I I think we should uh ship it", "So I think we should ship it"),
            ("Um, so I I think we should, uh, ship it.", "So I think we should, ship it."),
            // fillers
            ("uh", ""),
            ("Um.", ""),
            ("um uh erm", ""),
            ("this is, um, fine", "This is, fine"),
            ("we should ship it, um.", "We should ship it."),
            ("Really? Um.", "Really?"),
            ("Okay. Um, so then", "Okay. So then"),
            ("a human umbrella", "A human umbrella"),
            ("uh-huh, right", "Uh-huh, right"),
            // hedges go only when set off by punctuation
            ("It was, like, huge.", "It was, huge."),
            ("Like, I think so.", "I think so."),
            ("It's good, you know.", "It's good."),
            ("I like pizza", "I like pizza"),
            ("I don't like.", "I don't like."),
            ("Do you like, pizza", "Do you like, pizza"),
            ("You know what I mean", "You know what I mean"),
            ("Yes, like it.", "Yes, like it."),
            // stutters
            ("I I I think", "I think"),
            ("the the cat sat", "The cat sat"),
            ("The the cat", "The cat"),
            ("we went went home.", "We went home."),
            ("No, no, that's wrong", "No, no, that's wrong"),
            ("I know that that is true", "I know that that is true"),
            ("she had had enough", "She had had enough"),
            ("it's it's fine", "It's fine"),
            ("10 10 people", "10 10 people"),
            // capitals
            ("hello there. how are you? fine! ok", "Hello there. How are you? Fine! Ok"),
            ("he said e.g. the thing. then left", "He said e.g. the thing. Then left"),
            ("\"hello\" she said", "\"Hello\" she said"),
            ("élan is nice", "Élan is nice"),
            ("", ""),
            ("   ", ""),
            ("already Fine.", "Already Fine."),
        ];
        for (input, want) in cases {
            assert_eq!(rules(input), want, "input: {input:?}");
        }
    }

    #[test]
    fn dictionary_table() {
        let d = [
            entry("druv", "Dhruv"),
            entry("", "Kubernetes"),
            entry("my na", "Myna"),
            entry("see plus plus", "C++"),
            entry("iphone", "iPhone"),
            entry("", ""),
            entry("  ", "x"),
        ];
        let cases = [
            ("Druv said hi", "Dhruv said hi"),
            ("ask druv, then go", "ask Dhruv, then go"),
            ("kubernetes is big. (kubernetes)", "Kubernetes is big. (Kubernetes)"),
            ("the my na app", "the Myna app"),
            ("I write see plus plus.", "I write C++."),
            ("My iphone", "My iPhone"),
            ("druvs book", "druvs book"),
            ("nothing here", "nothing here"),
        ];
        for (input, want) in cases {
            assert_eq!(apply_dictionary(input, &d), want, "input: {input:?}");
        }
    }

    #[test]
    fn longest_entry_wins() {
        let d = [entry("my", "MY"), entry("my na", "Myna")];
        assert_eq!(apply_dictionary("my na and my", &d), "Myna and MY");
    }

    #[test]
    fn dictionary_spelling_beats_the_sentence_start_capital() {
        let d = [entry("", "iPhone")];
        assert_eq!(clean("um iphone is nice", true, &d, &|_| None), "iPhone is nice");
    }

    #[test]
    fn rules_can_be_switched_off_but_the_dictionary_still_runs() {
        let d = [entry("druv", "Dhruv")];
        assert_eq!(clean("um I I think druv", false, &d, &|_| None), "um I I think Dhruv");
        assert_eq!(clean("um I I think druv", true, &d, &|_| None), "I think Dhruv");
    }

    #[test]
    fn dictionary_setting_is_parsed_defensively() {
        assert_eq!(dictionary(None), vec![]);
        assert_eq!(dictionary(Some("not json".into())), vec![]);
        assert_eq!(dictionary(Some(r#"[{"to":"Myna"},{"from":"x","to":" "},{"from":"druv","to":"Dhruv"}]"#.into())), vec![entry("", "Myna"), entry("druv", "Dhruv")]);
    }

    #[test]
    fn the_chain_runs_rules_then_llm_then_dictionary() {
        let d = [entry("druv", "Dhruv")];
        // The model sees what the rules left (no "um", no stutter), and its own spelling of a name loses to the dictionary.
        let polish = |t: &str| {
            assert_eq!(t, "I think druv should ship it");
            Some("In my view Druv should ship it".to_string())
        };
        assert_eq!(clean("um I I think druv should ship it", true, &d, &polish), "In my view Dhruv should ship it");
    }

    #[test]
    fn a_pass_that_gives_nothing_back_leaves_the_local_text() {
        assert_eq!(clean("um hello there", true, &[], &|_| None), "Hello there");
        // A transcript that was all fillers never reaches the provider.
        assert_eq!(clean("um uh", true, &[], &|_| panic!("sent an empty text")), "");
    }
}
