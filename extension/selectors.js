// Where Teams' mute button is and how to read it: the one table to edit when Teams' markup changes (07.5 will
// fetch it instead of shipping it). UNVERIFIED: written from memory of new Teams (v2) on the web, not from the page.
export const TEAMS = {
  // Tried in order; the first that exists is the button.
  buttons: ["#microphone-button", "button[data-tid='toggle-mute']"],
  // No known button: the one button on the page whose label says mute. Two or more is a guess, so it's unknown.
  label: /\b(un)?mute\b/i,
  // What `aria-pressed="true"` means on the button. Only used when there's no label wording or data-state to go by.
  pressed: "unmuted",
};

/** The state from a button's attributes (plain strings, null when absent): "muted", "unmuted" or "unknown", never a guess. */
export function readMute({ label, dataState, pressed }, rules = TEAMS) {
  // The words say what a click does: "Unmute" is shown while you are muted.
  const words = label ?? "";
  const [un, mute] = [/\bunmute\b/i.test(words), /\bmute\b/i.test(words)];
  if (un !== mute) return un ? "muted" : "unmuted"; // both or neither: the words don't say
  if (un && mute) return "unknown";
  if (dataState === "muted" || dataState === "unmuted") return dataState;
  if (pressed === "true" || pressed === "false") return (pressed === "true") === (rules.pressed === "muted") ? "muted" : "unmuted";
  return "unknown";
}

const attrs = (b) => ({ label: b.getAttribute("aria-label") ?? b.getAttribute("title"), dataState: b.getAttribute("data-state"), pressed: b.getAttribute("aria-pressed") });

/** The mute state on this page (`root` is the document), or "unknown" when the button isn't found. */
export function readPage(root, rules = TEAMS) {
  for (const sel of rules.buttons) {
    const b = root.querySelector(sel);
    if (b) return readMute(attrs(b), rules);
  }
  const named = [...root.querySelectorAll("button")].filter((b) => rules.label.test(`${b.getAttribute("aria-label") ?? ""} ${b.getAttribute("title") ?? ""}`));
  return named.length === 1 ? readMute(attrs(named[0]), rules) : "unknown";
}
