//! Dictation pipeline: captured audio -> text. Capture, paste and the pill live elsewhere.
use crate::{audio, stt::Runtime};
use parakeet_rs::{ort::environment::Environment, ExecutionConfig};
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

/// A line of speech with times relative to the start of the audio it came from.
pub struct Utterance {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A speech-to-text backend. Takes mono 16 kHz samples.
pub trait Transcriber: Send {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String>;

    /// Timestamped lines, for meetings. A backend without timestamps reports one line spanning the audio.
    fn transcribe_timed(&mut self, audio: &[f32]) -> Result<Vec<Utterance>, String> {
        let text = self.transcribe(audio)?;
        Ok(vec![Utterance { start_ms: 0, end_ms: audio.len() as u64 * 1000 / audio::RATE as u64, text }])
    }
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

pub struct Parakeet(parakeet_rs::ParakeetTDT);

/// ONNX Runtime allows one environment per process, so every model load shares this one.
pub fn ort_config() -> Result<ExecutionConfig, String> {
    static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
    let env = ENV.get_or_init(|| parakeet_rs::ort::init().build().map_err(|e| e.to_string())).clone()?;
    Ok(ExecutionConfig::new().with_environment(env))
}

impl Parakeet {
    pub fn load(dir: &Path) -> Result<Self, String> {
        parakeet_rs::ParakeetTDT::from_pretrained(dir, Some(ort_config()?))
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

    fn transcribe_timed(&mut self, audio: &[f32]) -> Result<Vec<Utterance>, String> {
        use parakeet_rs::Transcriber as _;
        let r = self
            .0
            .transcribe_samples(audio.to_vec(), audio::RATE, 1, Some(parakeet_rs::TimestampMode::Sentences))
            .map_err(|e| e.to_string())?;
        Ok(r.tokens
            .into_iter()
            .map(|t| Utterance { start_ms: (t.start * 1000.0) as u64, end_ms: (t.end * 1000.0) as u64, text: t.text })
            .collect())
    }
}

pub struct Whisper(WhisperState);

impl Whisper {
    /// `file` is the ggml .bin. Uses the GPU (Metal) when whisper.cpp can.
    pub fn load(file: &Path) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(file, WhisperContextParameters::default()).map_err(|e| e.to_string())?;
        ctx.create_state().map(Self).map_err(|e| e.to_string())
    }
}

impl Whisper {
    fn run(&mut self, audio: &[f32], timestamps: bool) -> Result<(), String> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(None); // detect it: Whisper is the pick for speech Parakeet doesn't cover
        p.set_no_timestamps(!timestamps);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        self.0.full(p, audio).map_err(|e| e.to_string())
    }
}

impl Transcriber for Whisper {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        self.run(audio, false)?;
        Ok(self.0.as_iter().map(|s| s.to_string()).collect())
    }

    fn transcribe_timed(&mut self, audio: &[f32]) -> Result<Vec<Utterance>, String> {
        self.run(audio, true)?;
        // whisper.cpp reports segment times in centiseconds.
        Ok(self
            .0
            .as_iter()
            .map(|s| Utterance {
                start_ms: s.start_timestamp().max(0) as u64 * 10,
                end_ms: s.end_timestamp().max(0) as u64 * 10,
                text: s.to_string().trim().to_string(),
            })
            .filter(|u| !u.text.is_empty())
            .collect())
    }
}

