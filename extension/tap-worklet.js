// Runs on the audio thread: mixes the tab's sound to mono and hands it to offscreen.js in ~4096-sample batches,
// so the page isn't messaged 375 times a second. Its own output stays silent.
const BATCH = 4096;

registerProcessor("myna-tap", class extends AudioWorkletProcessor {
  buf = new Float32Array(BATCH);
  n = 0;
  process([input]) {
    const chans = input ?? [];
    const len = chans[0]?.length ?? 0;
    for (let i = 0; i < len; i++) {
      let s = 0;
      for (const c of chans) s += c[i];
      this.buf[this.n++] = s / chans.length;
      if (this.n === BATCH) {
        this.port.postMessage(this.buf, [this.buf.buffer]);
        this.buf = new Float32Array(BATCH);
        this.n = 0;
      }
    }
    return true;
  }
});
