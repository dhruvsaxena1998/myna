//! Experimental triage: for each collapsed (muted or unknown) mic segment, the decision provider is asked a yes/no
//! question and a yes pulls the segment into the transcript, marked as automatic. The request carries that one
//! segment's text and the two nearest lines that are already in the transcript, never audio and never another
//! collapsed segment.
use super::{
    store::Segment,
    transcript::label,
};
use crate::decision::DecisionProvider;

const QUESTION: &str = "Is the line to judge part of the meeting's conversation, rather than private talk, \
    self-talk or something unrelated to the meeting? Answer yes only if it belongs in the meeting's transcript.";
// Laya reads about 1,000 tokens, so a long paragraph is cut.
const CLIP: usize = 600;

fn clip(text: &str) -> String {
    text.chars().take(CLIP).collect()
}

/// The text sent for `segments[i]`: the line, then the nearest shown lines on each side.
fn context(segments: &[Segment], shown: &[bool], i: usize) -> String {
    let near = |s: Option<&Segment>, what: &str| s.map(|s| format!("{what}: {}: {}\n", label(&s.speaker), clip(&s.text))).unwrap_or_default();
    let before = (0..i).rev().find(|&j| shown[j]).map(|j| &segments[j]);
    let after = (i + 1..segments.len()).find(|&j| shown[j]).map(|j| &segments[j]);
    format!("{}Line to judge: {}\n{}", near(before, "Previous line"), clip(&segments[i].text), near(after, "Next line"))
}

