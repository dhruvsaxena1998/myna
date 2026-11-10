// The tab's AudioWorklet, run in a fake audio global scope: mono mix and 4096-sample batches.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

test("the tap worklet mixes to mono and posts 4096-sample batches", () => {
  let Proc;
  const posted = [];
  class AudioWorkletProcessor { port = { postMessage: (b) => posted.push(b) }; }
  vm.runInNewContext(readFileSync(new URL("../../extension/tap-worklet.js", import.meta.url), "utf8"), {
    AudioWorkletProcessor, Float32Array, registerProcessor: (name, cls) => { assert.equal(name, "myna-tap"); Proc = cls; },
  });
  const p = new Proc();
  const block = (l, r) => [[new Float32Array(128).fill(l), new Float32Array(128).fill(r)]];
  for (let i = 0; i < 32; i++) assert.equal(p.process(block(1, 0)), true); // 32 × 128 = 4096
  assert.equal(posted.length, 1);
  assert.equal(posted[0].length, 4096);
  assert.ok(posted[0].every((s) => s === 0.5));
  p.process([[]]); // no input yet: nothing posted, keeps running
  assert.equal(posted.length, 1);
});
