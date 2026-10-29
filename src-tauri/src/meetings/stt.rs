//! Parakeet TDT through parakeet-rs, behind the `Transcriber` trait.
use super::transcript::{Transcriber, Utterance};
use crate::models;
use parakeet_rs::{ExecutionConfig, ParakeetTDT, TimestampMode, Transcriber as _};
use std::{path::Path, sync::OnceLock};

const MODEL: &str = "parakeet-tdt-0.6b-v3";

// shortcut: Parakeet is the only backend, so the `model_stt` choice in Settings is ignored here;
// pick the backend from that setting when Whisper lands (ticket 05).
pub struct Parakeet(ParakeetTDT);

// ONNX Runtime wants one environment per process, shared by every model.
fn exec_config() -> Result<ExecutionConfig, String> {
    static CONFIG: OnceLock<ExecutionConfig> = OnceLock::new();
    if let Some(c) = CONFIG.get() {
        return Ok(c.clone());
    }
    let env = parakeet_rs::ort::init().build().map_err(|e| e.to_string())?;
    Ok(CONFIG.get_or_init(|| ExecutionConfig::new().with_environment(env)).clone())
}

pub fn load(models_dir: &Path) -> Result<Parakeet, String> {
    let m = models::MODELS.iter().find(|m| m.id == MODEL).expect("curated model list has Parakeet");
    if models::status(models_dir, m) != models::Status::Installed {
        return Err(format!("Download {} under Settings → Models, then retry.", m.name));
    }
    ParakeetTDT::from_pretrained(models_dir.join(MODEL), Some(exec_config()?))
        .map(Parakeet)
        .map_err(|e| e.to_string())
}

impl Transcriber for Parakeet {
    fn transcribe(&mut self, samples: &[f32]) -> Result<Vec<Utterance>, String> {
        let r = self
            .0
            .transcribe_samples(samples.to_vec(), super::track::RATE, 1, Some(TimestampMode::Sentences))
            .map_err(|e| e.to_string())?;
        Ok(r.tokens
            .into_iter()
            .map(|t| Utterance { start_ms: (t.start * 1000.0) as u64, end_ms: (t.end * 1000.0) as u64, text: t.text })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meetings::{track, transcript};

    // Slow: downloads the ~670 MB model on first run (cached in the temp dir) and uses macOS `say` for speech.
    // `cargo test --lib real_parakeet -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_parakeet_transcribes_synthesized_speech() {
        let root = std::env::temp_dir().join("myna-test-models");
        let m = models::MODELS.iter().find(|m| m.id == MODEL).unwrap();
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
        let mut writer = track::TrackWriter::create(&live, 48_000).unwrap();
        let tx = writer.sender();
        let mut r = hound::WavReader::open(&wav48).unwrap();
        let wide: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        wide.chunks(480).for_each(|c| tx.send(c.to_vec()).unwrap());
        writer.finish().unwrap();

        let mut stt = load(&root).unwrap();
        for path in [&wav, &live] {
            let samples = track::read_track(path).unwrap();
            let lines = transcript::transcribe_track(&samples, &mut stt, |_| {}).unwrap();
            let segments = transcript::assemble(lines, vec![]);
            let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
            eprintln!("{path:?}: {segments:#?}");
            let hits = ["hello", "everyone", "roadmap", "quarter", "meeting", "capture", "feature"].iter().filter(|w| text.contains(*w)).count();
            assert!(hits >= 5, "heard: {text}");
            assert!(segments.iter().all(|s| s.end_ms >= s.start_ms && s.end_ms <= samples.len() as u64 * 1000 / 16_000 + 500));
        }
    }
}
