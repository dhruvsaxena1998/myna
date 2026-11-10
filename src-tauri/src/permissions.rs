//! Microphone, Accessibility and System Audio: what macOS allows Myna, and asking for it.
//! In `tauri dev` the binary is unbundled, so macOS asks on behalf of the app that launched it (the terminal).

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use serde::Serialize;
use std::ffi::c_void;

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mic {
    /// macOS hasn't asked yet; asking shows its pop-up.
    Ask,
    Allowed,
    /// Denied or restricted: only System Settings can change it.
    Denied,
}

/// Recording other apps' sound (Screen & System Audio Recording), for a meeting's Them track.
#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Audio {
    Ask,
    Allowed,
    Denied,
    /// macOS has no public way to tell; it asks the first time a meeting records.
    Unknown,
}

#[derive(Serialize)]
pub struct State {
    pub mic: Mic,
    pub ax: bool,
    pub audio: Audio,
}

pub fn state() -> State {
    State { mic: mic(), ax: ax(), audio: audio() }
}

unsafe extern "C" {
    fn myna_audio_capture_state() -> isize;
    fn myna_audio_capture_ask() -> bool;
}

pub fn audio() -> Audio {
    match unsafe { myna_audio_capture_state() } {
        0 => Audio::Allowed,
        1 => Audio::Denied,
        2 => Audio::Ask,
        _ => Audio::Unknown,
    }
}

/// macOS's pop-up the first time; after that, or when Myna can't ask, the System Settings pane.
pub fn ask_audio() {
    if audio() != Audio::Ask || !unsafe { myna_audio_capture_ask() } {
        open_pane("Privacy_ScreenCapture"); // "Screen & System Audio Recording"
    }
}

pub fn mic() -> Mic {
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return Mic::Denied };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::NotDetermined => Mic::Ask,
        AVAuthorizationStatus::Authorized => Mic::Allowed,
        _ => Mic::Denied,
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

pub fn ax() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Shows macOS's pop-up once, then waits for the answer. Afterwards only System Settings can change it.
pub fn ask_mic() {
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return };
    let (tx, rx) = std::sync::mpsc::channel();
    let done = RcBlock::new(move |_: Bool| {
        let _ = tx.send(());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &done) };
    let _ = rx.recv();
}

/// Adds Myna to the Accessibility list (macOS only lists apps that asked) and opens that pane.
pub fn ask_ax() {
    use objc2_core_foundation::{kCFBooleanTrue, CFDictionary, CFString};
    let key = CFString::from_static_str("AXTrustedCheckOptionPrompt");
    let opts = CFDictionary::from_slices(&[&*key], &[unsafe { kCFBooleanTrue }.unwrap()]);
    unsafe { AXIsProcessTrustedWithOptions(&*opts as *const _ as *const c_void) };
    open_pane("Privacy_Accessibility");
}

pub fn open_pane(anchor: &str) {
    let url = format!("x-apple.systempreferences:com.apple.preference.security?{anchor}");
    let _ = std::process::Command::new("/usr/bin/open").arg(url).status();
}
