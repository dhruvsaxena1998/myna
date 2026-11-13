//! Desert Ant Who through the Swift bridge (`swift/`), like Voz. Its SDK downloads, verifies and loads
//! the weights in `models/who/`.

use crate::runtime::{Diarizer, Turn};
use crate::voz::{c, take};
use serde::Deserialize;
use std::ffi::{c_char, c_void};
use std::path::Path;

unsafe extern "C" {
    fn myna_who_installed(dir: *const c_char) -> bool;
    fn myna_who_download(dir: *const c_char, ctx: *mut c_void, progress: extern "C" fn(*mut c_void, f64)) -> *mut c_char;
    fn myna_who_load(dir: *const c_char) -> *mut c_char;
    fn myna_who_free();
    fn myna_who_diarize(samples: *const f32, count: isize) -> *mut c_char;
}

#[derive(Deserialize)]
struct Out {
    #[serde(default)]
    turns: Vec<Turn>,
    error: Option<String>,
}

fn out(json: String) -> Result<Vec<Turn>, String> {
    let o: Out = serde_json::from_str(&json).map_err(|e| format!("Who answered oddly: {e}"))?;
    o.error.map_or(Ok(o.turns), Err)
}

pub fn installed(dir: &Path) -> bool {
    unsafe { myna_who_installed(c(dir).as_ptr()) }
}

/// Downloads into `dir`; `progress` gets 0.0 to 1.0.
pub fn download<F: FnMut(f64)>(dir: &Path, mut progress: F) -> Result<(), String> {
    extern "C" fn call<F: FnMut(f64)>(ctx: *mut c_void, done: f64) {
        unsafe { (*ctx.cast::<F>())(done) }
    }
    out(take(unsafe { myna_who_download(c(dir).as_ptr(), (&mut progress as *mut F).cast(), call::<F>) })).map(|_| ())
}

/// The loaded model; dropping it unloads it.
pub struct Who;

impl Who {
    pub fn load(dir: &Path) -> Result<Self, String> {
        out(take(unsafe { myna_who_load(c(dir).as_ptr()) }))?;
        Ok(Who)
    }
}

impl Diarizer for Who {
    fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Turn>, String> {
        out(take(unsafe { myna_who_diarize(audio.as_ptr(), audio.len() as isize) }))
    }
}

impl Drop for Who {
    fn drop(&mut self) {
        unsafe { myna_who_free() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_turns_come_through() {
        assert_eq!(out(r#"{"error":"nope"}"#.into()), Err("nope".into()));
        let turns = out(r#"{"turns":[{"start":0.5,"end":1.25,"speaker":1}]}"#.into()).unwrap();
        assert_eq!(turns, [Turn { start: 0.5, end: 1.25, speaker: 1 }]);
    }

    #[test]
    fn an_empty_folder_is_not_installed_and_wont_load() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!installed(tmp.path()));
        assert_eq!(Who::load(tmp.path()).err().unwrap(), "Who is missing. Download it again.");
    }
}
