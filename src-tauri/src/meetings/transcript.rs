//! From two recorded tracks to a Me / Them transcript, and from a transcript to Markdown or plain text.
use crate::{
    dictation::{Transcriber, Utterance},
    diarize::{self, Diarizer},
};
use super::{
    mute::{self, Event, State},
    speakers::{self, Speaker},
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
/// Each line from the system track comes with its speaker (see `diarize::assign`); lines only join within one.
pub fn assemble(me: Vec<Utterance>, them: Vec<(Utterance, Option<u32>)>, mutes: &[Event], always: bool) -> Vec<Segment> {
    let tag = |speaker: &'static str| {
        move |(u, cluster): (Utterance, Option<u32>)| Segment {
            speaker: speaker.into(),
            start_ms: u.start_ms,
            end_ms: u.end_ms,
            text: u.text.trim().into(),
            mute: if speaker == ME { mute::state(mutes, u.start_ms, u.end_ms, always) } else { State::Unmuted },
            cluster,
            ..Default::default()
        }
    };
    let mut all: Vec<Segment> = me
        .into_iter()
        .map(|u| (u, None))
        .map(tag(ME))
        .chain(them.into_iter().map(tag(THEM)))
        .filter(|s| !s.text.is_empty())
        .collect();
    all.sort_by_key(|s| s.start_ms);
    let mut out: Vec<Segment> = vec![];
    for s in all {
        match out.last_mut() {
            Some(prev)
                if prev.speaker == s.speaker
                    && prev.cluster == s.cluster
                    && prev.mute == s.mute
                    && s.start_ms.saturating_sub(prev.end_ms) <= MERGE_GAP_MS =>
            {
                prev.text.push(' ');
                prev.text.push_str(&s.text);
                prev.end_ms = prev.end_ms.max(s.end_ms);
            }
            _ => out.push(s),
        }
    }
    out
}

/// Which speaker each line on the system track belongs to, and the speakers' voice prints. A diarizer that
/// can't load or fails costs the speaker labels, not the transcript: the lines stay "Them" and the meeting says why.
fn separate(
    db: &Mutex<Connection>,
    id: i64,
    track: &[i16],
    lines: &[Utterance],
    diarizer: impl FnOnce() -> Result<Box<dyn Diarizer>, String>,
) -> (Vec<Option<u32>>, Vec<Speaker>) {
    if lines.is_empty() {
        return (vec![], vec![]);
    }
    let floats: Vec<f32> = track.iter().map(|&s| s as f32 / 32768.0).collect();
    let spans: Vec<(u64, u64)> = lines.iter().map(|u| (u.start_ms, u.end_ms)).collect();
    match diarizer().and_then(|mut d| d.diarize(&floats)) {
        Ok(turns) => {
            let clusters = diarize::assign(&spans, &turns);
            let profiles = speakers::profiles(&db.lock().unwrap());
            let speakers = speakers::build(&floats, &spans, &clusters, &profiles);
            (clusters, speakers)
        }
        Err(e) => {
            store::add_note(&db.lock().unwrap(), id, &format!("Speakers weren't separated: {e}"));
            (vec![None; lines.len()], vec![])
        }
    }
}

