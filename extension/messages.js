// The messages between this add-on and Myna. Keep in step with src-tauri/src/addon.rs, which is the other copy.
// Every message is JSON with `v` (the version) and `type`. A type this side doesn't know is ignored, never fatal.
export const V = 1;
// Tried in order; Myna takes the first free one.
export const PORTS = [47821, 47822, 47823, 47824];
// Close codes from Myna.
export const WRONG_CODE = 4001;
export const REPLACED = 4002;

export const hello = (code) => ({ v: V, type: "hello", code });
// Defined now so both sides agree; later tickets send them.
export const mute = (state, at) => ({ v: V, type: "mute", state, at }); // state: "unmuted" | "muted" | "unknown"
export const caption = (speaker, at) => ({ v: V, type: "caption", speaker, at });
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
