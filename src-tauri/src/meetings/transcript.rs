//! From two recorded tracks to a Me / Them transcript, and from a transcript to Markdown or plain text.
use crate::dictation::{Transcriber, Utterance};
use super::{
    mute::{self, Event, State},
    store::{self, Meeting, Segment, ME, THEM},
    track::{read_track, RATE},
};
use rusqlite::Connection;
use std::{ops::Range, path::Path, sync::Mutex};

// The model's memory grows with input length, so a track goes through in pieces. Each cut is placed at
// the quietest 100 ms in the last 5 s of a piece so it rarely lands inside a word.
const CHUNK: usize = 30 * RATE as usize;
const SEARCH: usize = 5 * RATE as usize;
const STEP: usize = RATE as usize / 10;
/// Same-speaker lines closer than this read as one paragraph.
const MERGE_GAP_MS: u64 = 2000;

pub fn plan_chunks(samples: &[i16]) -> Vec<Range<usize>> {
    let energy = |w: &[i16]| w.iter().map(|&s| (s as i64) * (s as i64)).sum::<i64>();
    let (mut out, mut start) = (vec![], 0);
    while samples.len() - start > CHUNK {
        let quietest = (start + CHUNK - SEARCH..=start + CHUNK - STEP)
            .step_by(STEP)
            .min_by_key(|&w| energy(&samples[w..w + STEP]))
            .unwrap();
        let cut = quietest + STEP / 2;
        out.push(start..cut);
        start = cut;
    }
    out.push(start..samples.len());
    out
}

/// Digital silence (an idle tap) and slivers too short for the model are not worth a pass.
fn skip(chunk: &[i16]) -> bool {
    let sum: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
    chunk.len() < RATE as usize / 3 || (sum / chunk.len() as f64).sqrt() < 5.0
}

/// `progress` is told how many samples each piece covered, whether or not it was transcribed.
pub fn transcribe_track(
    samples: &[i16],
    t: &mut dyn Transcriber,
    mut progress: impl FnMut(usize),
) -> Result<Vec<Utterance>, String> {
    let mut all = vec![];
    for range in plan_chunks(samples) {
        let chunk = &samples[range.clone()];
        if !skip(chunk) {
            let floats: Vec<f32> = chunk.iter().map(|&s| s as f32 / 32768.0).collect();
            let offset = range.start as u64 * 1000 / RATE as u64;
            for mut u in t.transcribe_timed(&floats)? {
                u.start_ms += offset;
                u.end_ms += offset;
                all.push(u);
            }
        }
        progress(chunk.len());
    }
    Ok(all)
}

/// Interleaves both speakers by start time and joins runs of one speaker into paragraphs. Each mic line gets its
/// mute state first, and lines only join when their state matches, so a muted aside never merges into a live one.
pub fn assemble(me: Vec<Utterance>, them: Vec<Utterance>, mutes: &[Event], always: bool) -> Vec<Segment> {
    let tag = |speaker: &'static str| {
        move |u: Utterance| Segment {
            speaker: speaker.into(),
            start_ms: u.start_ms,
            end_ms: u.end_ms,
            text: u.text.trim().into(),
            mute: if speaker == ME { mute::state(mutes, u.start_ms, u.end_ms, always) } else { State::Unmuted },
            ..Default::default()
        }
    };
    let mut all: Vec<Segment> =
        me.into_iter().map(tag(ME)).chain(them.into_iter().map(tag(THEM))).filter(|s| !s.text.is_empty()).collect();
    all.sort_by_key(|s| s.start_ms);
    let mut out: Vec<Segment> = vec![];
    for s in all {
        match out.last_mut() {
            Some(prev) if prev.speaker == s.speaker && prev.mute == s.mute && s.start_ms.saturating_sub(prev.end_ms) <= MERGE_GAP_MS => {
                prev.text.push(' ');
                prev.text.push_str(&s.text);
                prev.end_ms = prev.end_ms.max(s.end_ms);
            }
            _ => out.push(s),
        }
    }
    out
}

