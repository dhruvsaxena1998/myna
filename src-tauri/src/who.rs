//! Desert Ant Who through the Swift bridge in `src-tauri/swift`. Myna's model manager downloads the
//! files; only the Desert Ant SDK ever loads them.
use crate::{
    diarize::{Diarizer, Turn},
    voz::take,
};
use std::{
    ffi::{c_char, c_void, CString},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
};

extern "C" {
    fn myna_who_unavailable() -> *mut c_char;
    fn myna_who_load(dir: *const c_char, failure: *mut *mut c_char) -> *mut c_void;
    fn myna_who_diarize(who: *mut c_void, samples: *const f32, count: usize, failure: *mut *mut c_char) -> *mut c_char;
    fn myna_who_free(who: *mut c_void);
}

/// Why Who can't run here (no SDK in this build, or macOS too old), or None when it can.
pub fn unavailable() -> Option<String> {
    take(unsafe { myna_who_unavailable() })
}

pub struct Who(*mut c_void);

// SAFETY: the handle is a retained Swift object whose model serializes its own runs.
unsafe impl Send for Who {}

impl Who {
    pub fn load(dir: &Path) -> Result<Self, String> {
        if let Some(why) = unavailable() {
            return Err(why);
        }
        let dir = CString::new(dir.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut failure = ptr::null_mut();
        let handle = unsafe { myna_who_load(dir.as_ptr(), &mut failure) };
        if handle.is_null() {
            return Err(take(failure).unwrap_or_else(|| "Who failed to load".into()));
        }
        Ok(Self(handle))
    }
}

impl Drop for Who {
    fn drop(&mut self) {
        unsafe { myna_who_free(self.0) }
    }
}

impl Diarizer for Who {
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String> {
        if audio.is_empty() {
            return Ok(vec![]);
        }
        let mut failure = ptr::null_mut();
        let turns = unsafe { myna_who_diarize(self.0, audio.as_ptr(), audio.len(), &mut failure) };
        parse(&take(turns).ok_or_else(|| take(failure).unwrap_or_else(|| "Who failed to separate speakers".into()))?)
    }
}

/// The bridge's answer: one "start end speaker" line per turn, in seconds.
fn parse(text: &str) -> Result<Vec<Turn>, String> {
    text.lines()
        .map(|line| {
            let bad = || format!("Who answered something unexpected: {line}");
            let [start, end, speaker] = line.split(' ').collect::<Vec<_>>()[..] else { return Err(bad()) };
            let ms = |s: &str| s.parse::<f64>().map(|s| (s * 1000.0).round() as u64).map_err(|_| bad());
            Ok(Turn { start_ms: ms(start)?, end_ms: ms(end)?, speaker: speaker.parse().map_err(|_| bad())? })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn turns_are_read_from_the_bridges_lines() {
        let got = parse("0.0 4.25 0\n3.5 9.0 1\n").unwrap();
        assert_eq!(got, [Turn { start_ms: 0, end_ms: 4250, speaker: 0 }, Turn { start_ms: 3500, end_ms: 9000, speaker: 1 }]);
        assert_eq!(parse("").unwrap(), []);
        for bad in ["0.0 1.0", "a 1.0 0", "0.0 1.0 -1", "0.0 1.0 0 9"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn missing_files_fail_to_load_and_are_not_downloaded() {
        let dir = std::env::temp_dir().join(format!("myna-who-{}-empty", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let err = Who::load(&dir).err().expect("an empty folder is not a model");
        assert!(!err.is_empty());
        // The SDK downloads into a folder it is given; the bridge must not let it.
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    }

    #[test]
    fn unavailable_says_why_or_nothing() {
        if let Some(why) = unavailable() {
            assert!(why.ends_with('.'), "{why}");
        }
    }
}
