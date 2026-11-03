//! On-device model runtimes behind the `Transcriber` and `Diarizer` traits.
//! Audio in is 16 kHz mono f32; times out are seconds from the start of that audio.

use crate::models;
use parakeet_rs::sortformer::{DiarizationConfig, Sortformer};
use parakeet_rs::{ExecutionConfig, ParakeetTDT, TimestampMode, Transcriber as _};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

pub const RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Segment {
    pub start: f32,
    pub end: f32,
    pub text: String,
}

#[cfg_attr(not(test), allow(dead_code))] // first caller is Meetings (story 05)
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start: f32,
    pub end: f32,
    pub speaker: usize,
}

pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<Segment>, String>;
    /// Nudges the next `transcribe` toward these words' spelling. Only Whisper takes one.
    fn bias(&mut self, _words: &str) {}
}

#[cfg_attr(not(test), allow(dead_code))] // first caller is Meetings (story 05)
pub trait Diarizer: Send {
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String>;
}

/// Loads the speech-to-text model `id` from its folder (`models/<id>/`).
pub fn transcriber(id: &str, dir: &Path) -> Result<Box<dyn Transcriber>, String> {
    let m = models::find(id).ok_or_else(|| format!("Unknown model {id}"))?;
    let err = |e: String| format!("Couldn't load {}: {e}", m.name);
    match id {
        "parakeet-tdt-v3" => {
            Ok(Box::new(Parakeet(ParakeetTDT::from_pretrained(dir, Some(ort()?)).map_err(|e| err(e.to_string()))?)))
        }
        "whisper-large-v3-turbo" => {
            let ctx = WhisperContext::new_with_params(first_file(&m, dir), WhisperContextParameters::default())
                .map_err(|e| err(e.to_string()))?;
            Ok(Box::new(Whisper(ctx.create_state().map_err(|e| err(e.to_string()))?, String::new())))
        }
        "apple-speech" => Ok(Box::new(AppleSpeech)),
        _ => Err(format!("{} can't run in Myna yet", m.name)),
    }
}

#[cfg_attr(not(test), allow(dead_code))] // first caller is Meetings (story 05)
/// Loads the diarization model `id` from its folder (`models/<id>/`).
pub fn diarizer(id: &str, dir: &Path) -> Result<Box<dyn Diarizer>, String> {
    let m = models::find(id).ok_or_else(|| format!("Unknown model {id}"))?;
    match id {
        "sortformer" => Ok(Box::new(Sort(
            Sortformer::with_config(first_file(&m, dir), Some(ort()?), DiarizationConfig::default())
                .map_err(|e| format!("Couldn't load {}: {e}", m.name))?,
        ))),
        _ => Err(format!("{} can't run in Myna yet", m.name)),
    }
}

fn first_file(m: &models::Model, dir: &Path) -> PathBuf {
    dir.join(m.files.first().map_or("", |f| f.path.as_str()))
}

/// ONNX Runtime allows one environment per process; every model shares it.
// shortcut: CPU only, try the Core ML execution provider when speed on long meetings matters.
fn ort() -> Result<ExecutionConfig, String> {
    static ENV: Mutex<Option<parakeet_rs::ort::environment::Environment>> = Mutex::new(None);
    let mut env = ENV.lock().unwrap();
    if env.is_none() {
        *env = Some(parakeet_rs::ort::init().with_name("myna").build().map_err(|e| format!("Couldn't start ONNX Runtime: {e}"))?);
    }
    Ok(ExecutionConfig::new().with_environment(env.clone().unwrap()))
}

// shortcut: Parakeet TDT takes about 4 to 5 minutes of audio per call; meetings must chunk before this (story 05).
struct Parakeet(ParakeetTDT);

impl Transcriber for Parakeet {
    fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<Segment>, String> {
        let r = self.0.transcribe_samples(audio.to_vec(), RATE, 1, Some(TimestampMode::Sentences)).map_err(|e| e.to_string())?;
        Ok(r.tokens.into_iter().map(|t| Segment { start: t.start, end: t.end, text: t.text.trim().to_string() }).collect())
    }
}

/// The model state and the initial prompt.
struct Whisper(WhisperState, String);

impl Transcriber for Whisper {
    fn bias(&mut self, words: &str) {
        self.1 = words.to_string();
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<Segment>, String> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some("auto"));
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_special(false);
        p.set_print_timestamps(false);
        if !self.1.is_empty() {
            p.set_initial_prompt(&self.1);
        }
        self.0.full(p, audio).map_err(|e| e.to_string())?;
        self.0
            .as_iter()
            .map(|s| {
                let text = s.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
                // whisper.cpp timestamps are in centiseconds
                Ok(Segment { start: s.start_timestamp() as f32 / 100.0, end: s.end_timestamp() as f32 / 100.0, text })
            })
            .collect()
    }
}

/// macOS holds the model, so there's nothing to load or unload.
struct AppleSpeech;

impl Transcriber for AppleSpeech {
    fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<Segment>, String> {
        crate::apple::transcribe(audio)
    }
}

#[cfg_attr(not(test), allow(dead_code))] // first caller is Meetings (story 05)
struct Sort(Sortformer);

