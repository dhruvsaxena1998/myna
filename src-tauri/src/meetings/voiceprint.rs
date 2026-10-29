//! A voice print: the mean and spread of 12 mel cepstra over a speaker's loud frames. It is what a remembered
//! voice is stored as and what a later meeting's speakers are compared with. Neither Who nor Sortformer hands
//! out speaker embeddings, so this is Myna's own, computed from the audio on this Mac and never sent anywhere.
// shortcut: spectral statistics separate voices on one channel much less sharply than a neural speaker
// embedding, so a match is only ever a "suggested" name the user confirms; swap in a speaker-embedding model
// (WeSpeaker ResNet through ort) here if suggestions are wrong too often.
use std::f64::consts::PI;

pub const DIM: usize = 12;
const FRAME: usize = 400; // 25 ms at 16 kHz
const HOP: usize = 160;
const FFT: usize = 512;
const MELS: usize = 20;
/// Frames louder than this far (dB) below the loud ones are pauses and noise, not the voice.
const FLOOR_DB: f64 = 30.0;
/// And nothing quieter than this (dB of a frame's summed squares, full scale 1.0) is a voice, whoever else is quiet.
const SILENCE_DB: f64 = -45.0;
/// Three seconds of voiced frames: less than that says too little about a voice.
const MIN_FRAMES: usize = 300;

#[derive(Clone, Debug, PartialEq)]
pub struct Print {
    pub frames: u32,
    pub mean: [f32; DIM],
    pub var: [f32; DIM],
}

fn fft(re: &mut [f64; FFT], im: &mut [f64; FFT], twiddle: &[(f64, f64); FFT / 2]) {
    let mut j = 0;
    for i in 1..FFT {
        let mut bit = FFT >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= FFT {
        for start in (0..FFT).step_by(len) {
            for k in 0..len / 2 {
                let (wr, wi) = twiddle[k * (FFT / len)];
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * wr - im[b] * wi, re[b] * wi + im[b] * wr);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                (re[a], im[a]) = (re[a] + tr, im[a] + ti);
            }
        }
        len *= 2;
    }
}

/// Triangular mel filters over 80 Hz to 7 kHz, one row per filter over the FFT's positive bins.
fn filterbank() -> Vec<Vec<f64>> {
    let mel = |hz: f64| 2595.0 * (1.0 + hz / 700.0).log10();
    let hz = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
    let (lo, hi) = (mel(80.0), mel(7000.0));
    let edge: Vec<f64> = (0..MELS + 2).map(|i| hz(lo + (hi - lo) * i as f64 / (MELS + 1) as f64) * FFT as f64 / 16_000.0).collect();
    (0..MELS)
        .map(|m| {
            (0..=FFT / 2)
                .map(|bin| {
                    let b = bin as f64;
                    ((b - edge[m]) / (edge[m + 1] - edge[m])).min((edge[m + 2] - b) / (edge[m + 2] - edge[m + 1])).max(0.0)
                })
                .collect()
        })
        .collect()
}

