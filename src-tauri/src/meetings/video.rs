//! Window video: the Swift bridge records one window to an H.264 MP4 (ScreenCaptureKit and AVAssetWriter).
//! Kept thin because it needs Screen Recording and a real Mac; the size estimate is pure and tested here.
use crate::voz::take;
use std::{
    ffi::{c_char, c_void, CString},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr,
    time::Instant,
};

/// Video is scaled to this many px wide, whatever the window's size.
const WIDTH: usize = 1280;
/// H.264 average bit rate: about 400 MB an hour, which is what Settings tells the user before they pick it.
// shortcut: the encoder treats this as an average, so a busy shared screen can run over it and a still slide
// comes in far under. Measure real calls before promising a tighter number.
const BITRATE: u64 = 900_000;

extern "C" {
    fn myna_window_video_start(path: *const c_char, window: u32, width: usize, bitrate: usize, lead_ms: isize, failure: *mut *mut c_char) -> *mut c_void;
    fn myna_window_video_capture(video: *mut c_void, window: u32, failure: *mut *mut c_char) -> bool;
    fn myna_window_video_stop(video: *mut c_void, end_ms: isize);
}

/// Bytes of video for `secs` of recording at the average bit rate.
fn estimate_bytes(secs: u64) -> u64 {
    BITRATE / 8 * secs
}

/// What Settings says before window video is picked: an hour's worth, to the nearest 50 MB.
pub fn hour_estimate() -> String {
    format!("about {} MB for an hour", (estimate_bytes(3600) + 25_000_000) / 50_000_000 * 50)
}

/// A window being recorded. Dropping it closes the file, so every exit path (stop, quit, mode switch) leaves a playable video.
pub struct Video {
    handle: *mut c_void,
    /// When the meeting began, so the video's clock matches the audio's.
    started: Instant,
}

// SAFETY: the handle is a retained Swift object that serializes its own calls on one queue.
unsafe impl Send for Video {}

fn ms(started: Instant) -> isize {
    started.elapsed().as_millis() as isize
}

impl Video {
    /// Starts recording window `id` into `path`, which must not be a file worth keeping.
    pub fn start(path: &Path, id: u32, started: Instant) -> Result<Self, String> {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let mut failure = ptr::null_mut();
        let handle = unsafe { myna_window_video_start(path.as_ptr(), id, WIDTH, BITRATE as usize, ms(started), &mut failure) };
        if handle.is_null() {
            return Err(take(failure).unwrap_or_else(|| "Window video failed to start".into()));
        }
        Ok(Self { handle, started })
    }

    /// Follows window `id` from now on; the same window again does nothing, and a closed one is picked back up.
    pub fn capture(&mut self, id: u32) -> Result<(), String> {
        let mut failure = ptr::null_mut();
        if unsafe { myna_window_video_capture(self.handle, id, &mut failure) } {
            return Ok(());
        }
        Err(take(failure).unwrap_or_else(|| "Window video failed".into()))
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        unsafe { myna_window_video_stop(self.handle, ms(self.started)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_hour_costs_about_400_mb_and_the_estimate_scales_with_time() {
        assert_eq!(estimate_bytes(3600), 405_000_000);
        assert_eq!(estimate_bytes(0), 0);
        assert_eq!(estimate_bytes(7200), 2 * estimate_bytes(3600));
        assert_eq!(hour_estimate(), "about 400 MB for an hour");
    }

    // Hardware check, not run in CI: needs Screen Recording allowed for whatever runs the tests (the first run
    // waits on that prompt). Records the first app window for 6 s. `cargo test --lib records_a_window -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn records_a_window() {
        let windows = crate::meetings::screen::windows().unwrap();
        let w = windows.first().expect("an app window is open");
        let path = std::env::temp_dir().join(format!("myna-video-{}/window.mp4", std::process::id()));
        eprintln!("recording {} — {} into {}", w.app, w.title, path.display());
        let mut video = Video::start(&path, w.id, Instant::now()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(3));
        video.capture(w.id).unwrap(); // the same window again is a no-op
        std::thread::sleep(std::time::Duration::from_secs(3));
        drop(video);
        let bytes = std::fs::metadata(&path).unwrap().len();
        eprintln!("{bytes} bytes");
        assert!(bytes > 1000);
    }
}
