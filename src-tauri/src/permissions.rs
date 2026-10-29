use block2::RcBlock;
use core_foundation::{
    base::TCFType, boolean::CFBoolean, dictionary::CFDictionary, string::{CFString, CFStringRef},
};
use objc2::{msg_send, runtime::{AnyClass, AnyObject, Bool}};

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {
    static AVMediaTypeAudio: &'static AnyObject;
}
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

fn av_device() -> &'static AnyClass {
    AnyClass::get(c"AVCaptureDevice").expect("AVFoundation not loaded")
}

pub fn granted(name: &str) -> bool {
    unsafe {
        match name {
            // AVAuthorizationStatusAuthorized == 3
            "Microphone" => {
                let s: isize = msg_send![av_device(), authorizationStatusForMediaType: AVMediaTypeAudio];
                s == 3
            }
            "Accessibility" => AXIsProcessTrusted(),
            "Screen Recording" => CGPreflightScreenCaptureAccess(),
            _ => false,
        }
    }
}

// Triggers the system prompt. Mic prompts asynchronously; Accessibility and Screen Recording
// open System Settings. Callers re-check status via `granted` afterwards.
pub fn request(name: &str) {
    unsafe {
        match name {
            "Microphone" => {
                let done = RcBlock::new(|_: Bool| {});
                let _: () = msg_send![av_device(), requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*done];
            }
            "Accessibility" => {
                let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
                let opts = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
                AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef() as *const _);
            }
            "Screen Recording" => {
                CGRequestScreenCaptureAccess();
            }
            _ => {}
        }
    }
}
