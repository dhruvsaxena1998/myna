//! Dictation: record the mic while the hotkey is held, turn it into text, paste it into the focused app.

use crate::runtime::{Transcriber, RATE};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// A mic recording in progress, on its own thread (a cpal stream can't move between threads).
pub struct Recording {
    stop: Sender<()>,
    done: Receiver<Result<Heard, String>>,
}

/// Interleaved samples, sample rate, channels.
type Heard = (Vec<f32>, u32, u16);

pub fn record() -> Recording {
    let (stop, stop_rx) = channel();
    let (done_tx, done) = channel();
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
                    move |data: &[f32], _: &_| b.lock().unwrap().extend_from_slice(data),
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
    Recording { stop, done }
}

impl Recording {
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

/// What gets pasted for this 16 kHz mono audio: the model's segments joined into one line.
pub fn text(t: &mut dyn Transcriber, audio: &[f32]) -> Result<String, String> {
    let segs = t.transcribe(audio)?;
    let words: Vec<_> = segs.iter().map(|s| s.text.trim()).filter(|s| !s.is_empty()).collect();
    Ok(words.join(" "))
}

/// Pastes `text` into the focused app through the clipboard, then puts back whatever was on it.
pub fn paste(text: &str) -> Result<(), String> {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString};

    // Check first: without Accessibility the Cmd-V goes nowhere, and the user's clipboard must stay as it was.
    if !crate::permissions::ax() {
        return Err("Myna needs Accessibility to add your words. Allow it on Settings › Voice typing.".into());
    }
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
    // shortcut: fixed wait for the app to read the clipboard; ticket 04.6 makes it a setting.
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
    // shortcut: key code 9 is V on QWERTY-shaped layouts; Dvorak users without "QWERTY ⌘" need the typing fallback (04.6).
    for down in [true, false] {
        let e = CGEvent::new_keyboard_event(src.clone(), 9, down).map_err(|_| "Couldn't send the paste keystroke.")?;
        e.set_flags(CGEventFlags::CGEventFlagCommand);
        e.post(CGEventTapLocation::HID);
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
    fn a_recording_becomes_the_text_to_paste() {
        let r = hound::WavReader::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/two-speakers.wav")).unwrap();
        let (rate, ch) = (r.spec().sample_rate, r.spec().channels);
        let samples: Vec<f32> = r.into_samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect();
        let seg = |text: &str| Segment { start: 0.0, end: 1.0, text: text.into() };
        let mut t = Fake(vec![seg(" The report is ready. "), seg(""), seg("Thanks.")]);
        assert_eq!(text(&mut t, &to_16k_mono(&samples, rate, ch)).unwrap(), "The report is ready. Thanks.");
    }
}
