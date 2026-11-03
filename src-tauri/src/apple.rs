//! Apple Speech through the Swift bridge (`swift/`): SpeechTranscriber in the Mac's language, macOS 26+.

use crate::models::System;
use crate::runtime::Segment;
use serde::Deserialize;
use std::ffi::{CStr, c_char, c_void};

unsafe extern "C" {
    fn myna_apple_stt_status() -> *mut c_char;
    fn myna_apple_stt_transcribe(samples: *const f32, count: isize) -> *mut c_char;
    fn myna_apple_stt_download(ctx: *mut c_void, progress: extern "C" fn(*mut c_void, f64)) -> *mut c_char;
    fn free(p: *mut c_void);
}

/// The Swift side returns JSON from strdup.
fn take(p: *mut c_char) -> String {
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
    let o: Out = serde_json::from_str(&json).map_err(|e| format!("Apple Speech answered oddly: {e}"))?;
    o.error.map_or(Ok(o.segments), Err)
}

/// The Mac's language as Apple Speech hears it, and whether macOS has its model.
pub fn status() -> System {
    serde_json::from_str(&take(unsafe { myna_apple_stt_status() })).unwrap_or_default()
}

pub fn transcribe(audio: &[f32]) -> Result<Vec<Segment>, String> {
    out(take(unsafe { myna_apple_stt_transcribe(audio.as_ptr(), audio.len() as isize) }))
}

/// Has macOS fetch the language's model; `progress` gets 0.0 to 1.0.
pub fn download<F: FnMut(f64)>(mut progress: F) -> Result<(), String> {
    extern "C" fn call<F: FnMut(f64)>(ctx: *mut c_void, done: f64) {
        unsafe { (*ctx.cast::<F>())(done) }
    }
    out(take(unsafe { myna_apple_stt_download((&mut progress as *mut F).cast(), call::<F>) })).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_segments_come_through() {
        assert_eq!(out(r#"{"error":"nope"}"#.into()), Err("nope".into()));
        assert_eq!(out("{}".into()), Ok(vec![]));
        let segs = out(r#"{"segments":[{"start":0.5,"end":1.0,"text":"Hi."}]}"#.into()).unwrap();
        assert_eq!(segs, [Segment { start: 0.5, end: 1.0, text: "Hi.".into() }]);
    }
}