/// Loads the backend for a model `stt::resolve` found.
pub fn load(runtime: Runtime, path: &Path) -> Result<Box<dyn Transcriber>, String> {
    Ok(match runtime {
        Runtime::Parakeet => Box::new(Parakeet::load(path)?),
        Runtime::Whisper => Box::new(Whisper::load(path)?),
        Runtime::Voz => Box::new(crate::voz::Voz::load(path)?),
    })
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
    use crate::{
        models,
        stt::{self, Custom, PARAKEET, WHISPER},
    };
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
    fn selecting_another_model_loads_its_backend_on_the_next_dictation() {
        let dir = tmp("switch");
        let (parakeet, whisper) = (dir.join("parakeet"), dir.join("whisper.bin"));
        fs::create_dir_all(&parakeet).unwrap();
        for n in ["vocab.txt", "encoder-model.int8.onnx", "decoder_joint-model.int8.onnx"] {
            fs::write(parakeet.join(n), b"x").unwrap();
        }
        fs::write(&whisper, 0x6767_6d6c_u32.to_le_bytes()).unwrap();
        let all: Vec<Custom> = [(Runtime::Parakeet, &parakeet), (Runtime::Whisper, &whisper)]
            .into_iter()
            .map(|(rt, p)| Custom::new(rt, &p.display().to_string(), |_| unreachable!()).unwrap())
            .collect();

        let engine = Engine::default();
        let loaded = Cell::new(vec![]);
        // What `with_engine` does for each dictation, with the loaders stubbed out.
        let dictate_with = |id: &str| {
            engine
                .with(
                    id,
                    || {
                        let (rt, _) = stt::resolve(id, &dir, &all)?;
                        let mut seen = loaded.take();
                        seen.push(rt);
                        loaded.set(seen);
                        Ok(Box::new(Fake("hi")) as Box<dyn Transcriber>)
                    },
                    |_| (),
                )
                .unwrap()
        };
        dictate_with(&all[0].id());
        dictate_with(&all[0].id());
        dictate_with(&all[1].id());
        dictate_with(&all[0].id());
        assert_eq!(loaded.take(), [Runtime::Parakeet, Runtime::Whisper, Runtime::Parakeet]);
    }

    #[test]
    fn a_backend_without_timestamps_reports_one_line_spanning_the_clip() {
        // Voz gives no timestamps, so meetings get this default for it.
        let u = Fake("hi").transcribe_timed(&vec![0.0; 16_000]).unwrap();
        assert_eq!((u.len(), u[0].start_ms, u[0].end_ms, u[0].text.as_str()), (1, 0, 1000, "hi"));
    }

    /// Share of the reference's words the transcript got wrong (edit distance over words), ignoring case and punctuation.
    fn word_error_rate(reference: &str, heard: &str) -> f64 {
        let words = |t: &str| -> Vec<String> {
            t.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect()
        };
        let (r, h) = (words(reference), words(heard));
        let mut row: Vec<usize> = (0..=h.len()).collect();
        for (i, rw) in r.iter().enumerate() {
            let mut diag = row[0];
            row[0] = i + 1;
            for (j, hw) in h.iter().enumerate() {
                let up = row[j + 1];
                row[j + 1] = (diag + usize::from(rw != hw)).min(row[j] + 1).min(up + 1);
                diag = up;
            }
        }
        row[h.len()] as f64 / r.len() as f64
    }

    #[test]
    fn word_error_rate_counts_substitutions_insertions_and_deletions() {
        assert_eq!(word_error_rate("the cat sat", "The cat, sat."), 0.0);
        assert_eq!(word_error_rate("the cat sat down", "the cat sat up"), 0.25);
        assert_eq!(word_error_rate("the cat sat", "the cat"), 1.0 / 3.0);
        assert_eq!(word_error_rate("the cat", "the big cat"), 0.5);
        assert_eq!(word_error_rate("one two", ""), 1.0);
    }

    // The tests below use real models and are run by hand: `cargo test -- --ignored real_`.
    // They need macOS `say`, and download models once into $TMPDIR (Parakeet ~670 MB, Whisper ~1.6 GB).
    const SPOKEN: &str = "The quick brown fox jumps over the lazy dog. She sells sea shells by the sea shore. \
        Please call Stella and ask her to bring a small snack from the store on Monday.";

    /// About ten seconds of synthetic speech, as a clip like the one the microphone delivers.
    fn spoken_fixture() -> Clip {
        let dir = std::env::temp_dir().join("myna-fixture");
        fs::create_dir_all(&dir).unwrap();
        let (aiff, wav) = (dir.join("say.aiff"), dir.join("say.wav"));
        assert!(Command::new("say").arg("-o").arg(&aiff).arg(SPOKEN).status().unwrap().success());
        assert!(Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000"]).arg(&aiff).arg(&wav).status().unwrap().success());
        let clip = read_wav(&wav);
        let secs = clip.samples.len() as f32 / clip.rate as f32;
        assert!((8.0..13.0).contains(&secs), "fixture is {secs} s, expected about 10");
        clip
    }

    fn installed(name: &str, id: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(name);
        let m = models::MODELS.iter().find(|m| m.id == id).unwrap();
        if models::status(&root, m) != models::Status::Installed {
            fs::create_dir_all(&root).unwrap();
            models::install(&root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
        }
        root
    }

    fn assert_hears_the_fixture(t: &mut dyn Transcriber) {
        let Heard::Text(text) = dictate(&spoken_fixture(), t).unwrap() else { panic!("expected text") };
        let wer = word_error_rate(SPOKEN, &text);
        println!("heard: {text}\nword error rate: {wer:.2}");
        assert!(wer <= 0.3, "word error rate {wer:.2}: {text}");
    }

    #[test]
    #[ignore]
    fn real_parakeet_hears_speech() {
        let root = installed("myna-parakeet", PARAKEET);
        let (rt, path) = stt::resolve(PARAKEET, &root, &[]).unwrap();
        assert_hears_the_fixture(&mut *load(rt, &path).unwrap());
    }

    #[test]
    #[ignore]
    fn real_whisper_hears_speech() {
        let root = installed("myna-whisper", WHISPER);
        let (rt, path) = stt::resolve(WHISPER, &root, &[]).unwrap();
        assert_eq!(rt, Runtime::Whisper);
        assert_hears_the_fixture(&mut *load(rt, &path).unwrap());
    }

    // The Advanced path end to end: look a repo up on Hugging Face, download it, resolve and dictate.
    // The smallest ggml model keeps it quick, so the bar is looser than for the real models above.
    #[test]
    #[ignore]
    fn real_custom_repo_downloads_and_dictates() {
        let root = std::env::temp_dir().join("myna-custom");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        // The Parakeet repo resolves to exactly the curated files, checksums included.
        let pk = Custom::new(Runtime::Parakeet, "istupakov/parakeet-tdt-0.6b-v3-onnx", stt::fetch_repo).unwrap();
        let curated = &models::MODELS[0];
        for f in curated.files {
            let got = pk.files.iter().find(|r| r.name == f.name).unwrap_or_else(|| panic!("{} not planned", f.name));
            assert_eq!((got.size, got.sha256.as_str()), (f.size, if f.name == "vocab.txt" { "" } else { f.sha256 }));
        }
        let c = Custom::new(Runtime::Whisper, "ggerganov/whisper.cpp/ggml-tiny.bin", stt::fetch_repo).unwrap();
        let all = [c.clone()];
        assert_eq!(stt::resolve(&c.id(), &root, &all).unwrap_err(), "Download ggerganov/whisper.cpp/ggml-tiny.bin in Settings");
        let files: Vec<_> = c.files.iter().map(models::Remote::file).collect();
        models::install(&root, &c.id(), &files, models::http_open, &models::Job::default()).unwrap();
        assert!(c.verify(&root));
        let (rt, path) = stt::resolve(&c.id(), &root, &all).unwrap();
        let Heard::Text(text) = dictate(&spoken_fixture(), &mut *load(rt, &path).unwrap()).unwrap() else { panic!("expected text") };
        let wer = word_error_rate(SPOKEN, &text);
        println!("heard: {text}\nword error rate: {wer:.2}");
        assert!(wer <= 0.5, "word error rate {wer:.2}: {text}");
    }
}
