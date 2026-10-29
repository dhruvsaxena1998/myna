//! Hardware capture: the mic through cpal, and the meeting app's audio through a per-process Core Audio tap.
//! Kept thin because it needs a real Mac and permissions; the logic around it is tested without hardware.
use super::track::TrackWriter;
use core_foundation::{
    array::CFArray,
    base::TCFType,
    boolean::CFBoolean,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SampleFormat, SizedSample, Stream,
};
use objc2::{rc::Retained, AnyThread};
use objc2_core_audio::*;
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_foundation::{NSArray, NSNumber};
use std::{
    ffi::c_void,
    path::Path,
    ptr::{null, null_mut, NonNull},
    sync::mpsc::{channel, Sender},
    thread::JoinHandle,
};

/// A running recording. Dropping it tears down the tap and the mic stream and closes both files,
/// so every exit path (stop, quit, panic) leaves nothing behind.
pub struct Capture {
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
    /// App whose audio is being tapped; None when only the mic is recorded.
    pub app: Option<String>,
    /// Why system audio is missing, when it is.
    pub note: Option<String>,
}

type Started = Result<(Option<String>, Option<String>), String>;

/// Writes `mic.wav` and `system.wav` into `dir`. The mic is required; losing the tap only costs the
/// "Them" track, so that failure is reported in `note` and the recording carries on.
// shortcut: the tapped app is chosen once, at start (whoever holds the mic then). Start the recording
// after joining the call; a per-meeting app picker belongs to the screenshots/window ticket.
// shortcut: the two tracks start within about a second of each other and are not time-aligned further; that
// only affects how close-together Me / Them lines interleave. Stamp each track's start if that shows up.
pub fn start(dir: &Path) -> Result<Capture, String> {
    let (ready_tx, ready_rx) = channel::<Started>();
    let (stop_tx, stop_rx) = channel::<()>();
    let dir = dir.to_path_buf();
    // Everything lives on one thread: cpal streams are not Send, and teardown order stays in one place.
    let thread = std::thread::spawn(move || {
        let mic = match Mic::start(&dir.join("mic.wav")) {
            Ok(m) => m,
            Err(e) => return drop(ready_tx.send(Err(e))),
        };
        let (tap, app, note) = match Tap::start(&dir.join("system.wav")) {
            Ok(Some((tap, app))) => (Some(tap), Some(app), None),
            Ok(None) => (None, None, Some("No app was using the mic, so only your mic was recorded.".to_string())),
            Err(e) => (None, None, Some(format!("System audio unavailable: {e}"))),
        };
        let _ = ready_tx.send(Ok((app, note)));
        let _ = stop_rx.recv();
        drop(tap);
        drop(mic);
    });
    match ready_rx.recv().map_err(|_| "capture thread died".to_string())? {
        Ok((app, note)) => Ok(Capture { stop: stop_tx, thread: Some(thread), app, note }),
        Err(e) => {
            let _ = thread.join();
            Err(e)
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// Field order matters: the stream stops before the writer closes the file.
struct Mic {
    _stream: Stream,
    _writer: TrackWriter,
}

impl Mic {
    fn start(path: &Path) -> Result<Self, String> {
        let device = cpal::default_host().default_input_device().ok_or("No microphone found.")?;
        let cfg = device.default_input_config().map_err(|e| e.to_string())?;
        let writer = TrackWriter::create(path, cfg.sample_rate())?;
        let (config, ch, tx) = (cfg.config(), cfg.channels() as usize, writer.sender());
        let stream = match cfg.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, config, ch, tx),
            SampleFormat::I16 => build::<i16>(&device, config, ch, tx),
            other => Err(format!("Unsupported microphone sample format {other:?}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self { _stream: stream, _writer: writer })
    }
}

fn build<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    tx: Sender<Vec<f32>>,
) -> Result<Stream, String>
where
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let samples: Vec<f32> = data.iter().map(|s| f32::from_sample(*s)).collect();
                let _ = tx.send(downmix(&[(&samples, channels)]));
            },
            |e| eprintln!("mic stream error: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

/// Averages every channel of every buffer into one mono stream. Each buffer is (samples, channels per frame).
fn downmix(bufs: &[(&[f32], usize)]) -> Vec<f32> {
    let frames = bufs.iter().map(|(s, ch)| s.len() / ch.max(&1)).min().unwrap_or(0);
    let total: usize = bufs.iter().map(|(_, ch)| *ch).sum();
    (0..frames)
        .map(|i| bufs.iter().map(|(s, ch)| s[i * ch..(i + 1) * ch].iter().sum::<f32>()).sum::<f32>() / total as f32)
        .collect()
}

fn check(status: i32, what: &str) -> Result<(), String> {
    if status == 0 { Ok(()) } else { Err(format!("{what} failed (OSStatus {status})")) }
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

/// Reads a fixed-size property (an integer, a struct, a CF reference).
fn property<T: Copy>(object: AudioObjectID, selector: u32, what: &str) -> Result<T, String> {
    let mut value: T = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<T>() as u32;
    let mut addr = address(selector);
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&mut addr),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    check(status, what).map(|_| value)
}

struct AudioProcess {
    object: AudioObjectID,
    pid: i32,
}

/// Processes for which `state` (IsRunningInput or IsRunningOutput) is on. With input that is the call app,
/// never the music player.
fn audio_processes(state: u32) -> Result<Vec<AudioProcess>, String> {
    let mut addr = address(kAudioHardwarePropertyProcessObjectList);
    let mut size = 0u32;
    check(
        unsafe { AudioObjectGetPropertyDataSize(SYSTEM, NonNull::from(&mut addr), 0, null(), NonNull::from(&mut size)) },
        "list audio processes",
    )?;
    let mut ids = vec![0 as AudioObjectID; size as usize / 4];
    check(
        unsafe {
            AudioObjectGetPropertyData(
                SYSTEM,
                NonNull::from(&mut addr),
                0,
                null(),
                NonNull::from(&mut size),
                NonNull::new(ids.as_mut_ptr().cast()).ok_or("no audio processes")?,
            )
        },
        "list audio processes",
    )?;
    ids.truncate(size as usize / 4);
    let own = std::process::id() as i32;
    Ok(ids
        .into_iter()
        .filter_map(|object| {
            let pid: i32 = property(object, kAudioProcessPropertyPID, "process id").ok()?;
            let on: u32 = property(object, state, "process audio state").ok()?;
            (on != 0 && pid != own).then_some(AudioProcess { object, pid })
        })
        .collect())
}

/// "Google Chrome" from the path of one of its helper processes.
fn app_name(exe_path: &str) -> Option<String> {
    exe_path.split('/').find_map(|part| part.strip_suffix(".app")).map(String::from)
}

fn process_label(pid: i32) -> String {
    let mut buf = [0u8; 4096];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let path = String::from_utf8_lossy(&buf[..n.max(0) as usize]).into_owned();
    app_name(&path).unwrap_or_else(|| format!("process {pid}"))
}

struct TapState {
    tx: Sender<Vec<f32>>,
}

unsafe extern "C-unwind" fn on_audio(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    state: *mut c_void,
) -> i32 {
    let state = &*(state as *const TapState);
    let list = input.as_ref();
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    let views: Vec<(&[f32], usize)> = buffers
        .iter()
        .filter(|b| !b.mData.is_null())
        .map(|b| (std::slice::from_raw_parts(b.mData as *const f32, b.mDataByteSize as usize / 4), b.mNumberChannels as usize))
        .collect();
    let _ = state.tx.send(downmix(&views));
    0
}

/// A process tap on the call app, read through a private aggregate device. Private objects belong to this
/// process, so even a crash cannot leave one behind; `Drop` covers every orderly exit.
struct Tap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    io_proc: AudioDeviceIOProcID,
    state: *mut TapState,
    // Closes the file after the device has stopped calling us.
    writer: Option<TrackWriter>,
}

impl Tap {
    /// None when no other app has the mic open.
    fn start(path: &Path) -> Result<Option<(Tap, String)>, String> {
        Self::attach(path, audio_processes(kAudioProcessPropertyIsRunningInput)?)
    }

    fn attach(path: &Path, procs: Vec<AudioProcess>) -> Result<Option<(Tap, String)>, String> {
        if procs.is_empty() {
            return Ok(None);
        }
        let mut labels: Vec<String> = procs.iter().map(|p| process_label(p.pid)).collect();
        labels.dedup();
        let objects: Vec<Retained<NSNumber>> = procs.iter().map(|p| NSNumber::new_u32(p.object)).collect();

        let mut t = Tap { tap: 0, aggregate: 0, io_proc: None, state: null_mut(), writer: None };
        unsafe {
            let desc = CATapDescription::initMonoMixdownOfProcesses(CATapDescription::alloc(), &NSArray::from_retained_slice(&objects));
            desc.setPrivate(true);
            check(AudioHardwareCreateProcessTap(Some(&desc), &mut t.tap), "create process tap")?;

            let key = |k: &std::ffi::CStr| CFString::new(k.to_str().unwrap());
            let output: AudioObjectID = property(SYSTEM, kAudioHardwarePropertyDefaultOutputDevice, "default output device")?;
            let output_uid = CFString::wrap_under_create_rule(property::<CFStringRef>(output, kAudioDevicePropertyDeviceUID, "output device id")?);
            let tap_entry = CFDictionary::from_CFType_pairs(&[
                (key(kAudioSubTapUIDKey), CFString::new(&desc.UUID().UUIDString().to_string()).as_CFType()),
                (key(kAudioSubTapDriftCompensationKey), CFBoolean::true_value().as_CFType()),
            ]);
            let sub_device = CFDictionary::from_CFType_pairs(&[(key(kAudioSubDeviceUIDKey), output_uid.as_CFType())]);
            let description = CFDictionary::from_CFType_pairs(&[
                (key(kAudioAggregateDeviceNameKey), CFString::new("Myna meeting tap").as_CFType()),
                // Unique per tap: destroying the previous aggregate device is asynchronous.
                (key(kAudioAggregateDeviceUIDKey), CFString::new(&format!("com.dhruvsaxena.myna.tap.{}", desc.UUID().UUIDString())).as_CFType()),
                (key(kAudioAggregateDeviceMainSubDeviceKey), output_uid.as_CFType()),
                (key(kAudioAggregateDeviceIsPrivateKey), CFBoolean::true_value().as_CFType()),
                (key(kAudioAggregateDeviceIsStackedKey), CFBoolean::false_value().as_CFType()),
                (key(kAudioAggregateDeviceTapAutoStartKey), CFBoolean::true_value().as_CFType()),
                (key(kAudioAggregateDeviceSubDeviceListKey), CFArray::from_CFTypes(&[sub_device]).as_CFType()),
                (key(kAudioAggregateDeviceTapListKey), CFArray::from_CFTypes(&[tap_entry]).as_CFType()),
            ]);
            check(
                AudioHardwareCreateAggregateDevice(&*description.as_concrete_TypeRef().cast(), NonNull::from(&mut t.aggregate)),
                "create aggregate device",
            )?;

            let format: AudioStreamBasicDescription = property(t.tap, kAudioTapPropertyFormat, "read tap format")?;
            // kAudioFormatFlagIsFloat
            if format.mFormatFlags & 1 == 0 || format.mBitsPerChannel != 32 {
                return Err("the tap delivers an audio format Myna does not handle".into());
            }
            let writer = TrackWriter::create(path, format.mSampleRate as u32)?;
            t.state = Box::into_raw(Box::new(TapState { tx: writer.sender() }));
            t.writer = Some(writer);
            check(
                AudioDeviceCreateIOProcID(t.aggregate, Some(on_audio), t.state.cast(), NonNull::from(&mut t.io_proc)),
                "create IO proc",
            )?;
            check(AudioDeviceStart(t.aggregate, t.io_proc), "start tap")?;
        }
        Ok(Some((t, labels.join(", "))))
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            if self.io_proc.is_some() {
                AudioDeviceStop(self.aggregate, self.io_proc);
                AudioDeviceDestroyIOProcID(self.aggregate, self.io_proc);
            }
            if self.aggregate != 0 {
                AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if self.tap != 0 {
                AudioHardwareDestroyProcessTap(self.tap);
            }
            if !self.state.is_null() {
                drop(Box::from_raw(self.state));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_interleaved_and_separate_channels() {
        assert_eq!(downmix(&[(&[1.0, 0.0, 1.0, 0.0], 2)]), [0.5, 0.5]);
        assert_eq!(downmix(&[(&[1.0, 1.0], 1), (&[0.0, 0.5], 1)]), [0.5, 0.75]);
        assert!(downmix(&[]).is_empty());
    }

    #[test]
    fn app_name_comes_from_the_outermost_app_bundle() {
        let helper = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper";
        assert_eq!(app_name(helper).as_deref(), Some("Google Chrome"));
        assert_eq!(app_name("/usr/sbin/coreaudiod"), None);
    }

    // Hardware check, not run in CI: needs System Audio Recording allowed for whatever runs the tests, and the
    // first run waits on that prompt. `cargo test --lib tap_hears -- --ignored --nocapture`. Plays a sound
    // with afplay and checks it lands in system.wav.
    #[test]
    #[ignore]
    fn tap_hears_a_playing_app() {
        let dir = std::env::temp_dir().join(format!("myna-tap-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut player = std::process::Command::new("sh")
            .args(["-c", "for i in 1 2 3 4 5 6; do afplay /System/Library/Sounds/Glass.aiff; done"])
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(800));
        let path = dir.join("system.wav");
        let (tap, label) = Tap::attach(&path, audio_processes(kAudioProcessPropertyIsRunningOutput).unwrap())
            .unwrap()
            .expect("something is playing audio");
        eprintln!("tapped: {label}");
        std::thread::sleep(std::time::Duration::from_secs(3));
        drop(tap);
        let _ = player.kill();
        let samples = super::super::track::read_track(&path).unwrap();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        eprintln!("{} samples, peak {peak}", samples.len());
        assert!(samples.len() > 16_000 * 2 && peak > 500);
    }
}
