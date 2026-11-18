//! Echo: "Me" lines that are the call coming back through the Mac's speakers into the mic, told from the user's own.
//! The audio decides first; when it can't, the words do. Spec: `.scratch/echo-bleed/spec.md`.

use crate::runtime::RATE;

// Where the real meeting (4, 2026-11-18, MacBook Air speakers) settled them; see `echo_real`.
const FRAME: usize = RATE as usize / 50; // 20 ms
const BLOCK: usize = 500; // frames: lag and coupling are found per 10 s block…
const REACH: usize = 6; // …from the blocks within ±60 s of it
const LAGS: std::ops::RangeInclusive<i64> = -15..=30; // frames the mic is behind the call: -300 to +600 ms
const MIN_FRAMES: f64 = 100.0; // a window needs 2 s of the call talking to say anything
const NO_BLEED: f64 = 0.2; // a block correlating less than this has no echo in it
const FLOOR_DB: f32 = 10.0; // a track is quiet under its 10th percentile plus this
const LOUD: f64 = 0.7; // the call's top 30% of frames set the coupling…
const NEAR_DB: f32 = 6.0; // …where the mic sits this close to the first guess
const SPREAD: usize = 2; // the echo of one frame rings into its neighbours: the prediction takes the loudest of ±40 ms
const LOCAL_DB: f32 = 9.0; // a mic frame this far over the predicted echo is the user
const KEEP: f64 = 0.5; // a line this local or more is the user's…
const ECHO: f64 = 0.2; // …this local or less is echo; between, the words decide
const WORDS_MS: i64 = 1500; // the call's lines within this of a line are what it may repeat
const SAME_WORDS: f64 = 0.6; // share of a line's words found, in order, in them
const SHORT: usize = 2; // a reply this short is echo if all its words are in them
/// Bumped when a better detector should re-check meetings this one judged.
pub const VERSION: u32 = 1;

/// One transcript line, as the detector needs it.
pub struct Said<'a> {
    pub start_ms: i64,
    pub end_ms: i64,
    pub me: bool,
    pub text: &'a str,
}

/// What a check found, stored as JSON in `meeting.echo`. Read with defaults so later fields can be added.
#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Summary {
    pub version: u32,
    pub headphones: bool,
    pub lag_ms: i64,
    pub corr: f32,
    /// "Me" lines judged echo.
    pub lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// Which of `said` are echo (never a call line), and the meeting's summary.
pub fn judge(mic: &[i16], them: &[i16], said: &[Said]) -> (Vec<bool>, Summary) {
    let mut echo = vec![false; said.len()];
    let skip = |why: &str| (vec![false; said.len()], Summary { version: VERSION, skipped: Some(why.into()), ..Default::default() });
    let n = mic.len().min(them.len()) / FRAME;
    if n == 0 {
        return skip("no call audio");
    }
    let (mic, call) = (db(&mic[..n * FRAME]), db(&them[..n * FRAME]));
    let (mic_floor, call_floor) = (floor(&mic), floor(&call));
    if call.iter().all(|&c| c < call_floor) || call.iter().all(|&c| c <= -119.0) {
        return skip("no call audio");
    }
    let blocks = blocks(&mic, &call, call_floor);
    let active: Vec<&Block> = blocks.iter().filter(|b| b.frames >= MIN_FRAMES).collect();
    let mut summary = Summary { version: VERSION, ..Default::default() };
    if !active.is_empty() {
        summary.corr = median(active.iter().map(|b| b.corr as f32).collect());
        summary.lag_ms = median(active.iter().map(|b| b.lag as f32).collect()) as i64 * 20;
    }
    summary.headphones = active.is_empty() || active.iter().filter(|b| b.corr < NO_BLEED).count() * 2 > active.len();
    if summary.headphones {
        return (echo, summary);
    }
    // Whether the user was talking in each frame.
    let local: Vec<bool> = (0..n)
        .map(|t| {
            let b = &blocks[t / BLOCK];
            if b.corr < NO_BLEED || b.frames < MIN_FRAMES {
                return true;
            }
            let at = t as i64 - b.lag;
            let near = (at - SPREAD as i64).max(0)..=(at + SPREAD as i64).min(n as i64 - 1);
            let heard = near.map(|i| call[i as usize]).fold(f32::MIN, f32::max);
            if heard < call_floor { mic[t] >= mic_floor } else { mic[t] >= heard + b.coupling + LOCAL_DB }
        })
        .collect();
    for (i, s) in said.iter().enumerate().filter(|(_, s)| s.me) {
        let (a, b) = (((s.start_ms / 20).max(0) as usize).min(n), ((s.end_ms / 20).max(0) as usize).min(n));
        let speech: Vec<bool> = (a..b).filter(|&t| mic[t] >= mic_floor).map(|t| local[t]).collect();
        let share = if speech.is_empty() { (KEEP + ECHO) / 2.0 } else { speech.iter().filter(|&&l| l).count() as f64 / speech.len() as f64 };
        echo[i] = share <= ECHO
            || share < KEEP && {
                let near: Vec<&str> = said.iter().filter(|c| !c.me && c.start_ms <= s.end_ms + WORDS_MS && c.end_ms >= s.start_ms - WORDS_MS).map(|c| c.text).collect();
                repeats(s.text, &near)
            };
    }
    summary.lines = echo.iter().filter(|&&e| e).count();
    (echo, summary)
}

