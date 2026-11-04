//! fn as voice typing's key. macOS's global shortcuts can't hold fn on its own, so an event tap
//! watches modifier changes for it. The tap may change events (it never does), which makes its
//! permission Accessibility, the one voice typing already has; a listen-only tap would need Input Monitoring.

use core_foundation::base::TCFType;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, CallbackResult};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
}

// shortcut: fn with another key (fn+Delete, fn+arrows) still counts as fn, since key presses aren't watched;
// watch key-downs too and drop the dictation if that trips people up.
/// fn going down (`Some(true)`) or up (`Some(false)`), from the flags of a modifier change; `None` if
/// fn didn't change (another modifier did).
fn edge(was_down: bool, flags: CGEventFlags) -> Option<bool> {
    let down = flags.contains(CGEventFlags::CGEventFlagSecondaryFn);
    (down != was_down).then_some(down)
}

/// Calls `on(down)` each time fn goes down or up, from its own thread so the tap never waits on it.
/// Starts once; later calls do nothing. Until Accessibility is allowed macOS refuses the tap, so it
/// tries again every second.
pub fn watch(on: impl Fn(bool) + Send + 'static) {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || rx.into_iter().for_each(&on));
        std::thread::spawn(move || run(tx));
    });
}

fn run(tx: mpsc::Sender<bool>) {
    static PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
    static DOWN: AtomicBool = AtomicBool::new(false);
    let tap = loop {
        let tx = tx.clone();
        let made = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![CGEventType::FlagsChanged],
            move |_, kind, event| {
                match kind {
                    // macOS switches a slow or interrupted tap off; switch it back on.
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => unsafe { CGEventTapEnable(PORT.load(Ordering::SeqCst), true) },
                    CGEventType::FlagsChanged => {
                        if let Some(down) = edge(DOWN.load(Ordering::SeqCst), event.get_flags()) {
                            DOWN.store(down, Ordering::SeqCst);
                            let _ = tx.send(down);
                        }
                    }
                    _ => {}
                }
                CallbackResult::Keep
            },
        );
        match made {
            Ok(tap) => break tap,
            Err(()) => std::thread::sleep(std::time::Duration::from_secs(1)),
        }
    };
    PORT.store(tap.mach_port().as_concrete_TypeRef().cast(), Ordering::SeqCst);
    let Ok(source) = tap.mach_port().create_runloop_source(0) else { return eprintln!("dictation: fn can't be watched") };
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();
    CFRunLoop::run_current();
}

/// Whether "Press 🌐 key to" (or "Press fn key to") in Keyboard settings is Do Nothing. Anything
/// else (emoji, input source, Dictation) fires along with voice typing on every press of fn.
pub fn globe_ok() -> bool {
    let out = std::process::Command::new("/usr/bin/defaults").args(["read", "com.apple.HIToolbox", "AppleFnUsageType"]).output();
    out.is_ok_and(|o| do_nothing(&String::from_utf8_lossy(&o.stdout)))
}

/// `defaults read` prints 0 for Do Nothing. A Mac that never changed the setting has no value,
/// which means macOS's default (emoji or input source).
fn do_nothing(out: &str) -> bool {
    out.trim() == "0"
}

pub fn open_keyboard_settings() {
    let _ = std::process::Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.Keyboard-Settings.extension").status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fn_changing_counts() {
        let fn_flag = CGEventFlags::CGEventFlagSecondaryFn;
        let shift = CGEventFlags::CGEventFlagShift;
        assert_eq!(edge(false, fn_flag), Some(true));
        assert_eq!(edge(true, fn_flag | shift), None); // Shift while fn is held
        assert_eq!(edge(true, shift), Some(false));
        assert_eq!(edge(false, shift), None);
    }

    #[test]
    fn only_zero_is_do_nothing() {
        assert!(do_nothing("0\n"));
        for out in ["", "1\n", "2\n", "3\n"] {
            assert!(!do_nothing(out), "{out:?}");
        }
    }
}