/// Asks about every collapsed segment. The first question the provider cannot answer ends the pass and leaves the
/// rest collapsed, so a dead provider costs one failure, not one per line.
pub fn pass(segments: &mut [Segment], provider: &dyn DecisionProvider) {
    // Neighbours are judged on what was in the transcript before the pass, so one yes does not vouch for the next.
    let shown: Vec<bool> = segments.iter().map(Segment::shown).collect();
    for i in 0..segments.len() {
        if shown[i] {
            continue;
        }
        match provider.decide(QUESTION, &context(segments, &shown, i)) {
            Ok(true) => (segments[i].included, segments[i].auto) = (true, true),
            Ok(false) => {}
            Err(e) => return eprintln!("Triage stopped: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai::stub,
        decision::{adapter, Wire},
        meetings::{
            mute::State,
            store::{ME, THEM},
        },
    };
    use std::sync::Mutex;

    fn seg(speaker: &str, text: &str, mute: State) -> Segment {
        Segment { speaker: speaker.into(), text: text.into(), mute, ..Default::default() }
    }

    /// Answers yes to any context mentioning "budget", and records every context it was asked.
    struct Keyword(Mutex<Vec<String>>);
    impl DecisionProvider for Keyword {
        fn decide(&self, question: &str, context: &str) -> Result<bool, String> {
            assert_eq!(question, QUESTION);
            self.0.lock().unwrap().push(context.into());
            Ok(context.contains("Line to judge: The budget"))
        }
    }

    fn call() -> Vec<Segment> {
        vec![
            seg(THEM, "Where did the numbers land?", State::Unmuted),
            seg(ME, "The budget is forty thousand.", State::Muted),
            seg(ME, "Ugh, my cat is on the keyboard.", State::Muted),
            seg(THEM, "Got it, thanks.", State::Unmuted),
            seg(ME, "And another private aside.", State::Unknown),
        ]
    }

    #[test]
    fn a_yes_includes_the_segment_marked_and_a_no_leaves_it_collapsed() {
        let mut segments = call();
        let p = Keyword(Default::default());
        pass(&mut segments, &p);
        assert_eq!(segments.iter().map(|s| (s.included, s.auto, s.shown())).collect::<Vec<_>>(), [
            (false, false, true), (true, true, true), (false, false, false), (false, false, true), (false, false, false),
        ]);
    }

    #[test]
    fn each_question_carries_one_segment_and_its_two_neighbouring_included_lines_only() {
        let mut segments = call();
        let p = Keyword(Default::default());
        pass(&mut segments, &p);
        let asked = p.0.lock().unwrap();
        assert_eq!(asked.len(), 3, "only collapsed segments are asked about");
        assert_eq!(asked[0], "Previous line: Them: Where did the numbers land?\nLine to judge: The budget is forty thousand.\nNext line: Them: Got it, thanks.\n");
        // The neighbour is the nearest shown line, so the other collapsed segment never rides along; and the segment that
        // was just included does not become the next one's neighbour.
        assert_eq!(asked[1], "Previous line: Them: Where did the numbers land?\nLine to judge: Ugh, my cat is on the keyboard.\nNext line: Them: Got it, thanks.\n");
        assert_eq!(asked[2], "Previous line: Them: Got it, thanks.\nLine to judge: And another private aside.\n");
        assert!(asked.iter().all(|c| c.matches("Line to judge").count() == 1));
    }

    #[test]
    fn nothing_is_asked_when_every_segment_is_already_in_the_transcript() {
        let mut segments = vec![seg(ME, "Live.", State::Unmuted), Segment { included: true, ..seg(ME, "Pulled in.", State::Muted) }];
        let p = Keyword(Default::default());
        pass(&mut segments, &p);
        assert!(p.0.lock().unwrap().is_empty());
        assert!(!segments[1].auto, "the user's own inclusion is not relabelled");
    }

    struct Down(Mutex<usize>);
    impl DecisionProvider for Down {
        fn decide(&self, _: &str, _: &str) -> Result<bool, String> {
            *self.0.lock().unwrap() += 1;
            Err("down".into())
        }
    }

    #[test]
    fn the_first_failure_ends_the_pass_with_everything_still_collapsed() {
        let mut segments = call();
        let p = Down(Default::default());
        pass(&mut segments, &p);
        assert_eq!(*p.0.lock().unwrap(), 1);
        assert!(segments.iter().all(|s| !s.included && !s.auto));
    }

    /// The whole path to a provider over HTTP: what leaves the machine is the question and the allowed lines.
    #[test]
    fn the_request_a_provider_receives_has_no_other_muted_text() {
        let mut segments = vec![
            seg(THEM, "Shall we start?", State::Unmuted),
            seg(ME, "Yes, the launch is Friday.", State::Muted),
            seg(ME, "Secret: my salary is high.", State::Muted),
        ];
        let (base, seen) = stub::serve(vec![(200, r#"{"choice":"yes"}"#.into()), (200, r#"{"choice":"no"}"#.into())]);
        pass(&mut segments, adapter(Wire::Jev, &base, Some("k".into()), "").as_ref());
        assert_eq!((segments[1].included, segments[2].included), (true, false));
        let first_body = seen.recv().unwrap().body;
        stub::assert_text_only(&first_body); // a triage request is text only, never audio, video or a frame
        let first: serde_json::Value = serde_json::from_str(&first_body).unwrap();
        let sent = first["context"].as_str().unwrap();
        assert!(sent.contains("Yes, the launch is Friday.") && sent.contains("Shall we start?"), "{sent}");
        assert!(!sent.contains("salary"), "{sent}");
        let second_body = seen.recv().unwrap().body;
        stub::assert_text_only(&second_body);
        let second: serde_json::Value = serde_json::from_str(&second_body).unwrap();
        assert!(!second["context"].as_str().unwrap().contains("launch"), "{second}");
    }

    /// A speaker the user has named stays in the speakers table; the request carries only "Them".
    #[test]
    fn a_renamed_speaker_is_never_named_in_the_request() {
        use crate::meetings::{speakers, store};
        let db = rusqlite::Connection::open_in_memory().unwrap();
        store::init(&db).unwrap();
        speakers::init(&db).unwrap();
        let id = store::create(&db, 1).unwrap();
        let them = |text: &str, cluster| Segment { cluster: Some(cluster), ..seg(THEM, text, State::Unmuted) };
        let lines = [them("Shall we start?", 1), seg(ME, "The budget is forty thousand.", State::Muted), them("Got it.", 2)];
        store::finish(&db, id, 1, &lines).unwrap();
        let people = [1, 2].map(|cluster| speakers::Speaker { cluster, name: None, suggested: None, from_captions: false, print: None });
        speakers::replace(&db, id, &people).unwrap();
        speakers::rename(&db, id, 1, "Priya Natarajan", false).unwrap();
        speakers::rename(&db, id, 2, "Quentin", false).unwrap();

        let mut segments = store::segments(&db, id).unwrap();
        let (base, seen) = stub::serve(vec![(200, r#"{"choice":"yes"}"#.into())]);
        pass(&mut segments, adapter(Wire::Jev, &base, Some("k".into()), "").as_ref());
        assert!(segments[1].auto, "the pass ran");
        let body = seen.recv().unwrap().body;
        stub::assert_text_only(&body);
        assert!(body.contains("Them: Shall we start?"), "{body}");
        assert!(!body.contains("Priya") && !body.contains("Quentin") && !body.contains("Speaker 1"), "{body}");
    }

    #[test]
    fn a_caption_name_never_goes_into_a_triage_question() {
        use crate::meetings::{captions, store};
        let db = rusqlite::Connection::open_in_memory().unwrap();
        store::init(&db).unwrap();
        let id = store::create(&db, 1).unwrap();
        let them = |text: &str| Segment { cluster: Some(1), ..seg(THEM, text, State::Unmuted) };
        store::finish(&db, id, 1, &[them("Shall we start?"), seg(ME, "The budget is forty thousand.", State::Muted)]).unwrap();
        captions::plant(&db, id, "Lena Ström");
        let mut segments = store::segments(&db, id).unwrap();
        let p = Keyword(Default::default());
        pass(&mut segments, &p);
        let asked = p.0.lock().unwrap().join("\n");
        assert!(asked.contains("Them: Shall we start?") && !asked.contains("Lena") && !asked.contains("Ström"), "{asked}");
    }

    #[test]
    fn long_text_is_cut() {
        let long = "x".repeat(5000);
        let segments = vec![seg(ME, &long, State::Muted)];
        assert!(context(&segments, &[false], 0).len() < 700);
    }
}