/// Each 20 ms frame's level in dB.
fn db(s: &[i16]) -> Vec<f32> {
    s.chunks_exact(FRAME)
        .map(|f| {
            let e = f.iter().map(|&x| (x as f64 / 32768.0).powi(2)).sum::<f64>() / FRAME as f64;
            (10.0 * (e + 1e-12).log10()) as f32
        })
        .collect()
}

fn percentile(mut v: Vec<f32>, p: f64) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn median(v: Vec<f32>) -> f32 {
    percentile(v, 0.5)
}

/// Under this a track is quiet: its own quiet stretches plus a margin.
fn floor(track: &[f32]) -> f32 {
    percentile(track.to_vec(), 0.1) + FLOOR_DB
}

#[derive(Debug)]
struct Block {
    lag: i64,
    corr: f64,
    coupling: f32,
    frames: f64,
}

/// Lag, correlation and coupling for each 10 s block, from the ±60 s around it.
fn blocks(mic: &[f32], call: &[f32], call_floor: f32) -> Vec<Block> {
    let n = mic.len();
    let count = n.div_ceil(BLOCK);
    let lags: Vec<i64> = LAGS.collect();
    // Per block and lag: n, Σx, Σy, Σx², Σy², Σxy over frames where the call (x, `lag` frames earlier) talks.
    let mut sums = vec![[0f64; 6]; count * lags.len()];
    for t in 0..n {
        for (k, &lag) in lags.iter().enumerate() {
            let at = t as i64 - lag;
            if at < 0 || at >= n as i64 || call[at as usize] < call_floor {
                continue;
            }
            let (x, y) = (call[at as usize] as f64, mic[t] as f64);
            let s = &mut sums[t / BLOCK * lags.len() + k];
            *s = [s[0] + 1.0, s[1] + x, s[2] + y, s[3] + x * x, s[4] + y * y, s[5] + x * y];
        }
    }
    (0..count)
        .map(|b| {
            let near = b.saturating_sub(REACH)..(b + REACH + 1).min(count);
            let (k, corr, frames) = lags
                .iter()
                .enumerate()
                .map(|(k, _)| {
                    let mut s = [0f64; 6];
                    for j in near.clone() {
                        (0..6).for_each(|i| s[i] += sums[j * lags.len() + k][i]);
                    }
                    let [m, x, y, xx, yy, xy] = s;
                    let r = if m < MIN_FRAMES { 0.0 } else { (m * xy - x * y) / ((m * xx - x * x).max(1e-9) * (m * yy - y * y).max(1e-9)).sqrt() };
                    (k, r, m)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            let lag = lags[k];
            // The coupling: how much louder the mic is than the call it echoes, where the call is loud.
            let frames_near = near.start * BLOCK..(near.end * BLOCK).min(n);
            let pairs: Vec<(f32, f32)> = frames_near
                .filter_map(|t| {
                    let at = t as i64 - lag;
                    (at >= 0 && (at as usize) < n && call[at as usize] >= call_floor).then(|| (call[at as usize], mic[t] - call[at as usize]))
                })
                .collect();
            let loud = percentile(pairs.iter().map(|p| p.0).collect(), LOUD);
            let diffs: Vec<f32> = pairs.iter().filter(|p| p.0 >= loud).map(|p| p.1).collect();
            let first = median(diffs.clone());
            let coupling = median(diffs.into_iter().filter(|d| (d - first).abs() < NEAR_DB).collect());
            Block { lag, corr, coupling, frames }
        })
        .collect()
}

/// Words as both sides compare them: lowercase, no punctuation, numbers as digits ("one thirty three" is "133").
fn words(text: &str) -> Vec<String> {
    const SMALL: [&str; 20] = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen"];
    const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    let raw: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == '-')
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '\'').collect::<String>().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    let (mut out, mut num, mut part) = (vec![], String::new(), None::<u64>); // `num`: digits of the number words so far; `part`: the group being built
    // A group takes what follows when its slot is free: "thirty" a "three", "one hundred" a "thirty three".
    let takes = |p: u64, v: u64| (p >= 100 && p % 100 == 0 && v < 100) || (p % 100 >= 20 && p % 10 == 0 && v < 10);
    for w in raw {
        let v = SMALL.iter().position(|s| *s == w).or_else(|| TENS.iter().position(|s| *s == w).map(|t| 20 + 10 * t)).map(|v| v as u64);
        match (v, part) {
            (Some(v), Some(p)) if takes(p, v) => part = Some(p + v),
            (Some(v), _) => {
                num.extend(part.map(|p| p.to_string()));
                part = Some(v);
            }
            (None, Some(p)) if w == "hundred" && p < 10 => part = Some(p * 100),
            _ => {
                num.extend(part.take().map(|p| p.to_string()));
                if !num.is_empty() {
                    out.push(std::mem::take(&mut num));
                }
                out.push(w);
            }
        }
    }
    num.extend(part.map(|p| p.to_string()));
    if !num.is_empty() {
        out.push(num);
    }
    out
}