/// Transcribes a recorded meeting's tracks and stores the segments. The caller records a failure.
pub fn run(db: &Mutex<Connection>, audio: &Path, id: i64, t: &mut dyn Transcriber) -> Result<(), String> {
    let dir = audio.join(id.to_string());
    let mic = read_track(&dir.join("mic.wav")).map_err(|e| format!("No microphone audio to transcribe ({e})"))?;
    let system = dir.join("system.wav");
    let them = if system.exists() { read_track(&system)? } else { vec![] };
    let total = (mic.len() + them.len()).max(1);
    let mut done = 0;
    let mut report = |n: usize| {
        done += n;
        store::set_progress(&db.lock().unwrap(), id, done as f64 / total as f64);
    };
    let me_lines = transcribe_track(&mic, t, &mut report)?;
    let them_lines = transcribe_track(&them, t, &mut report)?;
    let duration_ms = mic.len() as u64 * 1000 / RATE as u64;
    // A timeline that cannot be read fails the meeting (retryable) instead of letting every line in.
    let (mutes, always) = {
        let conn = db.lock().unwrap();
        (store::mute_events(&conn, id).map_err(|e| e.to_string())?, mute::always_record(&conn, id))
    };
    let mut segments = assemble(me_lines, them_lines, &mutes, always);
    // The optional LLM pass, outside the database lock. Only lines that are in the transcript go to a provider, and
    // the first one it cannot do (slow, down) ends the pass, so a dead network costs one timeout, not one per line.
    for s in segments.iter_mut().filter(|s| s.shown()) {
        match t.polish(&s.text) {
            Some(text) => s.text = text,
            None => break,
        }
    }
    store::finish(&db.lock().unwrap(), id, duration_ms, &segments).map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Markdown,
    Text,
}

impl Format {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "md" => Some(Self::Markdown),
            "txt" => Some(Self::Text),
            _ => None,
        }
    }
    fn ext(self) -> &'static str {
        if self == Self::Markdown { "md" } else { "txt" }
    }
}

pub fn label(speaker: &str) -> &'static str {
    if speaker == ME { "Me" } else { "Them" }
}

/// `mm:ss`, or `h:mm:ss` past an hour. Same shape as the margin time in the transcript view.
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{:02}:{:02}", s / 60, s % 60) }
}

fn local_tm(ts: i64) -> libc::tm {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm
}

fn when(ts: i64) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let tm = local_tm(ts);
    format!(
        "{} {} {} {}, {:02}:{:02}",
        DAYS[tm.tm_wday as usize], tm.tm_mday, MONTHS[tm.tm_mon as usize], 1900 + tm.tm_year, tm.tm_hour, tm.tm_min
    )
}

fn minutes(ms: u64) -> String {
    if ms < 60_000 { "<1 min".into() } else { format!("{} min", (ms + 30_000) / 60_000) }
}

pub fn export(m: &Meeting, segments: &[Segment], format: Format) -> String {
    let meta = format!("{} · {} · {}", when(m.started_at), minutes(m.duration_ms), m.app.as_deref().unwrap_or("Mic only"));
    let mut out = match format {
        Format::Markdown => format!("# {}\n\n{meta}\n\n", m.title),
        Format::Text => format!("{}\n{meta}\n\n", m.title),
    };
    for s in segments.iter().filter(|s| s.shown()) {
        let (who, at) = (label(&s.speaker), clock(s.start_ms));
        out += &match format {
            Format::Markdown => format!("**{who}** · {at}\n\n{}\n\n", s.text),
            Format::Text => format!("[{at}] {who}: {}\n\n", s.text),
        };
    }
    out.truncate(out.trim_end().len());
    out + "\n"
}

