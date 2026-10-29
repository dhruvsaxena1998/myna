//! Dictation pipeline: captured audio -> text. Capture, paste and the pill live elsewhere.
use crate::{audio, models};
use parakeet_rs::{ort::environment::Environment, ExecutionConfig};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};

/// A speech-to-text backend. Takes mono 16 kHz samples.
pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String>;
}

#[derive(Debug, PartialEq)]
pub enum Heard {
    Text(String),
    /// Shorter than a deliberate hold; treated as an accidental tap.
    TooShort,
    Silence,
}

const MIN_SAMPLES: usize = audio::RATE as usize * 3 / 10;

pub fn dictate(clip: &audio::Clip, t: &mut dyn Transcriber) -> Result<Heard, String> {
    let samples = audio::to_mono_16k(clip);
    if samples.len() < MIN_SAMPLES {
        return Ok(Heard::TooShort);
    }
    let text = t.transcribe(&samples)?;
    let text = text.trim();
    Ok(if text.is_empty() { Heard::Silence } else { Heard::Text(text.to_string()) })
}

const PARAKEET: &str = "parakeet-tdt-0.6b-v3";

pub struct Parakeet(parakeet_rs::ParakeetTDT);

impl Parakeet {
    pub fn load(dir: &Path) -> Result<Self, String> {
        // ONNX Runtime allows one environment per process, so every model load shares this one.
        static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
        let env = ENV.get_or_init(|| parakeet_rs::ort::init().build().map_err(|e| e.to_string())).clone()?;
        parakeet_rs::ParakeetTDT::from_pretrained(dir, Some(ExecutionConfig::new().with_environment(env)))
            .map(Self)
            .map_err(|e| e.to_string())
    }
}

impl Transcriber for Parakeet {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        use parakeet_rs::Transcriber as _;
        self.0
            .transcribe_samples(audio.to_vec(), audio::RATE, 1, None)
            .map(|r| r.text)
            .map_err(|e| e.to_string())
    }
}

/// The selected speech model's backend. Only Parakeet has one so far.
pub fn load(id: &str, root: &Path) -> Result<Box<dyn Transcriber>, String> {
    let m = models::MODELS.iter().find(|m| m.id == id).ok_or("Unknown speech model")?;
    if models::status(root, m) != models::Status::Installed {
        return Err("Download a speech model in Settings".into());
    }
    match id {
        PARAKEET => Ok(Box::new(Parakeet::load(&root.join(id))?)),
        _ => Err(format!("{} can't dictate yet", m.name)),
    }
}

/// Keeps the loaded model between dictations: loading takes seconds, transcribing a fraction of that.
#[derive(Default)]
pub struct Engine(Mutex<Option<(String, Box<dyn Transcriber>)>>);

