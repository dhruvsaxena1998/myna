// Where Teams' mute button is and how to read it: the one table to edit when Teams' markup changes (07.5 will
// fetch it instead of shipping it). UNVERIFIED: written from memory of new Teams (v2) on the web, not from the page.
export const TEAMS = {
  // Tried in order; the first that exists is the button.
  buttons: ["#microphone-button", "button[data-tid='toggle-mute']"],
  // No known button: the one button on the page whose label says mute. Two or more is a guess, so it's unknown.
  label: /\b(un)?mute\b/i,
  // What `aria-pressed="true"` means on the button. Only used when there's no label wording or data-state to go by.
  pressed: "unmuted",
  // Live captions. UNVERIFIED guesses, like the rest: the container is where Teams draws captions; each item in it is
  // one caption line, with the speaker's name and the words inside. Swap these three lines when real markup is known.
  captions: "[data-tid='closed-captions-renderer']",
  line: "[data-tid='closed-caption-message'], .ui-chat__item",
  author: "[data-tid='author'], .ui-chat__message__author",
  words: "[data-tid='closed-caption-text'], .ui-chat__message__content",
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

const NAME_MAX = 80;
const TEXT_MAX = 300;
const clip = (s, n) => (s ?? "").trim().slice(0, n);

/**
 * The caption lines on this page as `{ name, text }`, or null when the caption container isn't there (captions off, or
 * the markup isn't what the rules expect): never a guess. Lines without a name are skipped.
 */
export function readCaptions(root, rules = TEAMS) {
  const box = root.querySelector(rules.captions);
  if (!box) return null;
  return [...box.querySelectorAll(rules.line)]
    .map((l) => ({ name: clip(l.querySelector(rules.author)?.textContent, NAME_MAX), text: clip(l.querySelector(rules.words)?.textContent, TEXT_MAX) }))
    .filter((c) => c.name);
}

/** The lines in `lines` not seen before; `seen` is a Map from each name to the lines it has said. A caption grows as it is spoken, so a line that extends or trims one already seen is that line. */
// shortcut: a speaker repeating a line's opening words is counted as the same line, and 20 lines per name are remembered; key on the line's element if that matters.
export function newCaptions(lines, seen) {
  return lines.filter((c) => {
    const said = seen.get(c.name) ?? [];
    const i = said.findIndex((t) => t.startsWith(c.text) || c.text.startsWith(t));
    if (i >= 0) said[i] = c.text.length > said[i].length ? c.text : said[i];
    else said.push(c.text);
    seen.set(c.name, said.slice(-20));
    return i < 0;
  });
}

const HOSTS = ["teams.microsoft.com", "teams.live.com", "teams.cloud.microsoft"]; // as in manifest.json

/** Whether `url` is a Teams web page. */
export function isTeams(url) {
  try {
    const u = new URL(url);
    return u.protocol === "https:" && HOSTS.includes(u.hostname);
  } catch {
    return false;
  }
}
