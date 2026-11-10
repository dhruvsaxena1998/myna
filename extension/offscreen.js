// Holds the Teams tab's sound while the worker sleeps between messages (an offscreen document is the only place
// MV3 lets an extension open a tab's audio). Sends 16 kHz frames to the worker, which owns the one socket to Myna.
import { Framer, Resampler, downmix } from "./audio.js";

const b64 = (bytes) => btoa(String.fromCharCode(...bytes));

chrome.runtime.onMessage.addListener((m) => m?.to === "offscreen" && m.type === "start" && start(m.streamId));

async function start(streamId) {
  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: streamId } } });
    const ctx = new AudioContext();
    const src = ctx.createMediaStreamSource(stream);
    src.connect(ctx.destination); // capturing a tab silences it: this plays it again, so the user still hears the call
    const [rs, fr] = [new Resampler(ctx.sampleRate), new Framer()];
    // shortcut: ScriptProcessor runs on this page's main thread and is deprecated; nothing else runs here. Move to an AudioWorklet if it's ever removed.
    const tap = ctx.createScriptProcessor(4096, 2, 2); // its output stays silent; it must be connected to run
    tap.onaudioprocess = (e) => {
      const b = e.inputBuffer;
      const mono = downmix(Array.from({ length: b.numberOfChannels }, (_, i) => b.getChannelData(i)));
      for (const f of fr.push(rs.push(mono), Date.now())) chrome.runtime.sendMessage({ type: "frame", data: b64(f) }).catch(() => {});
    };
    src.connect(tap);
    tap.connect(ctx.destination);
    stream.getAudioTracks()[0].onended = () => chrome.runtime.sendMessage({ type: "tab-ended" }).catch(() => {});
  } catch {
    chrome.runtime.sendMessage({ type: "tab-ended" }).catch(() => {});
  }
}