/// Transcribes a recorded meeting's tracks, tells the system track's speakers apart and stores the segments.
/// `diarizer` is only called when the system track has speech. The caller records a failure.
pub fn run(
    db: &Mutex<Connection>,
    audio: &Path,
    id: i64,
    t: &mut dyn Transcriber,
    diarizer: impl FnOnce() -> Result<Box<dyn Diarizer>, String>,
) -> Result<(), String> {
    let dir = audio.join(id.to_string());
    let mic = read_track(&dir.join("mic.wav")).map_err(|e| format!("No microphone audio to transcribe ({e})"))?;
    let system = dir.join("system.wav");
    let them = if system.exists() { read_track(&system)? } else { vec![] };
    // The system track goes through twice: once for its words, once for its speakers.
    let total = (mic.len() + them.len() * 2).max(1);
    let mut done = 0;
    let mut report = |n: usize| {
        done += n;
        store::set_progress(&db.lock().unwrap(), id, done as f64 / total as f64);
    };
    let me_lines = transcribe_track(&mic, t, &mut report)?;
    let them_lines = transcribe_track(&them, t, &mut report)?;
    let (clusters, speakers) = separate(db, id, &them, &them_lines, diarizer);
    report(them.len());
    let duration_ms = mic.len() as u64 * 1000 / RATE as u64;
    let conn = db.lock().unwrap();
    // A timeline that cannot be read fails the meeting (retryable) instead of letting every line in.
    let mutes = store::mute_events(&conn, id).map_err(|e| e.to_string())?;
    let them_lines = them_lines.into_iter().zip(clusters.into_iter().chain(std::iter::repeat(None))).collect();
    let segments = assemble(me_lines, them_lines, &mutes, mute::always_record(&conn, id));
    speakers::replace(&conn, id, &speakers).map_err(|e| e.to_string())?;
    store::finish(&conn, id, duration_ms, &segments).map_err(|e| e.to_string())
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

/// Who a line is from: Me, the name the user gave the speaker, or "Speaker N" until they do; "Them" when the
/// system track's speakers weren't separated.
fn who(s: &Segment, speakers: &[Speaker]) -> String {
    match s.cluster {
        Some(c) if s.speaker == THEM => {
            speakers.iter().find(|p| p.cluster == c).and_then(|p| p.name.clone()).unwrap_or_else(|| format!("Speaker {c}"))
        }
        _ => label(&s.speaker).into(),
    }
}

/// What goes to an AI provider: lines from `shown()` only, and speakers as Me, "Speaker N" or Them, never the
/// names the user gave them.
pub fn export(m: &Meeting, segments: &[Segment], format: Format) -> String {
    export_named(m, segments, &[], format)
}

/// The user's own copy, with the names they gave their speakers.
pub fn export_named(m: &Meeting, segments: &[Segment], speakers: &[Speaker], format: Format) -> String {
    let meta = format!("{} · {} · {}", when(m.started_at), minutes(m.duration_ms), m.app.as_deref().unwrap_or("Mic only"));
    let mut out = match format {
        Format::Markdown => format!("# {}\n\n{meta}\n\n", m.title),
        Format::Text => format!("{}\n{meta}\n\n", m.title),
    };
    for s in segments.iter().filter(|s| s.shown()) {
        let (who, at) = (who(s, speakers), clock(s.start_ms));
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
    use crate::{dictation, diarize::Turn, meetings::voiceprint::tests::voice, models};

    fn utt(start_ms: u64, end_ms: u64, text: &str) -> Utterance {
        Utterance { start_ms, end_ms, text: text.into() }
    }

    /// Lines from the system track with no speaker separation.
    fn unsplit(lines: Vec<Utterance>) -> Vec<(Utterance, Option<u32>)> {
        lines.into_iter().map(|u| (u, None)).collect()
    }

    /// No speaker model in this test: the meeting still transcribes.
    fn no_diarizer() -> Result<Box<dyn Diarizer>, String> {
        Err("The Who model isn't downloaded.".into())
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
        let got: Vec<_> = assemble(me, unsplit(them), &live, false).into_iter().map(|s| (s.speaker, s.start_ms, s.text)).collect();
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
        speakers::init(&conn).unwrap();
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
        run(&db, &root, id, &mut Scripted, no_diarizer).unwrap();
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
            run(&db, &root, id, &mut Scripted, no_diarizer).unwrap();
            let got = store::segments(&db.lock().unwrap(), id).unwrap();
            let state_of = |who| got.iter().find(|s| s.speaker == who).unwrap().mute;
            assert_eq!((state_of(ME), state_of(THEM)), (want, State::Unmuted), "{name}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    // The command-layer test for speakers: a fake diarizer over a synthetic two-voice call. Clusters come out
    // numbered by who speaks first, a rename shows on every line of that speaker, a merge folds a split speaker back,
    // and a voice the user asked to remember is suggested in the next meeting.
    struct Scripted3(Vec<Turn>);
    impl Diarizer for Scripted3 {
        fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String> {
            assert!(audio.len() >= 12 * RATE as usize, "the whole system track goes through");
            Ok(self.0.clone())
        }
    }

    /// Six lines of six seconds on the system track, one per voice given; the mic hears one short line.
    struct SixSeconds;
    impl Transcriber for SixSeconds {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, String> {
            unreachable!("meetings ask for timestamps")
        }
        fn transcribe_timed(&mut self, samples: &[f32]) -> Result<Vec<Utterance>, String> {
            let n = samples.len() as u64 / (6 * RATE as u64);
            Ok(if n == 0 { vec![utt(200, 1200, "hello from me")] } else { (0..n).map(|i| utt(i * 6000 + 200, i * 6000 + 5800, &format!("line {i}"))).collect() })
        }
    }

    /// Adds a recorded meeting to `db`: a short mic line, and a system track of the voices in turn, six seconds each
    /// (person 0 or 1; the seed varies what is said, as a new meeting would).
    fn add_call(db: &Mutex<Connection>, root: &Path, who: &[(u32, u64)]) -> i64 {
        let conn = db.lock().unwrap();
        let id = store::create(&conn, 1_793_000_000).unwrap();
        store::set_transcribing(&conn, id).unwrap();
        store::add_mute_event(&conn, id, &live(0)).unwrap(); // a mic line with no mute source would be collapsed out of the transcript
        let dir = root.join(id.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut mic = hound::WavWriter::create(dir.join("mic.wav"), spec).unwrap();
        (0..RATE).for_each(|n| mic.write_sample(((2.0 * std::f32::consts::PI * 300.0 * n as f32 / RATE as f32).sin() * 16_000.0) as i16).unwrap());
        mic.finalize().unwrap();
        let mut system = hound::WavWriter::create(dir.join("system.wav"), spec).unwrap();
        for &(person, seed) in who {
            let samples = if person == 0 { voice(110.0, [700.0, 1200.0], seed, 6) } else { voice(210.0, [450.0, 2300.0], seed, 6) };
            samples.iter().for_each(|&x| system.write_sample((x * 32767.0) as i16).unwrap());
        }
        system.finalize().unwrap();
        id
    }

    fn them_clusters(conn: &Connection, id: i64) -> Vec<Option<u32>> {
        store::segments(conn, id).unwrap().iter().filter(|s| s.speaker == THEM).map(|s| s.cluster).collect()
    }

    #[test]
    fn speakers_are_separated_renamed_merged_and_remembered_into_the_next_meeting() {
        let root = std::env::temp_dir().join(format!("myna-meet-{}-speakers", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let db = Mutex::new(Connection::open_in_memory().unwrap());
        store::init(&db.lock().unwrap()).unwrap();
        speakers::init(&db.lock().unwrap()).unwrap();

        // Person 0, person 1, person 0 again. The backend's own numbering (7 and 3) means nothing to the transcript.
        let id = add_call(&db, &root, &[(0, 1), (1, 2), (0, 3)]);
        let turns = vec![Turn { start_ms: 0, end_ms: 6000, speaker: 7 }, Turn { start_ms: 6000, end_ms: 12_000, speaker: 3 }, Turn { start_ms: 12_000, end_ms: 18_000, speaker: 7 }];
        run(&db, &root, id, &mut SixSeconds, || Ok(Box::new(Scripted3(turns)))).unwrap();
        let conn = db.lock().unwrap();
        assert_eq!(them_clusters(&conn, id), [Some(1), Some(2), Some(1)]);
        let got = store::segments(&conn, id).unwrap();
        assert_eq!(got.iter().find(|s| s.speaker == ME).unwrap().cluster, None, "the mic is always Me");
        let people = speakers::list(&conn, id).unwrap();
        assert_eq!(people.iter().map(|s| (s.cluster, s.name.clone(), s.suggested.clone(), s.print.is_some())).collect::<Vec<_>>(), [(1, None, None, true), (2, None, None, true)]);

        // Rename once; every line of that speaker reads the new name, and the other speaker does not.
        let m = store::get(&conn, id).unwrap();
        assert!(speakers::rename(&conn, id, 1, "Priya", true).unwrap());
        let named = export_named(&m, &store::segments(&conn, id).unwrap(), &speakers::list(&conn, id).unwrap(), Format::Text);
        assert_eq!(named.matches("Priya:").count(), 2, "{named}");
        assert!(named.contains("Speaker 2: line 1") && named.contains("Me: hello from me"), "{named}");
        // What goes to a provider never carries the name.
        let sent = export(&m, &store::segments(&conn, id).unwrap(), Format::Text);
        assert!(!sent.contains("Priya") && sent.contains("Speaker 1: line 0") && sent.contains("Speaker 2: line 1"), "{sent}");

        // A speaker split in two is folded back together.
        speakers::merge(&conn, id, 2, 1).unwrap();
        assert_eq!(them_clusters(&conn, id), [Some(1); 3]);
        assert_eq!(speakers::list(&conn, id).unwrap().len(), 1);
        drop(conn);

        // The next call: person 1 first, then Priya, each saying new things. Only Priya's voice is suggested.
        let next = add_call(&db, &root, &[(1, 12), (0, 13)]);
        let turns = vec![Turn { start_ms: 0, end_ms: 6000, speaker: 0 }, Turn { start_ms: 6000, end_ms: 12_000, speaker: 1 }];
        run(&db, &root, next, &mut SixSeconds, || Ok(Box::new(Scripted3(turns)))).unwrap();
        let conn = db.lock().unwrap();
        let heard: Vec<_> = speakers::list(&conn, next).unwrap().iter().map(|s| (s.name.clone(), s.suggested.clone())).collect();
        assert_eq!(heard, [(None, None), (None, Some("Priya".to_string()))]);
        // Suggested, not applied: the line still reads "Speaker 2" until the user confirms.
        let text = |conn: &Connection| {
            let m = store::get(conn, next).unwrap();
            export_named(&m, &store::segments(conn, next).unwrap(), &speakers::list(conn, next).unwrap(), Format::Text)
        };
        assert!(text(&conn).contains("Speaker 2: line 1"), "{}", text(&conn));
        speakers::rename(&conn, next, 2, "Priya", false).unwrap();
        assert!(text(&conn).contains("Priya: line 1"), "{}", text(&conn));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_diarizer_that_cannot_run_costs_the_labels_not_the_transcript() {
        let (db, root, id) = fixture("nodiar", 0.2, &["mic.wav", "system.wav"]);
        for _ in 0..2 {
            run(&db, &root, id, &mut Scripted, no_diarizer).unwrap();
        }
        let conn = db.lock().unwrap();
        let m = store::get(&conn, id).unwrap();
        assert_eq!(m.status, "done");
        // Said once, however many times it was retried.
        assert_eq!(m.note.as_deref(), Some("Speakers weren't separated: The Who model isn't downloaded."));
        assert_eq!(them_clusters(&conn, id), [None]);
        assert!(speakers::list(&conn, id).unwrap().is_empty());
        assert!(export(&m, &store::segments(&conn, id).unwrap(), Format::Text).contains("Them: hi from them"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failing_diarizer_is_reported_like_a_missing_one() {
        struct Broken;
        impl Diarizer for Broken {
            fn diarize(&mut self, _: &[f32]) -> Result<Vec<Turn>, String> {
                Err("Core ML said no.".into())
            }
        }
        let (db, root, id) = fixture("brokendiar", 0.2, &["mic.wav", "system.wav"]);
        run(&db, &root, id, &mut Scripted, || Ok(Box::new(Broken))).unwrap();
        let conn = db.lock().unwrap();
        assert_eq!(store::get(&conn, id).unwrap().note.as_deref(), Some("Speakers weren't separated: Core ML said no."));
        assert_eq!(store::get(&conn, id).unwrap().status, "done");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lines_only_join_within_one_speaker() {
        let them = vec![(utt(0, 2000, "One."), Some(1)), (utt(2500, 4000, "Two."), Some(2)), (utt(4200, 5000, "Three."), Some(2)), (utt(5500, 6000, "Four."), None)];
        let got: Vec<_> = assemble(vec![], them, &[], false).into_iter().map(|s| (s.cluster, s.text)).collect();
        assert_eq!(got, [(Some(1), "One.".to_string()), (Some(2), "Two. Three.".into()), (None, "Four.".into())]);
    }

    #[test]
    fn mic_only_meeting_still_transcribes() {
        let (db, root, id) = fixture("mic", 0.2, &["mic.wav"]);
        run(&db, &root, id, &mut Scripted, || unreachable!("no system track, nothing to separate")).unwrap();
        let conn = db.lock().unwrap();
        assert!(!store::get(&conn, id).unwrap().has_them);
        assert_eq!(store::segments(&conn, id).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_mic_track_is_an_error_not_an_empty_transcript() {
        let (db, root, id) = fixture("none", 0.2, &[]);
        let err = run(&db, &root, id, &mut Scripted, no_diarizer).unwrap_err();
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
