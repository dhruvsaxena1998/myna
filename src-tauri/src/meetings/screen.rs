//! ScreenCaptureKit: the windows Myna can capture, and one window as a JPEG plus its perceptual hash.
//! Kept thin because it needs Screen Recording and a real Mac; the hash and the choice of window are tested in shots.rs.
use super::shots;
use block2::RcBlock;
use objc2::{rc::Retained, AnyThread};
use objc2_core_foundation::{CFMutableData, CFRetained, CFString};
use objc2_core_graphics::{CGDataProvider, CGImage};
use objc2_foundation::NSError;
use objc2_image_io::CGImageDestination;
use objc2_screen_capture_kit::{SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration};
use serde::Serialize;
use std::{ptr::NonNull, sync::mpsc::channel, time::Duration};

/// ScreenCaptureKit replies on its own queue; a reply that never comes must not hang the capture thread.
const REPLY: Duration = Duration::from_secs(5);
/// Smaller than this is a palette or a popup, never the call.
const MIN_SIDE: f64 = 160.0;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Window {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub w: u32,
    pub h: u32,
}

/// The captured window as JPEG bytes, with the hash that tells near-duplicates apart.
pub struct Frame {
    pub jpeg: Vec<u8>,
    pub hash: u64,
}

/// ScreenCaptureKit objects are immutable snapshots, so handing one back from its reply queue is safe.
struct Handoff<T>(T);
unsafe impl<T> Send for Handoff<T> {}

fn describe(err: *mut NSError) -> String {
    unsafe { err.as_ref() }.map_or("ScreenCaptureKit failed".into(), |e| e.localizedDescription().to_string())
}

fn content() -> Result<Retained<SCShareableContent>, String> {
    let (tx, rx) = channel();
    let done = RcBlock::new(move |content: *mut SCShareableContent, err: *mut NSError| {
        let _ = tx.send(unsafe { Retained::retain(content) }.map(Handoff).ok_or_else(|| describe(err)));
    });
    unsafe { SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(true, true, &done) };
    rx.recv_timeout(REPLY).map_err(|_| "ScreenCaptureKit did not answer".to_string())?.map(|h| h.0)
}

/// Ordinary app windows that are on screen, other than Myna's own.
// shortcut: on-screen only, so a minimised window or one on another Space is not offered or captured; the frames
// resume when it is back. ScreenCaptureKit can capture those too if that turns out to matter.
pub fn windows() -> Result<Vec<Window>, String> {
    let own = std::process::id() as i32;
    let content = content()?;
    Ok(unsafe { content.windows() }
        .iter()
        .filter_map(|w| unsafe {
            let (frame, app) = (w.frame(), w.owningApplication()?);
            let ok = w.windowLayer() == 0 && frame.size.width >= MIN_SIDE && frame.size.height >= MIN_SIDE && app.processID() != own;
            ok.then(|| Window {
                id: w.windowID(),
                app: app.applicationName().to_string(),
                title: w.title().map(|t| t.to_string()).unwrap_or_default(),
                w: frame.size.width as u32,
                h: frame.size.height as u32,
            })
        })
        .collect())
}

/// Window `id` scaled to `width` px wide.
// shortcut: a window narrower than `width` is scaled up, not left at its own size. Cap at the window's pixel width
// if the files turn out larger than they need to be.
pub fn grab(id: u32, width: usize) -> Result<Frame, String> {
    let content = content()?;
    let window = unsafe { content.windows() }.iter().find(|w| unsafe { w.windowID() } == id).ok_or("The window is gone.")?;
    let frame = unsafe { window.frame() };
    let filter = unsafe { SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window) };
    let cfg = unsafe { SCStreamConfiguration::new() };
    unsafe {
        cfg.setWidth(width);
        cfg.setHeight((width as f64 * frame.size.height / frame.size.width.max(1.0)).round().max(1.0) as usize);
        cfg.setShowsCursor(false);
    }
    let (tx, rx) = channel();
    let done = RcBlock::new(move |image: *mut CGImage, err: *mut NSError| {
        let image = NonNull::new(image).map(|p| Handoff(unsafe { CFRetained::retain(p) }));
        let _ = tx.send(image.ok_or_else(|| describe(err)));
    });
    unsafe { SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(&filter, &cfg, Some(&done)) };
    let image = rx.recv_timeout(REPLY).map_err(|_| "ScreenCaptureKit did not answer".to_string())??.0;
    Ok(Frame { jpeg: jpeg(&image)?, hash: hash(&image)? })
}

fn jpeg(image: &CGImage) -> Result<Vec<u8>, String> {
    let data = CFMutableData::new(None, 0).ok_or("out of memory")?;
    let dest = unsafe { CGImageDestination::with_data(&data, &CFString::from_str("public.jpeg"), 1, None) }.ok_or("JPEG encoder unavailable")?;
    unsafe { dest.add_image(image, None) };
    if !unsafe { dest.finalize() } {
        return Err("JPEG encoding failed".into());
    }
    Ok(unsafe { data.as_bytes_unchecked() }.to_vec())
}

/// SDR frames arrive as 32-bit BGRA, which is what shots::hash_bgra reads.
fn hash(image: &CGImage) -> Result<u64, String> {
    let (w, h, stride) = (CGImage::width(Some(image)), CGImage::height(Some(image)), CGImage::bytes_per_row(Some(image)));
    if CGImage::bits_per_pixel(Some(image)) != 32 {
        return Err("the screenshot has a pixel format Myna does not handle".into());
    }
    let data = CGDataProvider::data(CGImage::data_provider(Some(image)).as_deref()).ok_or("no screenshot pixels")?;
    Ok(shots::hash_bgra(unsafe { data.as_bytes_unchecked() }, w, h, stride))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Hardware check, not run in CI: needs Screen Recording allowed for whatever runs the tests (the first run
    // waits on that prompt). `cargo test --lib grabs_a_window -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn grabs_a_window() {
        let windows = windows().unwrap();
        let w = windows.first().expect("an app window is open");
        eprintln!("grabbing {} — {}", w.app, w.title);
        let frame = grab(w.id, 1280).unwrap();
        eprintln!("{} bytes, hash {:016x}", frame.jpeg.len(), frame.hash);
        assert_eq!(&frame.jpeg[..2], [0xFF, 0xD8]);
    }
}
