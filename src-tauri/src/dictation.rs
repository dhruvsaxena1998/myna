//! Dictation: record the mic while the hotkey is held, turn it into text, paste it into the focused app.

use crate::runtime::{Transcriber, RATE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// A mic recording in progress, on its own thread (a cpal stream can't move between threads).
pub struct Recording {
    stop: Sender<()>,
    done: Receiver<Result<Heard, String>>,
    /// Loudness of the latest mic buffer, 0 to 1, as f32 bits.
    level: Arc<AtomicU32>,
}

/// Interleaved samples, sample rate, channels.
type Heard = (Vec<f32>, u32, u16);

pub fn record() -> Recording {
    let (stop, stop_rx) = channel();
    let (done_tx, done) = channel();
    let level = Arc::new(AtomicU32::new(0));
    let lv = level.clone();
    std::thread::spawn(move || {
        let run = || {
            let dev = cpal::default_host().default_input_device().ok_or("No microphone found.")?;
            let cfg = dev.default_input_config().map_err(|e| format!("Couldn't open the microphone: {e}"))?;
            // shortcut: f32 only, which is what Core Audio gives; convert other formats if a mic ever reports one.
            if cfg.sample_format() != cpal::SampleFormat::F32 {
                return Err(format!("The microphone gives {} samples, which Myna can't read yet.", cfg.sample_format()));
            }
            let buf = Arc::new(Mutex::new(Vec::new()));
            let b = buf.clone();
            let stream = dev
                .build_input_stream(
                    cfg.config(),
                    move |data: &[f32], _: &_| {
                        lv.store(loudness(data).to_bits(), Ordering::Relaxed);
                        b.lock().unwrap().extend_from_slice(data);
                    },
                    |e| eprintln!("microphone: {e}"),
                    None,
                )
                .map_err(|e| format!("Couldn't open the microphone: {e}"))?;
            stream.play().map_err(|e| format!("Couldn't start the microphone: {e}"))?;
            let _ = stop_rx.recv(); // a message, or the Recording dropped
            drop(stream);
            let samples = std::mem::take(&mut *buf.lock().unwrap());
            Ok((samples, cfg.sample_rate(), cfg.channels()))
        };
        let _ = done_tx.send(run());
    });
    Recording { stop, done, level }
}

/// How loud `samples` are, 0 (quieter than -55 dBFS) to 1 (-10 dBFS and up), for the pill's level bars.
pub fn loudness(samples: &[f32]) -> f32 {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    ((20.0 * rms.max(1e-9).log10() + 55.0) / 45.0).clamp(0.0, 1.0)
}

impl Recording {
    /// The mic's current loudness, 0 to 1.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Stops the mic and returns what it heard as 16 kHz mono.
    pub fn finish(self) -> Result<Vec<f32>, String> {
        let _ = self.stop.send(());
        let (samples, rate, channels) = self.done.recv().map_err(|_| "The microphone stopped unexpectedly.")??;
        Ok(to_16k_mono(&samples, rate, channels))
    }
}

/// Downmixes interleaved audio and resamples it to the 16 kHz the models take.
// shortcut: box-filter resampling (each output sample averages its input window); fine for speech,
// switch to a sinc resampler if accuracy drops on 44.1 kHz mics.
pub fn to_16k_mono(input: &[f32], rate: u32, channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    let mono: Vec<f32> = input.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    if rate == RATE {
        return mono;
    }
    let step = rate as f64 / RATE as f64;
    let n = (mono.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * step) as usize;
            let b = (((i + 1) as f64 * step) as usize).clamp(a + 1, mono.len());
            mono[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// The speech-to-text step's text for this 16 kHz mono audio: the model's segments joined into one line.
/// Rules cleanup runs on it next; an empty result after that means nothing was heard.
pub fn text(t: &mut dyn Transcriber, audio: &[f32]) -> Result<String, String> {
    let segs = t.transcribe(audio)?;
    let words: Vec<_> = segs.iter().map(|s| s.text.trim()).filter(|s| !s.is_empty()).collect();
    Ok(words.join(" "))
}

/// What `paste` fails with when Accessibility is off.
pub const NO_AX: &str = "Myna needs Accessibility to add your words.";

/// Adds `text` to the focused app: typed out key by key if `typed`, else pasted through the clipboard.
pub fn paste(text: &str, typed: bool) -> Result<(), String> {
    // Check first: without Accessibility the keystrokes go nowhere, and the user's clipboard must stay as it was.
    if !crate::permissions::ax() {
        return Err(NO_AX.into());
    }
    if typed { type_out(text) } else { via_clipboard(text) }
}

/// Copy again, from Voice typing history: `text` on the clipboard, in place of what was there.
pub fn copy(text: &str) {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    pb.setString_forType(&objc2_foundation::NSString::from_str(text), unsafe { NSPasteboardTypeString });
}

/// Puts `text` on the clipboard, sends Cmd-V, then puts back whatever was on it.
fn via_clipboard(text: &str) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString};

    let pb = NSPasteboard::generalPasteboard();
    // Copy every item in every type (text, images, files), not just text.
    let saved: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = pb
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let copy = NSPasteboardItem::new();
                    for t in item.types().iter() {
                        if let Some(d) = item.dataForType(&t) {
                            copy.setData_forType(&d, &t);
                        }
                    }
                    ProtocolObject::from_retained(copy)
                })
                .collect()
        })
        .unwrap_or_default();

    pb.clearContents();
    pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
    let ours = pb.changeCount();
    cmd_v()?;
    // shortcut: a fixed wait for the app to read the clipboard; add a setting only if an app needs longer.
    std::thread::sleep(std::time::Duration::from_millis(300));
    if pb.changeCount() == ours {
        // Unless the user copied something meanwhile.
        pb.clearContents();
        if !saved.is_empty() {
            pb.writeObjects(&NSArray::from_retained_slice(&saved));
        }
    }
    Ok(())
}

