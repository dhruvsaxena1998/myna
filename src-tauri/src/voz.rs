//! Desert Ant Voz through the Swift bridge in `src-tauri/swift`. Myna's model manager downloads the
//! files; only the Desert Ant SDK ever loads them.
use crate::dictation::Transcriber;
use std::{
    ffi::{c_char, c_void, CStr, CString},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
};

pub const ID: &str = "voz";

extern "C" {
    fn myna_voz_unavailable() -> *mut c_char;
    fn myna_voz_load(dir: *const c_char, failure: *mut *mut c_char) -> *mut c_void;
    fn myna_voz_transcribe(voz: *mut c_void, samples: *const f32, count: usize, failure: *mut *mut c_char) -> *mut c_char;
    fn myna_voz_free(voz: *mut c_void);
    fn myna_string_free(s: *mut c_char);
}

/// Copies a string the bridge allocated and gives it back.
pub(crate) fn take(s: *mut c_char) -> Option<String> {
    if s.is_null() {
        return None;
    }
    // SAFETY: a non-null pointer from the bridge is a NUL-terminated string it malloc'd for us.
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
    unsafe { myna_string_free(s) };
    Some(text)
}

/// Why Voz can't run here (no SDK in this build, or macOS too old), or None when it can.
pub fn unavailable() -> Option<String> {
    take(unsafe { myna_voz_unavailable() })
}

pub struct Voz(*mut c_void);

// SAFETY: the handle is a retained Swift actor, which serializes its own calls.
unsafe impl Send for Voz {}

impl Voz {
    pub fn load(dir: &Path) -> Result<Self, String> {
        if let Some(why) = unavailable() {
            return Err(why);
        }
        let dir = CString::new(dir.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut failure = ptr::null_mut();
        let handle = unsafe { myna_voz_load(dir.as_ptr(), &mut failure) };
        if handle.is_null() {
            return Err(take(failure).unwrap_or_else(|| "Voz failed to load".into()));
        }
        Ok(Self(handle))
    }
}

impl Drop for Voz {
    fn drop(&mut self) {
        unsafe { myna_voz_free(self.0) }
    }
}

impl Transcriber for Voz {
    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        let mut failure = ptr::null_mut();
        let text = unsafe { myna_voz_transcribe(self.0, audio.as_ptr(), audio.len(), &mut failure) };
        take(text).ok_or_else(|| take(failure).unwrap_or_else(|| "Voz failed to transcribe".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio, dictation, models};
    use std::{fs, process::Command, time::Instant};

    #[test]
    fn missing_files_fail_to_load_and_are_not_downloaded() {
        let dir = std::env::temp_dir().join(format!("myna-voz-{}-empty", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let err = Voz::load(&dir).err().expect("an empty folder is not a model");
        assert!(!err.is_empty());
        // The SDK downloads into a folder it is given; the bridge must not let it.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn unavailable_says_why_or_nothing() {
        // Either this build has the SDK on a macOS 15+ host (None), or the reason is user-readable.
        if let Some(why) = unavailable() {
            assert!(why.ends_with('.'), "{why}");
        }
    }

    // Real model on real speech, timed against Parakeet. Run by hand, in release so the SDK's decode
    // loop isn't unoptimized: `cargo test --release -- --ignored real_voz --nocapture`.
    // Needs macOS 15+ and `say`; downloads Voz (337 MB) and Parakeet (670 MB) once into $TMPDIR/myna-voz.
    #[test]
    #[ignore]
    fn real_voz_hears_speech_and_beats_parakeet() {
        assert_eq!(unavailable(), None);
        let root = std::env::temp_dir().join("myna-voz");
        fs::create_dir_all(&root).unwrap();
        for id in [ID, "parakeet-tdt-0.6b-v3"] {
            let m = models::MODELS.iter().find(|m| m.id == id).unwrap();
            if models::status(&root, m) != models::Status::Installed {
                models::install(&root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
            }
            assert!(models::verify(&root, m), "{id} failed its checksums");
        }
        let phrase = "the quick brown fox jumps over the lazy dog";
        let (aiff, wav) = (root.join("say.aiff"), root.join("say.wav"));
        assert!(Command::new("say").arg("-o").arg(&aiff).arg(phrase).status().unwrap().success());
        assert!(Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000"]).arg(&aiff).arg(&wav).status().unwrap().success());
        let mut r = hound::WavReader::open(&wav).unwrap();
        let samples: Vec<f32> = r.samples::<i16>().map(|s| s.unwrap() as f32 / f32::from(i16::MAX)).collect();
        let clip = audio::Clip { samples, channels: 1, rate: 16_000 };

        let t = Instant::now();
        let mut voz = Voz::load(&root.join(ID)).unwrap();
        println!("voz load (first run pays Neural Engine specialization): {:?}", t.elapsed());
        let t = Instant::now();
        let mut parakeet = dictation::Parakeet::load(&root.join("parakeet-tdt-0.6b-v3")).unwrap();
        println!("parakeet load: {:?}", t.elapsed());

        for (name, engine) in [("voz", &mut voz as &mut dyn Transcriber), ("parakeet", &mut parakeet)] {
            dictation::dictate(&clip, engine).unwrap(); // warm up
            let t = Instant::now();
            let heard = dictation::dictate(&clip, engine).unwrap();
            println!("{name}: {:?} in {:?}", heard, t.elapsed());
            let dictation::Heard::Text(text) = heard else { panic!("{name}: expected text, got {heard:?}") };
            let got = text.to_lowercase();
            let hit = phrase.split(' ').filter(|w| got.contains(w)).count();
            assert!(hit >= 7, "{name}: only {hit}/9 words matched: {text}");
        }
    }
}
