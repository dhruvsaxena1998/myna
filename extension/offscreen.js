// Holds the Teams tab's sound while the worker sleeps between messages (an offscreen document is the only place
// MV3 lets an extension open a tab's audio). Sends 16 kHz frames to the worker, which owns the one socket to Myna.
import { Framer, Resampler } from "./audio.js";

const b64 = (bytes) => btoa(String.fromCharCode(...bytes));

chrome.runtime.onMessage.addListener((m) => m?.to === "offscreen" && m.type === "start" && start(m.streamId));

async function start(streamId) {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: streamId } } });
    const ctx = new AudioContext();
    const src = ctx.createMediaStreamSource(stream);
    src.connect(ctx.destination); // capturing a tab silences it: this plays it again, so the user still hears the call
    const [rs, fr] = [new Resampler(ctx.sampleRate), new Framer()];
    await ctx.audioWorklet.addModule("tap-worklet.js"); // mixes to mono on the audio thread
    const tap = new AudioWorkletNode(ctx, "myna-tap");
    tap.port.onmessage = ({ data: mono }) => {
      for (const f of fr.push(rs.push(mono), Date.now())) chrome.runtime.sendMessage({ type: "frame", data: b64(f) }).catch(() => {});
    };
    src.connect(tap);
    tap.connect(ctx.destination); // silent; keeps the node in the rendered graph
    stream.getAudioTracks()[0].onended = () => chrome.runtime.sendMessage({ type: "tab-ended" }).catch(() => {});
  } catch {
    chrome.runtime.sendMessage({ type: "tab-ended" }).catch(() => {});
  }
}
