//! macOS notifications through the Swift bridge (`swift/Sources/MynaSwift/Notify.swift`).

use std::ffi::{CString, c_char};
use std::sync::OnceLock;

unsafe extern "C" {
    fn myna_notify_setup(clicked: extern "C" fn());
    fn myna_notify(title: *const c_char, body: *const c_char);
}

static CLICKED: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

extern "C" fn clicked() {
    if let Some(f) = CLICKED.get() {
        f();
    }
}

/// Once at launch: `on_click` runs when the user clicks one of Myna's notifications.
pub fn setup(on_click: impl Fn() + Send + Sync + 'static) {
    let _ = CLICKED.set(Box::new(on_click));
    unsafe { myna_notify_setup(clicked) };
}

/// Sends nothing outside a .app bundle, or when the user said no.
pub fn send(title: &str, body: &str) {
    if let (Ok(t), Ok(b)) = (CString::new(title), CString::new(body)) {
        unsafe { myna_notify(t.as_ptr(), b.as_ptr()) };
    }
}
