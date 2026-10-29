//! Paste into the focused app through the clipboard: save it, put the text there, send Cmd-V, put it back.
use arboard::{Clipboard, ImageData, SetExtApple};
use core_graphics::event::CGEventFlags;
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
// types a different shortcut. The keystroke method below is layout-independent, so those users can switch to it.
fn press_cmd_v() -> Result<(), String> {
    const KEY_V: u16 = 9;
    post_key(KEY_V, CGEventFlags::CGEventFlagCommand, None) // exactly Cmd, even if the hotkey's modifier is still held
}

/// One key press (down, up). `units` overrides what the key types, whatever the keyboard layout.
fn post_key(code: u16, flags: CGEventFlags, units: Option<&[u16]>) -> Result<(), String> {
    use core_graphics::{
        event::{CGEvent, CGEventTapLocation},
        event_source::{CGEventSource, CGEventSourceStateID},
    };
    let err = |_| "Could not create the paste keystroke".to_string();
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(err)?;
    for down in [true, false] {
        let key = CGEvent::new_keyboard_event(source.clone(), code, down).map_err(err)?;
        key.set_flags(flags);
        if let Some(u) = units {
            key.set_string_from_utf16_unchecked(u);
        }
        key.post(CGEventTapLocation::AnnotatedSession);
    }
    Ok(())
}

/// A key event carries few characters; apps drop the rest of a long string.
const CHUNK_UNITS: usize = 20;
const CHUNK_GAP: Duration = Duration::from_millis(3);

/// UTF-16 pieces of at most `CHUNK_UNITS`, never cutting a surrogate pair.
fn chunks(text: &str) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::new();
    let mut cur: Vec<u16> = Vec::new();
    for ch in text.chars() {
        let mut buf = [0; 2];
        let units = &*ch.encode_utf16(&mut buf);
        if cur.len() + units.len() > CHUNK_UNITS {
            out.push(std::mem::take(&mut cur));
        }
        cur.extend_from_slice(units);
    }
    out.extend((!cur.is_empty()).then_some(cur));
    out
}

/// For apps that reject paste: type the text as keystrokes. The clipboard is not touched.
pub fn type_text(text: &str) -> Result<(), String> {
    type_with(text, |units| post_key(0, CGEventFlags::empty(), Some(units)))
}

fn type_with(text: &str, mut post: impl FnMut(&[u16]) -> Result<(), String>) -> Result<(), String> {
    for c in chunks(text) {
        post(&c)?;
        sleep(CHUNK_GAP);
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

    #[test]
    fn typed_text_is_split_without_losing_or_cutting_characters() {
        let long = "word ".repeat(30) + "ends 😀😀😀😀😀😀😀😀😀😀 é";
        let mut got: Vec<Vec<u16>> = Vec::new();
        type_with(&long, |u| Ok(got.push(u.to_vec()))).unwrap();
        assert!(got.len() > 1 && got.iter().all(|c| c.len() <= CHUNK_UNITS));
        // Every piece decodes on its own (no split surrogate pair) and together they are the text.
        let joined: String = got.iter().map(|c| String::from_utf16(c).unwrap()).collect();
        assert_eq!(joined, long);
        assert!(chunks("").is_empty());
        assert_eq!(chunks("hi"), vec!["hi".encode_utf16().collect::<Vec<_>>()]);
    }

    #[test]
    fn typing_stops_at_the_first_failure() {
        let mut calls = 0;
        let r = type_with(&"x".repeat(100), |_| {
            calls += 1;
            Err("blocked".into())
        });
        assert_eq!((r, calls), (Err("blocked".into()), 1));
    }
}
