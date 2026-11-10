// The Teams tab's sound as Myna wants it: 16 kHz mono Int16 in binary frames. Keep in step with src-tauri/src/addon.rs.
// A frame is 12 bytes of header, then the samples, all little-endian: seq (u32), capture time (i64, unix ms), Int16 PCM.
export const RATE = 16000;
export const FRAME = 1600; // samples: 100 ms, 3.2 KB, far under Myna's 64 KB cap

/** One block of any number of channels as mono: their average. */
export const downmix = (channels) => channels.length === 1 ? channels[0] : channels[0].map((_, i) => channels.reduce((s, c) => s + c[i], 0) / channels.length);

/** Linear-interpolation resampler to 16 kHz that carries its place from block to block. */
// shortcut: no low-pass before dropping samples, so anything above 8 kHz folds back in; harmless for speech, upgrade if it isn't.
export class Resampler {
  constructor(rate) {
    this.step = rate / RATE;
    this.pos = 1; // in the block laid after `prev`: index 0 is prev, 1 is the block's first sample
    this.prev = 0;
  }

  push(x) {
    const out = [];
    const at = (i) => (i === 0 ? this.prev : x[i - 1]);
    for (; this.pos < x.length; this.pos += this.step) {
      const i = Math.floor(this.pos);
      out.push(at(i) + (at(i + 1) - at(i)) * (this.pos - i));
    }
    this.pos -= x.length;
    if (x.length) this.prev = x[x.length - 1];
    return out;
  }
}

/** Cuts samples into frames of `FRAME`, each stamped with when its first sample was captured. */
export class Framer {
  constructor() {
    this.pending = [];
    this.seq = 0;
  }

  /** `samples` (floats, -1 to 1) have just been captured, at `now` (unix ms); returns the frames now complete, as bytes. */
  push(samples, now) {
    this.pending.push(...samples);
    // shortcut: the leftover is assumed to run straight into this block, so a stall in capture shows only in later frames.
    let ts = now - this.pending.length / (RATE / 1000);
    const out = [];
    while (this.pending.length >= FRAME) {
      out.push(frame(this.seq++, Math.round(ts), this.pending.splice(0, FRAME)));
      ts += FRAME / (RATE / 1000);
    }
    return out;
  }
}

export function frame(seq, ts, samples) {
  const b = new Uint8Array(12 + samples.length * 2);
  const v = new DataView(b.buffer);
  v.setUint32(0, seq, true);
  v.setBigInt64(4, BigInt(ts), true);
  samples.forEach((s, i) => v.setInt16(12 + i * 2, Math.round(Math.max(-1, Math.min(1, s)) * 32767), true));
  return b;
}
