//! Paste into the focused app through the clipboard: save it, put the text there, send Cmd-V, put it back.
use arboard::{Clipboard, ImageData, SetExtApple};
use std::{thread::sleep, time::Duration};

const DEFAULT_DELAY_MS: u64 = 150;
const MAX_DELAY_MS: u64 = 2_000;

/// Wait between Cmd-V and restoring the clipboard: apps read it asynchronously, and a slow one
/// would paste the old contents if it were restored too soon. Read from the `paste_delay_ms` setting.
pub fn delay(setting: Option<String>) -> Duration {
    let ms = setting.and_then(|s| s.trim().parse().ok()).unwrap_or(DEFAULT_DELAY_MS);
    Duration::from_millis(ms.min(MAX_DELAY_MS))
}

pub enum Saved {
    Text(String),
    Image(ImageData<'static>),
    Empty,
}

pub trait Board {
    fn save(&mut self) -> Saved;
    fn put_text(&mut self, text: &str) -> Result<(), String>;
    fn restore(&mut self, saved: Saved);
}

impl Board for Clipboard {
    // shortcut: restores text or an image only; files and rich text are lost (a file copied in Finder
    // comes back empty). Upgrade to snapshotting every NSPasteboard item if that bites.
    fn save(&mut self) -> Saved {
        if let Ok(t) = self.get_text() {
            Saved::Text(t)
        } else if let Ok(i) = self.get_image() {
            Saved::Image(i)
        } else {
            Saved::Empty
        }
    }

    fn put_text(&mut self, text: &str) -> Result<(), String> {
        // Concealed, so clipboard managers do not keep what was dictated.
        self.set().exclude_from_history().text(text).map_err(|e| e.to_string())
    }

    fn restore(&mut self, saved: Saved) {
        let _ = match saved {
            Saved::Text(t) => self.set_text(t),
            Saved::Image(i) => self.set_image(i),
            Saved::Empty => self.clear(),
        };
    }
}

/// `send_paste` presses Cmd-V. The clipboard is restored after `delay` even if that fails.
pub fn paste(
    board: &mut impl Board,
    text: &str,
    delay: Duration,
    send_paste: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let saved = board.save();
    board.put_text(text)?;
    let sent = send_paste();
    sleep(delay);
    board.restore(saved);
    sent
}

pub fn paste_text(text: &str, delay: Duration) -> Result<(), String> {
    let mut board = Clipboard::new().map_err(|e| e.to_string())?;
    paste(&mut board, text, delay, press_cmd_v)
}

/// For when pasting is not possible: leave the text on the clipboard so nothing dictated is lost.
pub fn copy_only(text: &str) -> Result<(), String> {
    Clipboard::new().and_then(|mut b| b.set_text(text)).map_err(|e| e.to_string())
}

// shortcut: keycode 9 is the physical key labelled V on a QWERTY board; on Dvorak and similar layouts it
// types a different shortcut. Needs a layout-aware lookup, which belongs with the keystroke fallback.
fn press_cmd_v() -> Result<(), String> {
    use core_graphics::{
        event::{CGEvent, CGEventFlags, CGEventTapLocation},
        event_source::{CGEventSource, CGEventSourceStateID},
    };
    const KEY_V: u16 = 9;
    let err = |_| "Could not create the paste keystroke".to_string();
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(err)?;
    for down in [true, false] {
        let key = CGEvent::new_keyboard_event(source.clone(), KEY_V, down).map_err(err)?;
        key.set_flags(CGEventFlags::CGEventFlagCommand); // exactly Cmd, even if the hotkey's modifier is still held
        key.post(CGEventTapLocation::AnnotatedSession);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    struct Fake(Rc<RefCell<Option<String>>>);
    impl Board for Fake {
        fn save(&mut self) -> Saved {
            self.0.borrow().clone().map_or(Saved::Empty, Saved::Text)
        }
        fn put_text(&mut self, text: &str) -> Result<(), String> {
            *self.0.borrow_mut() = Some(text.into());
            Ok(())
        }
        fn restore(&mut self, saved: Saved) {
            *self.0.borrow_mut() = match saved {
                Saved::Text(t) => Some(t),
                _ => None,
            };
        }
    }

    fn board(initial: Option<&str>) -> (Fake, Rc<RefCell<Option<String>>>) {
        let cell = Rc::new(RefCell::new(initial.map(String::from)));
        (Fake(cell.clone()), cell)
    }

    #[test]
    fn text_is_on_the_clipboard_only_while_the_keystroke_fires() {
        let (mut b, cell) = board(Some("my old clipboard"));
        let seen = Rc::new(RefCell::new(None));
        let (c, s) = (cell.clone(), seen.clone());
        paste(&mut b, "dictated words", Duration::ZERO, move || {
            *s.borrow_mut() = c.borrow().clone();
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.borrow().as_deref(), Some("dictated words"));
        assert_eq!(cell.borrow().as_deref(), Some("my old clipboard"));
    }

    #[test]
    fn empty_clipboard_is_left_empty() {
        let (mut b, cell) = board(None);
        paste(&mut b, "x", Duration::ZERO, || Ok(())).unwrap();
        assert_eq!(*cell.borrow(), None);
    }

    #[test]
    fn clipboard_is_restored_when_the_keystroke_fails() {
        let (mut b, cell) = board(Some("old"));
        let r = paste(&mut b, "x", Duration::ZERO, || Err("no key".into()));
        assert_eq!(r, Err("no key".into()));
        assert_eq!(cell.borrow().as_deref(), Some("old"));
    }

    #[test]
    fn delay_setting_is_parsed_and_bounded() {
        let ms = |s: Option<&str>| delay(s.map(String::from)).as_millis();
        assert_eq!(ms(None), 150);
        assert_eq!(ms(Some("300")), 300);
        assert_eq!(ms(Some(" 0 ")), 0);
        assert_eq!(ms(Some("soon")), 150);
        assert_eq!(ms(Some("-5")), 150);
        assert_eq!(ms(Some("999999")), 2_000);
    }
}