impl Engine {
    /// Runs `f` on the transcriber for `id`, calling `load` first if another model (or none) is loaded.
    /// The lock is held while loading, so a caller arriving mid-load waits instead of loading twice.
    pub fn with<R>(
        &self,
        id: &str,
        load: impl FnOnce() -> Result<Box<dyn Transcriber>, String>,
        f: impl FnOnce(&mut dyn Transcriber) -> R,
    ) -> Result<R, String> {
        let mut slot = self.0.lock().unwrap();
        if slot.as_ref().is_none_or(|(loaded, _)| loaded != id) {
            *slot = Some((id.to_string(), load()?));
        }
        Ok(f(slot.as_mut().unwrap().1.as_mut()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio::Clip;
    use std::{cell::Cell, fs, process::Command};

    struct Fake(&'static str);
    impl Transcriber for Fake {
        fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
            // Proves the backend gets mono 16 kHz regardless of the device format.
            assert!((audio.len() as i64 - 16_000).abs() <= 1, "got {} samples", audio.len());
            Ok(self.0.to_string())
        }
    }

    struct Broken;
    impl Transcriber for Broken {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, String> {
            Err("model crashed".into())
        }
    }

    fn read_wav(path: &Path) -> Clip {
        let mut r = hound::WavReader::open(path).unwrap();
        let spec = r.spec();
        let scale = f32::from(i16::MAX);
        let samples = r.samples::<i16>().map(|s| s.unwrap() as f32 / scale).collect();
        Clip { samples, channels: spec.channels, rate: spec.sample_rate }
    }

    fn write_wav(path: &Path, channels: u16, rate: u32, seconds: f32) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f32 * seconds) as usize {
            let s = ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16;
            (0..channels).for_each(|_| w.write_sample(s).unwrap());
        }
        w.finalize().unwrap();
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("myna-dictation-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn fixture_wav_becomes_text() {
        let dir = tmp("wav");
        // What a MacBook mic delivers: 48 kHz stereo.
        write_wav(&dir.join("hello.wav"), 2, 48_000, 1.0);
        let heard = dictate(&read_wav(&dir.join("hello.wav")), &mut Fake("  Hello world.\n")).unwrap();
        assert_eq!(heard, Heard::Text("Hello world.".into()));
    }

    #[test]
    fn taps_and_silence_paste_nothing() {
        let dir = tmp("short");
        write_wav(&dir.join("tap.wav"), 1, 16_000, 0.1);
        assert_eq!(dictate(&read_wav(&dir.join("tap.wav")), &mut Broken).unwrap(), Heard::TooShort);
        write_wav(&dir.join("quiet.wav"), 1, 16_000, 1.0);
        assert_eq!(dictate(&read_wav(&dir.join("quiet.wav")), &mut Fake("   ")).unwrap(), Heard::Silence);
    }

    #[test]
    fn transcriber_errors_reach_the_caller() {
        let dir = tmp("err");
        write_wav(&dir.join("a.wav"), 1, 16_000, 1.0);
        assert_eq!(dictate(&read_wav(&dir.join("a.wav")), &mut Broken), Err("model crashed".into()));
    }

    #[test]
    fn engine_loads_once_per_model() {
        let loads = Cell::new(0);
        let engine = Engine::default();
        let load = || {
            loads.set(loads.get() + 1);
            Ok(Box::new(Fake("x")) as Box<dyn Transcriber>)
        };
        // `Cell` is not Sync, but these closures only run on this thread.
        engine.with("a", load, |_| ()).unwrap();
        engine.with("a", load, |_| ()).unwrap();
        assert_eq!(loads.get(), 1);
        engine.with("b", load, |_| ()).unwrap();
        assert_eq!(loads.get(), 2);
        // A failed load leaves the old model usable for what it was.
        assert_eq!(engine.with("c", || Err("nope".into()), |_| ()).err().as_deref(), Some("nope"));
    }

    #[test]
    fn model_that_is_not_downloaded_says_so() {
        let root = tmp("absent");
        assert_eq!(load(PARAKEET, &root).err().as_deref(), Some("Download a speech model in Settings"));
        assert_eq!(load("nope", &root).err().as_deref(), Some("Unknown speech model"));
    }

    // Real model on real speech. Run by hand: `cargo test -- --ignored real_parakeet`.
    // Needs macOS `say`; downloads the ~670 MB model once into $TMPDIR/myna-parakeet.
    #[test]
    #[ignore]
    fn real_parakeet_hears_speech() {
        let root = std::env::temp_dir().join("myna-parakeet");
        let m = models::MODELS.iter().find(|m| m.id == PARAKEET).unwrap();
        if models::status(&root, m) != models::Status::Installed {
            fs::create_dir_all(&root).unwrap();
            models::install(&root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
        }
        let phrase = "the quick brown fox jumps over the lazy dog";
        let (aiff, wav) = (root.join("say.aiff"), root.join("say.wav"));
        assert!(Command::new("say").args(["-o"]).arg(&aiff).arg(phrase).status().unwrap().success());
        assert!(Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000"]).arg(&aiff).arg(&wav).status().unwrap().success());
        let heard = dictate(&read_wav(&wav), &mut Parakeet::load(&root.join(PARAKEET)).unwrap()).unwrap();
        let Heard::Text(text) = heard else { panic!("expected text, got {heard:?}") };
        println!("heard: {text}");
        let got = text.to_lowercase();
        let hit = phrase.split(' ').filter(|w| got.contains(w)).count();
        assert!(hit >= 7, "only {hit}/9 words matched: {text}");
    }
}
