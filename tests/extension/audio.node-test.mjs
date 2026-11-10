// Run with `pnpm test:extension`.
import assert from "node:assert/strict";
import test from "node:test";
import { FRAME, Framer, RATE, Resampler, downmix, frame } from "../../extension/audio.js";

const tone = (rate, secs, hz = 440) => Float32Array.from({ length: rate * secs }, (_, i) => 0.5 * Math.sin((2 * Math.PI * hz * i) / rate));

test("channels are averaged into one", () => {
  assert.deepEqual([...downmix([Float32Array.of(1, 0), Float32Array.of(0, 0)])], [0.5, 0]);
  const one = Float32Array.of(0.25);
  assert.equal(downmix([one]), one);
});

test("48 kHz becomes 16 kHz, the same in blocks of any size as in one go", () => {
  const x = tone(48000, 1);
  const whole = new Resampler(48000).push(x);
  assert.ok(Math.abs(whole.length - RATE) <= 1);
  const r = new Resampler(48000);
  const parts = [];
  for (let i = 0; i < x.length; i += 4096) parts.push(...r.push(x.subarray(i, i + 4096)));
  assert.equal(parts.length, whole.length);
  parts.forEach((v, i) => assert.ok(Math.abs(v - whole[i]) < 1e-6));
  // Every output sample is the input's at that moment: the tone is still a 440 Hz tone, at 16 kHz.
  const want = tone(RATE, 1);
  whole.forEach((v, i) => i > 0 && assert.ok(Math.abs(v - want[i]) < 0.03, `${i}`));
});

test("44.1 kHz works too, and a block shorter than a step carries over", () => {
  const r = new Resampler(44100);
  const n = [1, 1, 1, 4410, 4410].reduce((s, len) => s + r.push(new Float32Array(len)).length, 0);
  assert.ok(Math.abs(n - (4410 * 2 + 3) / 2.75625) < 2);
});

test("frames hold 100 ms each, numbered, stamped with when their first sample was captured, and carry the rest over", () => {
  const f = new Framer();
  assert.equal(f.push(new Array(1000).fill(0), 10_000).length, 0);
  const out = f.push(new Array(2400).fill(0), 10_150); // 3400 samples pending, the first captured 212.5 ms before now
  assert.equal(out.length, 2);
  const v = out.map((b) => new DataView(b.buffer));
  assert.deepEqual(v.map((d) => d.getUint32(0, true)), [0, 1]);
  assert.deepEqual(v.map((d) => Number(d.getBigInt64(4, true))), [9938, 10038]);
  assert.equal(out[0].length, 12 + FRAME * 2);
  assert.equal(f.push(new Array(1600 - 200).fill(0), 10_300).length, 1, "the 200 left over plus these make one more");
});

test("samples are little-endian Int16, clipped", () => {
  const b = frame(7, 1_700_000_000_123, [0.5, -1, 2, 0]);
  const d = new DataView(b.buffer);
  assert.equal(d.getUint32(0, true), 7);
  assert.equal(Number(d.getBigInt64(4, true)), 1_700_000_000_123);
  assert.deepEqual([12, 14, 16, 18].map((o) => d.getInt16(o, true)), [16384, -32767, 32767, 0]);
});