/// The print of `samples` (mono 16 kHz), or None when there is too little speech in them to say anything.
pub fn compute(samples: &[f32]) -> Option<Print> {
    let twiddle: [(f64, f64); FFT / 2] = std::array::from_fn(|k| {
        let a = -2.0 * PI * k as f64 / FFT as f64;
        (a.cos(), a.sin())
    });
    let window: Vec<f64> = (0..FRAME).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / (FRAME - 1) as f64).cos()).collect();
    let bank = filterbank();
    let dct: Vec<[f64; MELS]> = (1..=DIM).map(|k| std::array::from_fn(|m| (PI * k as f64 * (m as f64 + 0.5) / MELS as f64).cos())).collect();

    let mut frames: Vec<(f64, [f64; DIM])> = vec![];
    for frame in samples.windows(FRAME).step_by(HOP) {
        let (mut re, mut im) = ([0.0; FFT], [0.0; FFT]);
        let mut energy = 1e-10;
        for (i, w) in window.iter().enumerate() {
            let x = f64::from(frame[i]) - if i > 0 { 0.97 * f64::from(frame[i - 1]) } else { 0.0 };
            re[i] = x * w;
            energy += x * x;
        }
        fft(&mut re, &mut im, &twiddle);
        let log_mel: Vec<f64> = bank
            .iter()
            .map(|f| f.iter().enumerate().map(|(b, w)| w * (re[b] * re[b] + im[b] * im[b])).sum::<f64>().max(1e-10).ln())
            .collect();
        frames.push((10.0 * energy.log10(), std::array::from_fn(|k| dct[k].iter().zip(&log_mel).map(|(c, l)| c * l).sum())));
    }
    if frames.is_empty() {
        return None;
    }
    let mut loud: Vec<f64> = frames.iter().map(|f| f.0).collect();
    loud.sort_by(f64::total_cmp);
    let gate = (loud[loud.len() * 95 / 100] - FLOOR_DB).max(SILENCE_DB);
    let voiced: Vec<&[f64; DIM]> = frames.iter().filter(|f| f.0 > gate).map(|f| &f.1).collect();
    if voiced.len() < MIN_FRAMES {
        return None;
    }
    let n = voiced.len() as f64;
    let mean: [f64; DIM] = std::array::from_fn(|k| voiced.iter().map(|f| f[k]).sum::<f64>() / n);
    let var: [f64; DIM] = std::array::from_fn(|k| voiced.iter().map(|f| (f[k] - mean[k]).powi(2)).sum::<f64>() / n);
    Some(Print { frames: voiced.len() as u32, mean: mean.map(|x| x as f32), var: var.map(|x| x as f32) })
}

/// The print of both speakers' frames pooled, as if computed over the audio of both.
pub fn blend(a: &Print, b: &Print) -> Print {
    let (na, nb) = (f64::from(a.frames), f64::from(b.frames));
    let n = na + nb;
    let pool = |x: &[f32; DIM], y: &[f32; DIM]| -> [f64; DIM] { std::array::from_fn(|k| (na * f64::from(x[k]) + nb * f64::from(y[k])) / n) };
    let mean = pool(&a.mean, &b.mean);
    let second = |k: usize| (na * (f64::from(a.var[k]) + f64::from(a.mean[k]).powi(2)) + nb * (f64::from(b.var[k]) + f64::from(b.mean[k]).powi(2))) / n;
    Print { frames: a.frames.saturating_add(b.frames), mean: mean.map(|x| x as f32), var: std::array::from_fn(|k| (second(k) - mean[k] * mean[k]).max(0.0) as f32) }
}

/// How far apart two voices sound: the distance between their (mean, spread) vectors. 0 is the same voice.
pub fn distance(a: &Print, b: &Print) -> f32 {
    let sq = |x: f32, y: f32| (x - y) * (x - y);
    let mean: f32 = (0..DIM).map(|k| sq(a.mean[k], b.mean[k])).sum();
    let spread: f32 = (0..DIM).map(|k| sq(a.var[k].sqrt(), b.var[k].sqrt())).sum();
    (mean + spread).sqrt()
}

pub fn to_bytes(p: &Print) -> Vec<u8> {
    std::iter::once(p.frames.to_le_bytes()).chain(p.mean.iter().chain(&p.var).map(|x| x.to_le_bytes())).flatten().collect()
}

