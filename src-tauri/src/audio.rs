//! Microphone capture (cpal) and the conversion to what the transcribers want: mono, 16 kHz.
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub const RATE: u32 = 16_000;

/// Audio as the device delivered it: interleaved samples at the device's own rate.
#[derive(Default)]
pub struct Clip {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub rate: u32,
}

pub fn to_mono_16k(c: &Clip) -> Vec<f32> {
    let ch = c.channels.max(1) as usize;
    let mono: Vec<f32> = c.samples.chunks(ch).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect();
    if c.rate == RATE || c.rate == 0 {
        return mono;
    }
    // shortcut: box-filter resample (average each output sample's source span), fine for speech;
    // upgrade to a polyphase resampler (rubato) if recognition suffers on 44.1 kHz inputs.
    let ratio = c.rate as f64 / RATE as f64;
    (0..(mono.len() as f64 / ratio) as usize)
        .map(|i| {
            let lo = (i as f64 * ratio) as usize;
            let hi = (((i + 1) as f64 * ratio).ceil() as usize).clamp(lo + 1, mono.len());
            mono[lo..hi].iter().sum::<f32>() / (hi - lo) as f32
        })
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// RMS to 0..1 for the pill meter: -50 dBFS and below is flat, 0 dBFS is full.
pub fn meter_level(rms: f32) -> f32 {
    ((20.0 * rms.max(1e-5).log10() + 50.0) / 50.0).clamp(0.0, 1.0)
}

/// A running capture. The cpal stream is not `Send`, so it lives on its own thread.
pub struct Recording {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Clip>,
}

impl Recording {
    /// Opens the default input and starts capturing; `on_level` is called ~20 times a second.
    pub fn start(on_level: impl Fn(f32) + Send + 'static) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let flag = stop.clone();
        let thread = thread::spawn(move || capture(flag, ready_tx, on_level));
        ready_rx.recv().map_err(|_| "The microphone thread died.".to_string())??;
        Ok(Recording { stop, thread })
    }

    pub fn stop(self) -> Result<Clip, String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().map_err(|_| "The microphone thread crashed.".to_string())
    }
}

fn capture(stop: Arc<AtomicBool>, ready: mpsc::Sender<Result<(), String>>, on_level: impl Fn(f32)) -> Clip {
    let samples = Arc::new(Mutex::new(Vec::new()));
    let level = Arc::new(AtomicU32::new(0));
    let (stream, channels, rate) = match open(samples.clone(), level.clone()) {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return Clip::default();
        }
    };
    let _ = ready.send(Ok(()));
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
        on_level(meter_level(f32::from_bits(level.load(Ordering::Relaxed))));
    }
    drop(stream); // releases the mic, so the system indicator goes off before transcription starts
    let samples = std::mem::take(&mut *samples.lock().unwrap());
    Clip { samples, channels, rate }
}

fn open(samples: Arc<Mutex<Vec<f32>>>, level: Arc<AtomicU32>) -> Result<(Stream, u16, u32), String> {
    let dev = cpal::default_host().default_input_device().ok_or("No microphone found.")?;
    let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => build::<f32>(&dev, cfg.config(), samples, level),
        SampleFormat::I16 => build::<i16>(&dev, cfg.config(), samples, level),
        SampleFormat::U16 => build::<u16>(&dev, cfg.config(), samples, level),
        f => Err(format!("Unsupported microphone format {f:?}.")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, cfg.channels(), cfg.sample_rate()))
}

fn build<T: SizedSample + Send + 'static>(
    dev: &cpal::Device,
    cfg: StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<Stream, String>
where
    f32: FromSample<T>,
{
    dev.build_input_stream(
        cfg,
        move |data: &[T], _| {
            let mut buf = samples.lock().unwrap();
            let from = buf.len();
            buf.extend(data.iter().map(|s| f32::from_sample(*s)));
            level.store(rms(&buf[from..]).to_bits(), Ordering::Relaxed);
        },
        |e| eprintln!("microphone stream error: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(samples: Vec<f32>, channels: u16, rate: u32) -> Clip {
        Clip { samples, channels, rate }
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        let out = to_mono_16k(&clip(vec![1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, RATE));
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn resampling_scales_length_and_keeps_level() {
        // 1 s of a constant at 48 kHz -> 16 000 samples of the same value.
        let out = to_mono_16k(&clip(vec![0.25; 48_000], 1, 48_000));
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
        // 44.1 kHz is not an integer ratio: length still lands within a sample of 1 s.
        let out = to_mono_16k(&clip(vec![0.25; 44_100], 1, 44_100));
        assert!((out.len() as i64 - 16_000).abs() <= 1);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn lower_rates_do_not_panic() {
        let out = to_mono_16k(&clip(vec![0.5; 8_000], 1, 8_000));
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn meter_follows_loudness() {
        assert_eq!(meter_level(0.0), 0.0);
        assert_eq!(meter_level(1.0), 1.0);
        assert!((meter_level(0.01) - 0.2).abs() < 1e-3); // -40 dBFS
        assert!((meter_level(0.1) - 0.6).abs() < 1e-3); // -20 dBFS, normal speech
        assert!(meter_level(0.05) > meter_level(0.01));
    }
}