/// Whether `line` repeats the call's lines near it: most of its words, in order, or a short reply inside them.
fn repeats(line: &str, call: &[&str]) -> bool {
    let a = words(line);
    let b: Vec<String> = call.iter().flat_map(|c| words(c)).collect();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a.len() <= SHORT && a.iter().all(|w| b.contains(w)) {
        return true;
    }
    // Longest common subsequence, one row at a time.
    let mut row = vec![0usize; b.len() + 1];
    for x in &a {
        let mut prev = 0;
        for (j, y) in b.iter().enumerate() {
            let keep = row[j + 1];
            row[j + 1] = if x == y { prev + 1 } else { row[j + 1].max(row[j]) };
            prev = keep;
        }
    }
    row[b.len()] as f64 >= SAME_WORDS * a.len() as f64
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const SR: usize = RATE as usize;

    /// A repeatable noise source.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) - 1.0
        }
    }

    /// Speech-like bursts: noise at `amp`, swelling at 4 Hz like syllables, over each `(start s, length s)`.
    pub(crate) fn bursts(len_s: usize, at: &[(f32, f32)], amp: f32, seed: u64) -> Vec<f32> {
        let mut rng = Rng(seed);
        let mut out = vec![0f32; len_s * SR];
        for &(a, d) in at {
            for i in (a * SR as f32) as usize..((a + d) * SR as f32) as usize {
                let t = i as f32 / SR as f32;
                out[i] = amp * rng.next() * (0.55 + 0.45 * (t * 4.0 * std::f32::consts::TAU).sin());
            }
        }
        out
    }

    pub(crate) fn pcm(v: &[f32]) -> Vec<i16> {
        v.iter().map(|x| (x.clamp(-1.0, 1.0) * 32767.0) as i16).collect()
    }

    /// The mic: the call `delay` behind at `gain`, the user's own, and room noise.
    pub(crate) fn mic(call: &[f32], delay: impl Fn(usize) -> usize, gain: f32, own: &[f32]) -> Vec<i16> {
        let mut rng = Rng(99);
        pcm(&(0..call.len()).map(|i| i.checked_sub(delay(i)).map_or(0.0, |j| call[j]) * gain + own.get(i).copied().unwrap_or(0.0) + 0.002 * rng.next()).collect::<Vec<_>>())
    }

    /// The call talks 2 to 4 s, with 1 to 2 s between.
    pub(crate) fn turns(len_s: usize) -> Vec<(f32, f32)> {
        let mut out = vec![];
        let (mut t, mut k) = (1.0f32, 0);
        while t + 4.0 < len_s as f32 {
            let d = 2.0 + (k % 3) as f32;
            out.push((t, d));
            t += d + 1.0 + (k % 2) as f32;
            k += 1;
        }
        out
    }

    fn line(at: (f32, f32), shift_ms: i64, me: bool, text: &str) -> Said<'_> {
        Said { start_ms: (at.0 * 1000.0) as i64 + shift_ms, end_ms: ((at.0 + at.1) * 1000.0) as i64 + shift_ms, me, text }
    }

    const DELAY: usize = SR * 120 / 1000;

    #[test]
    fn echo_only_every_me_line_is_echo_and_the_lag_is_found() {
        let at = turns(120);
        let call = bursts(120, &at, 0.3, 1);
        let mic = mic(&call, |_| DELAY, 0.5, &[]);
        let texts: Vec<String> = (0..at.len()).map(|i| format!("the call said thing number {i} here")).collect();
        let mut said: Vec<Said> = at.iter().zip(&texts).map(|(&a, t)| line(a, 0, false, t)).collect();
        // The mic's transcript of the echo hears a different word now and then.
        let heard: Vec<String> = texts.iter().map(|t| t.replace("thing", "think")).collect();
        said.extend(at.iter().zip(&heard).map(|(&a, t)| line(a, 120, true, t)));
        let (echo, s) = judge(&mic, &pcm(&call), &said);
        assert!(echo[..at.len()].iter().all(|e| !e), "the call's own lines are never echo");
        assert!(echo[at.len()..].iter().all(|&e| e), "every Me line is the call: {echo:?}");
        assert!((s.lag_ms - 120).abs() <= 20, "lag {} ms", s.lag_ms);
        assert!(!s.headphones && s.corr > 0.5 && s.lines == at.len(), "{s:?}");
    }

    #[test]
    fn on_headphones_nothing_is_echo() {
        let at = turns(120);
        let call = bursts(120, &at, 0.3, 1);
        let own_at: Vec<(f32, f32)> = at.iter().map(|&(a, d)| (a + d + 0.1, 0.8)).collect();
        let own = bursts(120, &own_at, 0.2, 7);
        let mic = mic(&call, |_| DELAY, 0.0, &own);
        let mut said: Vec<Said> = at.iter().map(|&a| line(a, 0, false, "they talk")).collect();
        said.extend(own_at.iter().map(|&a| line(a, 0, true, "they talk")));
        let (echo, s) = judge(&mic, &pcm(&call), &said);
        assert!(echo.iter().all(|e| !e), "{echo:?}");
        assert!(s.headphones && s.corr < 0.2 && s.lines == 0, "{s:?}");
    }

    #[test]
    fn double_talk_keeps_the_users_lines() {
        let at = turns(120);
        let call = bursts(120, &at, 0.3, 1);
        // The user talks in some of the call's pauses and over the call twice, close to the mic.
        let own_at = [(at[3].0 + at[3].1 + 0.1, 0.8), (at[8].0 + 0.5, 1.5), (at[14].0 + at[14].1 + 0.1, 0.8), (at[20].0 + 0.5, 1.5)];
        let own = bursts(120, &own_at, 0.6, 7);
        let mic = mic(&call, |_| DELAY, 0.5, &own);
        let mut said: Vec<Said> = at.iter().map(|&a| line(a, 0, false, "we ship on the eighteenth")).collect();
        let echoes = said.len();
        said.extend(at.iter().map(|&a| line(a, 120, true, "we ship on the eighteenth")));
        let mine = said.len();
        said.extend(own_at.iter().map(|&a| line(a, 0, true, "hold on a second please")));
        let (echo, _) = judge(&mic, &pcm(&call), &said);
        assert!(echo[mine..].iter().all(|e| !e), "the user's lines are kept: {:?}", &echo[mine..]);
        // The echo lines the user talked over may go either way; the rest are echo.
        let clean = (echoes..mine).filter(|&i| ![8, 20].contains(&(i - echoes))).all(|i| echo[i]);
        assert!(clean, "{:?}", &echo[echoes..mine]);
    }

    #[test]
    fn a_lag_that_drifts_is_followed_block_by_block() {
        let len = 300;
        let call = bursts(len, &turns(len), 0.3, 1);
        let lag_at = |i: usize| SR * (80 + 80 * i / (len * SR)) / 1000; // 80 ms to 160 ms
        let mic = mic(&call, lag_at, 0.5, &[]);
        let (m, c) = (db(&mic), db(&pcm(&call)));
        for (k, b) in blocks(&m, &c, floor(&c)).iter().enumerate() {
            let truth = (80 + 80 * (k * BLOCK + BLOCK / 2) / (len * 50)) as i64;
            assert!((b.lag * 20 - truth).abs() <= 30, "block {k}: {} ms, truly {truth} ms", b.lag * 20);
        }
    }

    #[test]
    fn number_words_match_digits() {
        assert_eq!(words("It's one thirty-three, OK?"), ["it's", "133", "ok"]);
        assert_eq!(words("one hundred thirty three"), ["133"]);
        assert_eq!(words("twenty twenty six"), ["2026"]);
        assert!(repeats("we pay one thirty three now", &["so we pay 133 now, okay"]));
    }

    #[test]
    fn a_short_reply_inside_the_calls_words_repeats_it() {
        assert!(repeats("Yeah.", &["Yeah, that works for me."]));
        assert!(!repeats("Sure.", &["Yeah, that works for me."]));
    }

    #[test]
    fn a_line_sharing_only_some_words_is_the_users() {
        // 3 of 7 words, about 40%.
        assert!(!repeats("I think we should ship the beta", &["we ship it", "the plan"]));
        assert!(repeats("we should ship the beta", &["I think we should ship the beta today"]));
    }

    #[test]
    fn no_call_audio_is_skipped() {
        let mic = pcm(&bursts(30, &[(1.0, 2.0)], 0.3, 1));
        let said = [line((1.0, 2.0), 0, true, "hello")];
        assert_eq!(judge(&mic, &[], &said).1.skipped.as_deref(), Some("no call audio"));
        assert_eq!(judge(&mic, &vec![0; mic.len()], &said).0, [false]);
    }

    /// The real meeting: `.scratch/echo-bleed/probe/fixture.sh 4`, then
    /// `MYNA_ECHO_FIXTURE=.scratch/echo-bleed/fixtures/4 cargo test --release --lib -- --ignored echo_real --nocapture`.
    #[test]
    #[ignore]
    fn echo_real() {
        let dir = std::path::PathBuf::from(std::env::var("MYNA_ECHO_FIXTURE").expect("MYNA_ECHO_FIXTURE"));
        let read = |f: &str| hound::WavReader::open(dir.join(f)).unwrap().into_samples::<i16>().map(Result::unwrap).collect::<Vec<_>>();
        let (mic, them) = (read("mic.wav"), read("them.wav"));
        let json = |f: &str| serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(dir.join(f)).unwrap()).unwrap();
        let rows = json("lines.json");
        let mine: Vec<i64> = json("expect.json").iter().map(|v| v["start_ms"].as_i64().unwrap()).collect();
        let said: Vec<Said> = rows
            .iter()
            .map(|r| Said { start_ms: r["start_ms"].as_i64().unwrap(), end_ms: r["end_ms"].as_i64().unwrap(), me: r["speaker"] == "Me", text: r["text"].as_str().unwrap() })
            .collect();
        let t = std::time::Instant::now();
        let (echo, s) = judge(&mic, &them, &said);
        let took = t.elapsed();
        let repeated: Vec<usize> = (0..said.len()).filter(|&i| said[i].me && !mine.contains(&said[i].start_ms)).collect();
        let caught = repeated.iter().filter(|&&i| echo[i]).count();
        let kept: Vec<i64> = mine.iter().copied().filter(|m| said.iter().zip(&echo).any(|(s, &e)| s.me && s.start_ms == *m && !e)).collect();
        println!("{s:?} in {took:?}: {caught} of {} repeated lines judged echo, {} of {} of the user's lines kept", repeated.len(), kept.len(), mine.len());
        for (_, s) in said.iter().enumerate().filter(|(i, s)| s.me && repeated.contains(i) && !echo[*i]) {
            println!("  missed {} {:?}", s.start_ms, s.text);
        }
        assert!(caught as f64 >= 0.95 * repeated.len() as f64);
        // Not asserted: on meeting 4 the 7 lines `expect.json` calls the user's are call audio the call's transcript
        // missed (no Them line 108.5 to 121.4 s). The mic follows the call through them at +120 ms, never 9 dB over
        // the predicted echo, so they are judged echo, rightly. Hand-label `expect.json` before asserting on it.
        println!("the user's lines kept: {kept:?}");
    }
}
