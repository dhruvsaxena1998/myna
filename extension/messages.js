// The messages between this add-on and Myna. The wire is pinned by tests/extension/protocol.json, which this file and src-tauri/src/addon.rs are both tested against.
// Every message is JSON with `v` (the version) and `type`. A type this side doesn't know is ignored, never fatal.
export const V = 1;
// Tried in order; Myna takes the first free one.
export const PORTS = [47821, 47822, 47823, 47824];
// Close codes from Myna.
export const WRONG_CODE = 4001;
export const REPLACED = 4002;
// The types Myna sends, which the service worker reacts to.
export const WELCOME = "welcome";
export const STOP_TAB = "stop-tab";

export const hello = (code) => ({ v: V, type: "hello", code });
// `mute`, `caption` and `captions` come from a call tab's content script (through the worker). `site` ("teams" | "meet")
// says whose page; an add-on before Meet sent none, which Myna reads as Teams.
export const mute = (state, at, site) => ({ v: V, type: "mute", state, at, ...(site ? { site } : {}) }); // state: "unmuted" | "muted" | "unknown"
export const caption = (name, at, text) => ({ v: V, type: "caption", name, at, ...(text ? { text } : {}) });
export const captions = (state) => ({ v: V, type: "captions", state }); // state: "unavailable"
// `tab` brackets the binary audio frames (see audio.js); Myna answers a meeting's end with `stop-tab`.
export const tab = (state, title, site) => ({ v: V, type: "tab", state, title, ...(site ? { site } : {}) }); // state: "start" | "stop"
export const selectors = (version) => ({ v: V, type: "selectors", version }); // which selector table the add-on reads call pages with
export const ping = () => ({ v: V, type: "ping" });

/** A message from Myna, or null if it isn't one this version understands. */
export function parse(text) {
  try {
    const m = JSON.parse(text);
    return m && m.v === V && typeof m.type === "string" ? m : null;
  } catch {
    return null;
  }
}