fn cmd_v() -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    // A private source, so modifiers still held from the hotkey (Option) don't join the Cmd-V.
    let src = CGEventSource::new(CGEventSourceStateID::Private).map_err(|_| "Couldn't send the paste keystroke.")?;
    // shortcut: key code 9 is V on QWERTY-shaped layouts; Dvorak users without "QWERTY ⌘" need the Type method.
    for down in [true, false] {
        let e = CGEvent::new_keyboard_event(src.clone(), 9, down).map_err(|_| "Couldn't send the paste keystroke.")?;
        e.set_flags(CGEventFlags::CGEventFlagCommand);
        e.post(CGEventTapLocation::HID);
    }
    Ok(())
}

/// One keystroke's worth of typing: up to `TYPE_UNITS` UTF-16 units of text, or the Return key.
#[derive(Debug, PartialEq)]
enum Key<'a> {
    Text(&'a str),
    Return,
}

/// A key event carries at most 20 UTF-16 units; an emoji is two, and never gets split.
const TYPE_UNITS: usize = 20;

fn keys(text: &str) -> Vec<Key<'_>> {
    let mut out = vec![];
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(Key::Return);
        }
        let (mut start, mut units) = (0, 0);
        for (at, c) in line.char_indices() {
            if units + c.len_utf16() > TYPE_UNITS {
                out.push(Key::Text(&line[start..at]));
                (start, units) = (at, 0);
            }
            units += c.len_utf16();
        }
        if start < line.len() {
            out.push(Key::Text(&line[start..]));
        }
    }
    out
}

