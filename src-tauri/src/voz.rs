//! Desert Ant Voz through the Swift bridge (`swift/`). Its SDK downloads, verifies and loads the
//! weights in `models/voz/`. Each model load also asks Desert Ant's server for a license (no audio or text).

use crate::runtime::{Segment, Transcriber};
use serde::Deserialize;
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;

unsafe extern "C" {
    fn myna_voz_installed(dir: *const c_char) -> bool;
    fn myna_voz_download(dir: *const c_char, ctx: *mut c_void, progress: extern "C" fn(*mut c_void, f64)) -> *mut c_char;
    fn myna_voz_load(dir: *const c_char) -> *mut c_char;
    fn myna_voz_free();
    fn myna_voz_transcribe(samples: *const f32, count: isize, vocabulary: *const c_char) -> *mut c_char;
    fn free(p: *mut c_void);
}

pub(crate) fn c(dir: &Path) -> CString {
    CString::new(dir.to_string_lossy().as_bytes()).unwrap_or_default()
}

/// The Swift side returns JSON from strdup.
pub(crate) fn take(p: *mut c_char) -> String {
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { free(p.cast()) };
    s
}

#[derive(Deserialize)]
struct Out {
    #[serde(default)]
    segments: Vec<Segment>,
    error: Option<String>,
}

fn out(json: String) -> Result<Vec<Segment>, String> {
    let o: Out = serde_json::from_str(&json).map_err(|e| format!("Voz answered oddly: {e}"))?;
    o.error.map_or(Ok(o.segments), Err)
}

pub fn installed(dir: &Path) -> bool {
    unsafe { myna_voz_installed(c(dir).as_ptr()) }
}

/// Downloads into `dir`; `progress` gets 0.0 to 1.0.
pub fn download<F: FnMut(f64)>(dir: &Path, mut progress: F) -> Result<(), String> {
    extern "C" fn call<F: FnMut(f64)>(ctx: *mut c_void, done: f64) {
        unsafe { (*ctx.cast::<F>())(done) }
    }
    out(take(unsafe { myna_voz_download(c(dir).as_ptr(), (&mut progress as *mut F).cast(), call::<F>) })).map(|_| ())
}

/// The loaded model; dropping it unloads it. Only one is loaded at a time, as with every model.
pub struct Voz(String);

impl Voz {
    pub fn load(dir: &Path) -> Result<Self, String> {
        out(take(unsafe { myna_voz_load(c(dir).as_ptr()) }))?;
        Ok(Voz(String::new()))
    }
}

impl Transcriber for Voz {
    /// `words` is the comma-separated list from `rules::prompt`.
    fn bias(&mut self, words: &str) {
        self.0 = words.to_string();
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<Vec<Segment>, String> {
        let words: Vec<&str> = self.0.split(", ").filter(|w| !w.is_empty()).collect();
        let vocab = CString::new(serde_json::to_string(&words).unwrap_or_default()).unwrap_or_default();
        out(take(unsafe { myna_voz_transcribe(audio.as_ptr(), audio.len() as isize, vocab.as_ptr()) }))
    }
}

impl Drop for Voz {
    fn drop(&mut self) {
        unsafe { myna_voz_free() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_segments_come_through() {
        assert_eq!(out(r#"{"error":"nope"}"#.into()), Err("nope".into()));
        let segs = out(r#"{"segments":[{"start":0.5,"end":1.0,"text":"Hi."}]}"#.into()).unwrap();
        assert_eq!(segs, [Segment { start: 0.5, end: 1.0, text: "Hi.".into() }]);
    }

    #[test]
    fn an_empty_folder_is_not_installed_and_wont_load() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!installed(tmp.path()));
        assert_eq!(Voz::load(tmp.path()).err().unwrap(), "Voz is missing. Download it again.");
    }
}
