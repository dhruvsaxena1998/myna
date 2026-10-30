//! Desert Ant Uhm (filler-word detection) through the Swift bridge in `src-tauri/swift`. Like Voz, Myna
//! downloads the files and only the Desert Ant SDK loads them.
use crate::{dictation::FillerDetector, voz::take};
use std::{
    ffi::{c_char, c_void, CString},
    ops::Range,
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
};

pub const ID: &str = "uhm";

extern "C" {
    fn myna_uhm_unavailable() -> *mut c_char;
    fn myna_uhm_load(dir: *const c_char, failure: *mut *mut c_char) -> *mut c_void;
    fn myna_uhm_analyze(uhm: *mut c_void, samples: *const f32, count: usize, failure: *mut *mut c_char) -> *mut c_char;
    fn myna_uhm_free(uhm: *mut c_void);
}

/// Why Uhm can't run in this build, or None when it can.
pub fn unavailable() -> Option<String> {
    take(unsafe { myna_uhm_unavailable() })
}

pub struct Uhm(*mut c_void);

// SAFETY: the handle is a retained Swift object whose analysis calls are independent.
unsafe impl Send for Uhm {}

impl Uhm {
    pub fn load(dir: &Path) -> Result<Self, String> {
        if let Some(why) = unavailable() {
            return Err(why);
        }
        let dir = CString::new(dir.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut failure = ptr::null_mut();
        let handle = unsafe { myna_uhm_load(dir.as_ptr(), &mut failure) };
        if handle.is_null() {
            return Err(take(failure).unwrap_or_else(|| "Uhm failed to load".into()));
        }
        Ok(Self(handle))
    }
}

impl Drop for Uhm {
    fn drop(&mut self) {
        unsafe { myna_uhm_free(self.0) }
    }
}

/// The bridge's answer: one "start end" line in seconds per filler.
fn parse_spans(lines: &str) -> Vec<Range<u64>> {
    lines
        .lines()
        .filter_map(|l| {
            let (s, e) = l.split_once(' ')?;
            Some((s.parse::<f64>().ok()? * 1000.0) as u64..(e.parse::<f64>().ok()? * 1000.0) as u64)
        })
        .collect()
}

impl FillerDetector for Uhm {
    fn fillers(&mut self, audio: &[f32]) -> Result<Vec<Range<u64>>, String> {
        let mut failure = ptr::null_mut();
        let lines = unsafe { myna_uhm_analyze(self.0, audio.as_ptr(), audio.len(), &mut failure) };
        take(lines).map(|l| parse_spans(&l)).ok_or_else(|| take(failure).unwrap_or_else(|| "Uhm failed to analyze".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models;
    use std::{fs, process::Command};

    #[test]
    fn the_bridge_answer_becomes_millisecond_spans() {
        assert_eq!(parse_spans("0.5 0.92\n3 3.25"), vec![500..920, 3000..3250]);
        assert_eq!(parse_spans(""), Vec::<Range<u64>>::new());
        assert_eq!(parse_spans("junk\n1 x"), Vec::<Range<u64>>::new());
    }

    #[test]
    fn missing_files_fail_to_load_and_are_not_downloaded() {
        let dir = std::env::temp_dir().join(format!("myna-uhm-{}-empty", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(Uhm::load(&dir).err().is_some_and(|e| !e.is_empty()));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    // Real model on synthetic speech: loads Uhm, runs it, and checks the spans are inside the audio.
    // `cargo test --release -- --ignored real_uhm --nocapture`. Needs macOS `say`; downloads Uhm (47 MB)
    // once into $TMPDIR/myna-uhm. `say` writes a file; nothing is played or recorded.
    #[test]
    #[ignore]
    fn real_uhm_runs_on_speech() {
        assert_eq!(unavailable(), None);
        let root = std::env::temp_dir().join("myna-uhm");
        fs::create_dir_all(&root).unwrap();
        let m = models::MODELS.iter().find(|m| m.id == ID).unwrap();
        if models::status(&root, m) != models::Status::Installed {
            models::install(&root, m.id, m.files, models::http_open, &models::Job::default()).unwrap();
        }
        assert!(models::verify(&root, m), "uhm failed its checksums");
        let (aiff, wav) = (root.join("say.aiff"), root.join("say.wav"));
        assert!(Command::new("say").arg("-o").arg(&aiff).arg("so, um, I think we should, uh, ship it").status().unwrap().success());
        assert!(Command::new("afconvert").args(["-f", "WAVE", "-d", "LEI16@16000"]).arg(&aiff).arg(&wav).status().unwrap().success());
        let samples: Vec<f32> = hound::WavReader::open(&wav).unwrap().samples::<i16>().map(|s| s.unwrap() as f32 / f32::from(i16::MAX)).collect();
        let ms = samples.len() as u64 * 1000 / 16_000;
        let spans = Uhm::load(&root.join(ID)).unwrap().fillers(&samples).unwrap();
        println!("{ms} ms of audio, fillers: {spans:?}");
        assert!(spans.iter().all(|s| s.start < s.end && s.end <= ms + 50));
    }
}