/// Types `text` as keystrokes, for apps that block pasting.
fn type_out(text: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    const FAIL: &str = "Couldn't type your words.";
    // A private source, so modifiers still held from the hotkey don't join the keys.
    let src = CGEventSource::new(CGEventSourceStateID::Private).map_err(|_| FAIL)?;
    for key in keys(text) {
        for down in [true, false] {
            // Return is key code 36; text rides on a blank key event as its Unicode string.
            let e = CGEvent::new_keyboard_event(src.clone(), if key == Key::Return { 36 } else { 0 }, down).map_err(|_| FAIL)?;
            if let Key::Text(t) = key {
                e.set_string(t);
            }
            e.post(CGEventTapLocation::HID);
        }
        // shortcut: a fixed pause so apps don't drop keys; long text takes a moment, speed it up if that's felt.
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Fake, Segment};

    #[test]
    fn stereo_48k_becomes_16k_mono() {
        // One second of left 0.2 / right 0.6.
        let input: Vec<f32> = (0..48_000).flat_map(|_| [0.2, 0.6]).collect();
        let out = to_16k_mono(&input, 48_000, 2);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.4).abs() < 1e-6));
        assert_eq!(to_16k_mono(&[0.1, 0.2], RATE, 1), [0.1, 0.2]);
        assert_eq!(to_16k_mono(&vec![0.5; 44_100], 44_100, 1).len(), 16_000);
    }

    #[test]
    fn loudness_spans_quiet_to_loud() {
        assert_eq!(loudness(&[]), 0.0);
        assert_eq!(loudness(&[0.0; 64]), 0.0);
        assert_eq!(loudness(&[0.5, -0.5]), 1.0);
        let mid = loudness(&[0.01, -0.01]); // -40 dBFS
        assert!((mid - 1.0 / 3.0).abs() < 1e-3, "{mid}");
    }

    fn heard(segs: &[&str], settings: &crate::rules::Settings) -> String {
        let r = hound::WavReader::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two-speakers.wav")).unwrap();
        let (rate, ch) = (r.spec().sample_rate, r.spec().channels);
        let samples: Vec<f32> = r.into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        let seg = |text: &str| Segment { start: 0.0, end: 1.0, text: text.into() };
        let mut t = Fake(segs.iter().map(|s| seg(s)).collect());
        crate::rules::clean(&text(&mut t, &to_16k_mono(&samples, rate, ch)).unwrap(), settings)
    }

    #[test]
    fn a_recording_becomes_the_text_to_paste() {
        let plain = crate::rules::Settings::default();
        assert_eq!(heard(&[" The report is ready. ", "", "Thanks."], &plain), "The report is ready. Thanks.");
    }

    #[test]
    fn rules_run_between_speech_to_text_and_paste_and_all_filler_is_nothing_heard() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(tmp.path()).unwrap();
        let mut s = crate::rules::Settings::default();
        s.words.push(crate::rules::Word { spelling: "Postgres".into(), heard: None });
        crate::rules::save(&conn, &s).unwrap();
        let s = crate::rules::load(&conn).unwrap(); // the settings come back from the database
        assert_eq!(heard(&[" Uh, the the report", "is on post gres."], &s), "The report is on Postgres.");
        // Nothing left to paste: the command layer shows "didn't hear anything" for an empty text.
        assert_eq!(heard(&["Hmm,", " um."], &s), "");
    }

    #[test]
    fn typing_splits_text_without_cutting_characters_and_sends_newlines_as_return() {
        assert_eq!(keys("Hello"), [Key::Text("Hello")]);
        assert_eq!(keys("a\nb\n"), [Key::Text("a"), Key::Return, Key::Text("b"), Key::Return]);
        assert_eq!(keys(""), []);
        // 21 letters: 20 then 1. Emoji are 2 units each, so 10 fit; é stays whole.
        assert_eq!(keys(&"x".repeat(21)), [Key::Text(&"x".repeat(20)), Key::Text("x")]);
        let smileys = "😀".repeat(11);
        assert_eq!(keys(&smileys), [Key::Text(&"😀".repeat(10)), Key::Text("😀")]);
        let long = "é😀 ".repeat(40);
        let parts = keys(&long);
        assert!(parts.iter().all(|k| matches!(k, Key::Text(t) if t.encode_utf16().count() <= TYPE_UNITS)));
        let joined: String = parts.iter().map(|k| if let Key::Text(t) = k { *t } else { "" }).collect();
        assert_eq!(joined, long);
    }
}