pub fn file_name(m: &Meeting, format: Format) -> String {
    let tm = local_tm(m.started_at);
    let title: String = m.title.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
    format!(
        "{title} {}-{:02}-{:02} {:02}{:02}.{}",
        1900 + tm.tm_year, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, format.ext()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dictation, models};

    fn utt(start_ms: u64, end_ms: u64, text: &str) -> Utterance {
        Utterance { start_ms, end_ms, text: text.into() }
    }

    /// Loud noise with a silent gap at `gap_ms`, so cuts have an obvious place to go.
    fn noise_with_gap(secs: usize, gap_ms: usize) -> Vec<i16> {
        let mut s: Vec<i16> = (0..secs * RATE as usize).map(|i| if i % 2 == 0 { 9000 } else { -9000 }).collect();
        let at = gap_ms * RATE as usize / 1000;
        s[at..at + 400 * RATE as usize / 1000].fill(0);
        s
    }

    #[test]
    fn long_track_is_cut_in_the_quiet_gap_and_nothing_is_lost() {
        let samples = noise_with_gap(70, 27_000);
        let chunks = plan_chunks(&samples);
        assert_eq!(chunks.len(), 3);
        assert!((27_000..27_400).contains(&(chunks[0].end * 1000 / RATE as usize)), "{:?}", chunks[0]);
        assert_eq!(chunks[0].start, 0);
        assert_eq!(chunks.last().unwrap().end, samples.len());
        assert!(chunks.windows(2).all(|w| w[0].end == w[1].start));
        assert!(chunks.iter().all(|c| c.len() <= CHUNK));
    }

    struct EachChunk(Vec<usize>);
    impl Transcriber for EachChunk {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, String> {
            unreachable!("meetings ask for timestamps")
        }
        fn transcribe_timed(&mut self, samples: &[f32]) -> Result<Vec<Utterance>, String> {
            self.0.push(samples.len());
            Ok(vec![utt(100, 900, "line")])
        }
    }

    #[test]
    fn utterance_times_are_shifted_by_their_chunk_start_and_silence_is_skipped() {
        // Loud 0-26 s, silent 26-56 s, loud again: the middle chunk has nothing to transcribe.
        let quiet = 26 * RATE as usize..56 * RATE as usize;
        let samples: Vec<i16> = (0..70 * RATE as usize)
            .map(|i| if quiet.contains(&i) { 0 } else if i % 2 == 0 { 9000 } else { -9000 })
            .collect();
        let plan = plan_chunks(&samples);
        assert_eq!(plan.len(), 3);
        let mut t = EachChunk(vec![]);
        let mut covered = 0;
        let lines = transcribe_track(&samples, &mut t, |n| covered += n).unwrap();
        assert_eq!(covered, samples.len());
        assert_eq!(t.0.len(), 2, "silent chunk must not reach the model");
        let third = plan[2].start as u64 * 1000 / RATE as u64;
        assert_eq!(lines.iter().map(|u| u.start_ms).collect::<Vec<_>>(), [100, third + 100]);
    }

    #[test]
    fn assemble_interleaves_speakers_and_joins_close_same_speaker_lines() {
        let me = vec![utt(0, 3000, "Hi there."), utt(4000, 6000, "How are you?"), utt(30_000, 31_000, "Bye.")];
        let them = vec![utt(7000, 9000, "Good."), utt(9500, 10_000, "You?"), utt(12_000, 12_500, "  ")];
        let live = [Event { at_ms: 0, muted: false, source: mute::Source::Manual }];
        let got: Vec<_> = assemble(me, them, &live, false).into_iter().map(|s| (s.speaker, s.start_ms, s.text)).collect();
        assert_eq!(
            got,
            [
                ("me".into(), 0, "Hi there. How are you?".to_string()),
                ("them".into(), 7000, "Good. You?".into()),
                ("me".into(), 30_000, "Bye.".into()),
            ]
        );
    }

    fn live(at_ms: u64) -> Event {
        Event { at_ms, muted: false, source: mute::Source::Manual }
    }

    fn off(at_ms: u64) -> Event {
        Event { at_ms, muted: true, source: mute::Source::Manual }
    }

    #[test]
    fn a_muted_aside_gets_its_state_and_never_merges_into_the_live_lines_around_it() {
        let me = vec![utt(0, 1000, "Live."), utt(1500, 2500, "Private."), utt(3000, 4000, "Back."), utt(4500, 5000, "More.")];
        let got: Vec<_> = assemble(me, vec![], &[live(0), off(1200), live(2800)], false)
            .into_iter()
            .map(|s| (s.text, s.mute))
            .collect();
        assert_eq!(
            got,
            [("Live.".into(), State::Unmuted), ("Private.".into(), State::Muted), ("Back. More.".into(), State::Unmuted)]
        );
    }

    #[test]
    fn export_leaves_out_collapsed_segments_unless_the_user_included_them() {
        let line = |speaker: &str, start_ms, text: &str, mute, included| Segment {
            speaker: speaker.into(), start_ms, end_ms: start_ms + 1000, text: text.into(), mute, included, ..Default::default()
        };
        let segments = [
            line(ME, 0, "Live line.", State::Unmuted, false),
            line(ME, 2000, "Private aside.", State::Muted, false),
            line(ME, 4000, "Unknown line.", State::Unknown, false),
            line(ME, 6000, "Pulled in.", State::Muted, true),
            line(THEM, 8000, "Their line.", State::Unmuted, false),
        ];
        for format in [Format::Markdown, Format::Text] {
            let out = export(&meeting(), &segments, format);
            for kept in ["Live line.", "Pulled in.", "Their line."] {
                assert!(out.contains(kept), "{kept} missing from {out}");
            }
            for dropped in ["Private aside.", "Unknown line."] {
                assert!(!out.contains(dropped), "{dropped} leaked into {out}");
            }
        }
    }

    #[test]
    fn clock_switches_to_hours() {
        assert_eq!([clock(5_000), clock(754_000), clock(3_725_000)], ["00:05", "12:34", "1:02:05"]);
    }

    fn meeting() -> Meeting {
        Meeting {
            id: 1, title: "Design: review/2".into(), started_at: 1_793_000_000, duration_ms: 2_520_000, app: Some("Google Chrome".into()),
            note: None, status: "done".into(), progress: 1.0, error: None, segments: 2, has_them: true, folder_id: None,
        }
    }

    fn lines() -> Vec<Segment> {
        vec![
            Segment { speaker: ME.into(), start_ms: 12_000, end_ms: 15_000, text: "Hello.".into(), ..Default::default() },
            Segment { speaker: THEM.into(), start_ms: 20_000, end_ms: 24_000, text: "Hi, thanks for joining.".into(), ..Default::default() },
        ]
    }

    #[test]
    fn markdown_and_text_export_carry_speaker_time_and_text() {
        let md = export(&meeting(), &lines(), Format::Markdown);
        assert!(md.starts_with("# Design: review/2\n\n"));
        assert!(md.contains(" · 42 min · Google Chrome\n"));
        assert!(md.ends_with("**Me** · 00:12\n\nHello.\n\n**Them** · 00:20\n\nHi, thanks for joining.\n"));
        let txt = export(&meeting(), &lines(), Format::Text);
        assert!(txt.starts_with("Design: review/2\n"));
        assert!(txt.ends_with("[00:12] Me: Hello.\n\n[00:20] Them: Hi, thanks for joining.\n"));
    }

    #[test]
    fn file_name_is_safe_and_has_the_right_extension() {
        let name = file_name(&meeting(), Format::Markdown);
        assert!(name.starts_with("Design- review-2 20") && name.ends_with(".md"), "{name}");
        assert!(!name.contains('/') && !name.contains(':'));
    }

    // The command-layer test: two fixture tracks in, speaker-labelled timed segments out.
    struct Scripted;
    impl Transcriber for Scripted {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, String> {
            unreachable!("meetings ask for timestamps")
        }
        fn transcribe_timed(&mut self, samples: &[f32]) -> Result<Vec<Utterance>, String> {
            let peak = samples.iter().fold(0f32, |m, x| m.max(x.abs()));
            Ok(if peak > 0.4 { vec![utt(200, 1200, "hello from me")] } else { vec![utt(1500, 2500, "hi from them")] })
        }
    }

    fn fixture(name: &str, amplitude: f32, tracks: &[&str]) -> (Mutex<Connection>, std::path::PathBuf, i64) {
        let root = std::env::temp_dir().join(format!("myna-meet-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let conn = Connection::open_in_memory().unwrap();
        store::init(&conn).unwrap();
        let id = store::create(&conn, 1_793_000_000).unwrap();
        store::set_transcribing(&conn, id).unwrap();
        let dir = root.join(id.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        for (i, track) in tracks.iter().enumerate() {
            let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
            let mut w = hound::WavWriter::create(dir.join(track), spec).unwrap();
            let amp = if i == 0 { 0.5 } else { amplitude };
            for n in 0..3 * RATE {
                let x = (2.0 * std::f32::consts::PI * 300.0 * n as f32 / RATE as f32).sin();
                w.write_sample((x * amp * 32767.0) as i16).unwrap();
            }
            w.finalize().unwrap();
        }
        (Mutex::new(conn), root, id)
    }

    #[test]
    fn two_tracks_become_me_and_them_segments_in_time_order() {
        let (db, root, id) = fixture("both", 0.2, &["mic.wav", "system.wav"]);
        run(&db, &root, id, &mut Scripted).unwrap();
        let conn = db.lock().unwrap();
        let m = store::get(&conn, id).unwrap();
        assert_eq!((m.status.as_str(), m.duration_ms, m.progress), ("done", 3000, 1.0));
        let got = store::segments(&conn, id).unwrap();
        let want = [(ME, 200, 1200, "hello from me"), (THEM, 1500, 2500, "hi from them")];
        assert_eq!(got.len(), 2);
        for (g, w) in got.iter().zip(want) {
            assert_eq!((g.speaker.as_str(), g.start_ms, g.end_ms, g.text.as_str()), w);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn transcription_assigns_each_mic_line_its_state_from_the_stored_timeline_and_policy() {
        // (events, zoom is on the always-record list, state of the mic line)
        let table = [
            ("no mute source", vec![], false, State::Unknown),
            ("no mute source, always record", vec![], true, State::Unmuted),
            ("live", vec![live(0)], false, State::Unmuted),
            ("muted partway through the line", vec![live(0), off(1000), live(2000)], false, State::Muted),
            ("muted, always record", vec![off(0)], true, State::Muted),
        ];
        for (i, (name, events, always, want)) in table.into_iter().enumerate() {
            let (db, root, id) = fixture(&format!("state{i}"), 0.2, &["mic.wav", "system.wav"]);
            {
                let conn = db.lock().unwrap();
                conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
                store::set_source(&conn, id, Some("zoom.us"), None).unwrap();
                for e in &events {
                    store::add_mute_event(&conn, id, e).unwrap();
                }
                if always {
                    crate::settings::set(&conn, mute::ALWAYS_KEY, r#"["zoom.us"]"#).unwrap();
                }
            }
            run(&db, &root, id, &mut Scripted).unwrap();
            let got = store::segments(&db.lock().unwrap(), id).unwrap();
            let state_of = |who| got.iter().find(|s| s.speaker == who).unwrap().mute;
            assert_eq!((state_of(ME), state_of(THEM)), (want, State::Unmuted), "{name}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// `Scripted` plus the LLM pass: shouts each line it is given, or fails after `ok` lines.
    struct Polishing {
        asked: std::cell::RefCell<Vec<String>>,
        ok: usize,
    }
    impl Transcriber for Polishing {
        fn transcribe(&mut self, s: &[f32]) -> Result<String, String> {
            Scripted.transcribe(s)
        }
        fn transcribe_timed(&mut self, s: &[f32]) -> Result<Vec<Utterance>, String> {
            Scripted.transcribe_timed(s)
        }
        fn polish(&self, text: &str) -> Option<String> {
            self.asked.borrow_mut().push(text.into());
            (self.asked.borrow().len() <= self.ok).then(|| text.to_uppercase())
        }
    }

    #[test]
    fn the_llm_pass_runs_after_transcription_on_lines_in_the_transcript_only() {
        let (db, root, id) = fixture("polish", 0.2, &["mic.wav", "system.wav"]);
        {
            // The mic is muted: that line is collapsed, so it must never reach a provider.
            let conn = db.lock().unwrap();
            conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
            store::add_mute_event(&conn, id, &off(0)).unwrap();
        }
        let mut t = Polishing { asked: Default::default(), ok: 9 };
        run(&db, &root, id, &mut t).unwrap();
        assert_eq!(*t.asked.borrow(), ["hi from them"]);
        let got = store::segments(&db.lock().unwrap(), id).unwrap();
        let text_of = |who| got.iter().find(|s| s.speaker == who).unwrap().text.clone();
        assert_eq!((text_of(ME).as_str(), text_of(THEM).as_str()), ("hello from me", "HI FROM THEM"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_first_line_the_provider_cannot_do_ends_the_llm_pass_and_keeps_the_local_text() {
        let (db, root, id) = fixture("polish-down", 0.2, &["mic.wav", "system.wav"]);
        {
            let conn = db.lock().unwrap();
            conn.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)", []).unwrap();
            store::add_mute_event(&conn, id, &live(0)).unwrap();
        }
        let mut t = Polishing { asked: Default::default(), ok: 0 };
        run(&db, &root, id, &mut t).unwrap();
        assert_eq!(t.asked.borrow().len(), 1, "one timeout, not one per line");
        let texts: Vec<_> = store::segments(&db.lock().unwrap(), id).unwrap().into_iter().map(|s| s.text).collect();
        assert_eq!(texts, ["hello from me", "hi from them"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mic_only_meeting_still_transcribes() {
        let (db, root, id) = fixture("mic", 0.2, &["mic.wav"]);
        run(&db, &root, id, &mut Scripted).unwrap();
        let conn = db.lock().unwrap();
        assert!(!store::get(&conn, id).unwrap().has_them);
        assert_eq!(store::segments(&conn, id).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_mic_track_is_an_error_not_an_empty_transcript() {
        let (db, root, id) = fixture("none", 0.2, &[]);
        let err = run(&db, &root, id, &mut Scripted).unwrap_err();
        assert!(err.contains("No microphone audio"), "{err}");
        assert_eq!(store::get(&db.lock().unwrap(), id).unwrap().status, "transcribing");
        let _ = std::fs::remove_dir_all(&root);
    }

    // Slow: downloads the ~670 MB model on first run (cached in the temp dir) and uses macOS `say` for speech.
    // `cargo test --lib real_parakeet -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_parakeet_transcribes_synthesized_speech() {
        let root = std::env::temp_dir().join("myna-test-models");
        let m = models::MODELS.iter().find(|m| m.id == crate::stt::PARAKEET).unwrap();
        if models::status(&root, m) != models::Status::Installed {
            std::fs::create_dir_all(&root).unwrap();
            models::install(&root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
        }
        let dir = std::env::temp_dir().join(format!("myna-stt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let said = "Hello everyone. Let's review the roadmap for next quarter. The first item is the meeting capture feature.";
        let run = |cmd: &str, args: &[&str]| assert!(std::process::Command::new(cmd).args(args).status().unwrap().success());
        let (aiff, wav, wav48) = (dir.join("s.aiff"), dir.join("s.wav"), dir.join("s48.wav"));
        run("say", &["-o", aiff.to_str().unwrap(), said]);
        run("afconvert", &["-f", "WAVE", "-d", "LEI16@16000", "-c", "1", aiff.to_str().unwrap(), wav.to_str().unwrap()]);
        run("afconvert", &["-f", "WAVE", "-d", "LEI16@48000", "-c", "1", aiff.to_str().unwrap(), wav48.to_str().unwrap()]);

        // The same speech as a 48 kHz device would deliver it, through the live writer's resampler.
        let live = dir.join("live.wav");
        let mut writer = super::super::track::TrackWriter::create(&live, 48_000).unwrap();
        let tx = writer.sender();
        let mut r = hound::WavReader::open(&wav48).unwrap();
        let wide: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        wide.chunks(480).for_each(|c| tx.send(c.to_vec()).unwrap());
        writer.finish().unwrap();

        let mut stt = dictation::Parakeet::load(&root.join(crate::stt::PARAKEET)).unwrap();
        for path in [&wav, &live] {
            let samples = read_track(path).unwrap();
            let lines = transcribe_track(&samples, &mut stt, |_| {}).unwrap();
            let segments = assemble(lines, vec![], &[Event { at_ms: 0, muted: false, source: mute::Source::Manual }], false);
            let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
            eprintln!("{path:?}: {segments:#?}");
            let hits = ["hello", "everyone", "roadmap", "quarter", "meeting", "capture", "feature"].iter().filter(|w| text.contains(*w)).count();
            assert!(hits >= 5, "heard: {text}");
            assert!(segments.iter().all(|s| s.end_ms >= s.start_ms && s.end_ms <= samples.len() as u64 * 1000 / 16_000 + 500));
        }
    }
}
