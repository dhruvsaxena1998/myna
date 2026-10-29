//! Track files: 16 kHz mono 16-bit WAV. Written live from the capture callbacks, read back for transcription.
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, RecvTimeoutError, Sender},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub const RATE: u32 = 16_000;

/// Streaming windowed-sinc resampler to `RATE`. The low-pass sits at the output Nyquist,
/// so a 48 kHz mic does not alias into the speech band.
pub struct Resampler {
    step: f64,
    half: f64,
    pos: f64,
    buf: Vec<f32>,
}

const ZEROS: f64 = 8.0;

impl Resampler {
    pub fn new(in_rate: u32) -> Self {
        let step = in_rate as f64 / RATE as f64;
        Self { step, half: ZEROS * step.max(1.0), pos: 0.0, buf: Vec::new() }
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(input);
        let cutoff = 1.0 / self.step.max(1.0);
        while self.pos + self.half < self.buf.len() as f64 {
            let lo = (self.pos - self.half).ceil().max(0.0) as usize;
            let hi = (self.pos + self.half).floor() as usize;
            let (mut acc, mut wsum) = (0.0f64, 0.0f64);
            for i in lo..=hi {
                let d = i as f64 - self.pos;
                let x = std::f64::consts::PI * cutoff * d;
                let sinc = if x == 0.0 { 1.0 } else { x.sin() / x };
                let w = sinc * 0.5 * (1.0 + (std::f64::consts::PI * d / self.half).cos());
                acc += w * self.buf[i] as f64;
                wsum += w;
            }
            out.push((acc / wsum) as f32);
            self.pos += self.step;
        }
        let drop = ((self.pos - self.half).floor().max(0.0)) as usize;
        self.buf.drain(..drop);
        self.pos -= drop as f64;
    }
}

/// Owns a WAV file and a thread that resamples and writes whatever the audio callback sends.
/// The header is refreshed every few seconds, so a crash leaves a file readable up to the last refresh.
pub struct TrackWriter {
    tx: Sender<Vec<f32>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl TrackWriter {
    /// `in_rate` is the sample rate of the mono chunks that will be sent.
    pub fn create(path: &Path, in_rate: u32) -> Result<Self, String> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let wav = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            move || write_loop(wav, rx, stop, in_rate)
        });
        Ok(Self { tx, stop, thread: Some(thread) })
    }

    pub fn sender(&self) -> Sender<Vec<f32>> {
        self.tx.clone()
    }

    /// Writes what is queued and finalizes the file. Also runs on drop, so every exit path closes the file.
    pub fn finish(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Relaxed);
        match self.thread.take() {
            Some(t) => t.join().map_err(|_| "track writer panicked".to_string())?,
            None => Ok(()),
        }
    }
}

impl Drop for TrackWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn write_loop(
    mut wav: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    rx: Receiver<Vec<f32>>,
    stop: Arc<AtomicBool>,
    in_rate: u32,
) -> Result<(), String> {
    let mut rs = Resampler::new(in_rate);
    let mut out = Vec::new();
    let mut flushed = Instant::now();
    let mut write = |chunk: Vec<f32>, wav: &mut hound::WavWriter<_>| -> Result<(), String> {
        out.clear();
        rs.process(&chunk, &mut out);
        for s in &out {
            wav.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => write(chunk, &mut wav)?,
            Err(RecvTimeoutError::Timeout) if !stop.load(Ordering::Relaxed) => {}
            Err(_) => break,
        }
        if flushed.elapsed() > Duration::from_secs(5) {
            wav.flush().map_err(|e| e.to_string())?;
            flushed = Instant::now();
        }
    }
    while let Ok(chunk) = rx.try_recv() {
        write(chunk, &mut wav)?;
    }
    wav.finalize().map_err(|e| e.to_string())
}

pub fn read_track(path: &Path) -> Result<Vec<i16>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    r.samples::<i16>().collect::<Result<_, _>>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, rate: u32, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() as f32)
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
    }

    fn resample(input: &[f32], rate: u32) -> Vec<f32> {
        let mut out = Vec::new();
        let mut rs = Resampler::new(rate);
        // Feed in odd-sized chunks, the way a device callback would.
        for c in input.chunks(1021) {
            rs.process(c, &mut out);
        }
        out
    }

    #[test]
    fn speech_band_survives_and_length_follows_the_ratio() {
        for rate in [48_000, 44_100] {
            let out = resample(&tone(1000.0, rate, 1.0), rate);
            assert!((out.len() as i64 - 16_000).abs() < 40, "{rate}: {}", out.len());
            let mid = &out[500..out.len() - 500];
            assert!((rms(mid) - 0.707).abs() < 0.02, "{rate}: {}", rms(mid));
        }
    }

    #[test]
    fn content_above_the_new_nyquist_is_removed_not_folded_down() {
        // 12 kHz would alias to 4 kHz at 16 kHz without the low-pass.
        let out = resample(&tone(12_000.0, 48_000, 1.0), 48_000);
        assert!(rms(&out[500..out.len() - 500]) < 0.05);
    }

    #[test]
    fn writer_resamples_to_16k_and_closes_the_file() {
        let path = std::env::temp_dir().join(format!("myna-track-{}.wav", std::process::id()));
        let mut w = TrackWriter::create(&path, 48_000).unwrap();
        let tx = w.sender();
        for c in tone(440.0, 48_000, 1.0).chunks(4800) {
            tx.send(c.to_vec()).unwrap();
        }
        w.finish().unwrap();
        let spec = hound::WavReader::open(&path).unwrap().spec();
        assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (16_000, 1, 16));
        let samples = read_track(&path).unwrap();
        assert!((samples.len() as i64 - 16_000).abs() < 40, "{}", samples.len());
        assert!(samples.iter().any(|&s| s.abs() > 20_000));
        let _ = std::fs::remove_file(&path);
    }
}