pub fn from_bytes(b: &[u8]) -> Option<Print> {
    if b.len() != 4 * (1 + 2 * DIM) {
        return None;
    }
    let f = |i: usize| f32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap());
    Some(Print { frames: u32::from_le_bytes(b[..4].try_into().unwrap()), mean: std::array::from_fn(|k| f(1 + k)), var: std::array::from_fn(|k| f(1 + DIM + k)) })
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A made-up voice: a pulse train at `f0` through two formant resonators that drift the way vowels do.
    pub fn voice(f0: f64, formants: [f64; 2], seed: u64, secs: usize) -> Vec<f32> {
        let mut rng = seed;
        let mut next = move || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng >> 33) as f64 / (1u64 << 31) as f64
        };
        let n = secs * 16_000;
        let (mut y, mut out) = ([[0.0f64; 2]; 2], Vec::with_capacity(n));
        let mut drift = [0.0, 0.0];
        for i in 0..n {
            if i % 4000 == 0 {
                drift = [next() * 0.35 - 0.175, next() * 0.35 - 0.175];
            }
            let pulse = if i as f64 % (16_000.0 / f0) < 1.0 { 1.0 } else { 0.0 };
            let mut x = pulse;
            for (r, y) in y.iter_mut().enumerate() {
                let (freq, bw) = (formants[r] * (1.0 + drift[r]), 90.0);
                let (rad, th) = ((-PI * bw / 16_000.0).exp(), 2.0 * PI * freq / 16_000.0);
                let v = x + 2.0 * rad * th.cos() * y[0] - rad * rad * y[1];
                y[1] = y[0];
                y[0] = v;
                x = v;
            }
            out.push(x as f32);
        }
        let peak = out.iter().fold(0f32, |m, x| m.max(x.abs()));
        out.iter().map(|x| x * 0.5 / peak).collect()
    }

    #[test]
    fn the_same_voice_sounds_nearer_than_a_different_one() {
        let a1 = compute(&voice(110.0, [700.0, 1200.0], 1, 8)).unwrap();
        let a2 = compute(&voice(110.0, [700.0, 1200.0], 2, 8)).unwrap();
        let b = compute(&voice(210.0, [450.0, 2300.0], 3, 8)).unwrap();
        let (same, other) = (distance(&a1, &a2), distance(&a1, &b));
        assert!(same * 2.0 < other, "same {same}, other {other}");
    }

    #[test]
    fn too_little_speech_or_silence_has_no_print() {
        assert_eq!(compute(&voice(110.0, [700.0, 1200.0], 1, 2)), None);
        assert_eq!(compute(&vec![0.0; 16_000 * 10]), None);
        assert_eq!(compute(&[]), None);
    }

    #[test]
    fn pauses_do_not_change_a_print() {
        let talk = voice(110.0, [700.0, 1200.0], 1, 6);
        let mut padded = talk.clone();
        padded.extend(vec![0.0; 16_000 * 4]);
        padded.extend(&talk);
        let (a, b) = (compute(&talk).unwrap(), compute(&padded).unwrap());
        assert!(distance(&a, &b) < 0.05 * distance(&a, &compute(&voice(210.0, [450.0, 2300.0], 3, 6)).unwrap()), "{a:?} {b:?}");
    }

    #[test]
    fn blending_pools_the_frames_of_both() {
        let mut a = Print { frames: 100, mean: [0.0; DIM], var: [1.0; DIM] };
        let mut b = Print { frames: 300, mean: [4.0; DIM], var: [1.0; DIM] };
        let m = blend(&a, &b);
        // Mean (100*0 + 300*4) / 400 = 3; variance 1 + the spread of the means: (100*9 + 300*1) / 400 = 3.
        assert_eq!((m.frames, m.mean[0], m.var[0]), (400, 3.0, 4.0));
        assert_eq!(blend(&a, &b), blend(&b, &a));
        a.frames = 0;
        b.mean = [7.0; DIM];
        assert_eq!(blend(&a, &b).mean[0], 7.0, "an empty print adds nothing");
    }

    #[test]
    fn prints_survive_storage_and_bad_blobs_are_refused() {
        let p = compute(&voice(110.0, [700.0, 1200.0], 1, 6)).unwrap();
        assert_eq!(from_bytes(&to_bytes(&p)), Some(p));
        for bad in [&[][..], &[0u8; 99], &[0u8; 101]] {
            assert_eq!(from_bytes(bad), None);
        }
    }
}