impl Diarizer for Sort {
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String> {
        let segs = self.0.diarize(audio.to_vec(), RATE, 1).map_err(|e| e.to_string())?;
        let secs = |n: u64| n as f32 / RATE as f32;
        Ok(segs.into_iter().map(|s| Turn { start: secs(s.start), end: secs(s.end), speaker: s.speaker_id }).collect())
    }
}

/// Scripted output, so pipeline tests run in milliseconds without model downloads.
#[cfg(test)]
pub struct Fake<T>(pub Vec<T>);

#[cfg(test)]
impl Transcriber for Fake<Segment> {
    fn transcribe(&mut self, _: &[f32]) -> Result<Vec<Segment>, String> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
impl Diarizer for Fake<Turn> {
    fn diarize(&mut self, _: &[f32]) -> Result<Vec<Turn>, String> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_unsupported_models_say_so() {
        let empty = tempfile::tempdir().unwrap();
        for id in ["parakeet-tdt-v3", "whisper-large-v3-turbo"] {
            let err = transcriber(id, empty.path()).err().unwrap();
            assert!(err.starts_with("Couldn't load"), "{id}: {err}");
        }
        assert!(diarizer("sortformer", empty.path()).err().unwrap().starts_with("Couldn't load Sortformer"));
        assert_eq!(transcriber("voz", empty.path()).err().unwrap(), "Desert Ant Voz can't run in Myna yet");
        assert_eq!(diarizer("nope", empty.path()).err().unwrap(), "Unknown model nope");
    }

    #[test]
    fn fakes_return_their_script() {
        let seg = Segment { start: 0.0, end: 1.0, text: "hi".into() };
        let mut t: Box<dyn Transcriber> = Box::new(Fake(vec![seg.clone()]));
        assert_eq!(t.transcribe(&[]).unwrap(), [seg]);
        let turn = Turn { start: 0.0, end: 1.0, speaker: 0 };
        let mut d: Box<dyn Diarizer> = Box::new(Fake(vec![turn.clone()]));
        assert_eq!(d.diarize(&[]).unwrap(), [turn]);
    }

    // Slow tests: real models on a 9 s fixture (two macOS `say` voices, switch at ~4.6 s).
    // Run with `cargo test --release -- --ignored`. Models come from MYNA_MODELS or the app's models folder.

    const SAID: &str = "The quarterly report is ready. Sales grew in every region except the north. \
                        Thanks. Let's review the numbers on Monday and plan the next steps.";

    fn models_dir() -> PathBuf {
        std::env::var_os("MYNA_MODELS").map(PathBuf::from).unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap()).join("Library/Application Support/Myna/models")
        })
    }

    fn fixture() -> Vec<f32> {
        let r = hound::WavReader::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two-speakers.wav")).unwrap();
        assert_eq!((r.spec().sample_rate, r.spec().channels), (RATE, 1));
        r.into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    }

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace()
            .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// Word error rate: word-level edit distance over the reference length.
    fn wer(reference: &str, heard: &str) -> f32 {
        let (r, h) = (words(reference), words(heard));
        let mut row: Vec<usize> = (0..=h.len()).collect();
        for (i, rw) in r.iter().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, hw) in h.iter().enumerate() {
                let cur = (prev + (rw != hw) as usize).min(row[j] + 1).min(row[j + 1] + 1);
                prev = row[j + 1];
                row[j + 1] = cur;
            }
        }
        row[h.len()] as f32 / r.len() as f32
    }

    #[test]
    fn wer_counts_edits() {
        assert_eq!(wer("the cat sat", "The cat, sat."), 0.0);
        assert_eq!(wer("the cat sat", "the dog sat down"), 2.0 / 3.0);
    }

    fn transcribes_the_fixture(id: &str) {
        let mut t = transcriber(id, &models_dir().join(id)).unwrap();
        let heard: Vec<_> = t.transcribe(&fixture()).unwrap().into_iter().map(|s| s.text).collect();
        let heard = heard.join(" ");
        assert!(wer(SAID, &heard) < 0.2, "{id} heard: {heard}");
    }

    #[test]
    #[ignore = "slow: needs the downloaded Parakeet model"]
    fn parakeet_transcribes() {
        transcribes_the_fixture("parakeet-tdt-v3");
    }

    #[test]
    #[ignore = "slow: needs the downloaded Whisper model"]
    fn whisper_transcribes() {
        transcribes_the_fixture("whisper-large-v3-turbo");
    }

    #[test]
    #[ignore = "slow: needs macOS 26 and Apple's model for the Mac's language"]
    fn apple_speech_transcribes() {
        transcribes_the_fixture("apple-speech");
    }

    #[test]
    #[ignore = "slow: needs the downloaded Sortformer model"]
    fn sortformer_tells_the_two_voices_apart() {
        let turns = diarizer("sortformer", &models_dir().join("sortformer")).unwrap().diarize(&fixture()).unwrap();
        let at = |t: f32| turns.iter().find(|s| s.start <= t && t <= s.end).map(|s| s.speaker);
        let (first, second) = (at(2.0), at(7.0));
        assert!(first.is_some() && second.is_some() && first != second, "{turns:?}");
    }
}
