// How to read a Teams or Meet page: the logic here, the selectors in selectors.json. The service worker fetches a newer
// selectors.json from the repo once a day (see background.js); every call tab is handed the one in force.
// Teams' mute button rules are checked against a real Teams web call (2026-11-10); its caption rules are still unverified.
// Meet's rules come from public extensions, not a real call yet: sources in tests/extension/meet.html.

const SCHEMA = 1; // bump with any change to the shape below that an older add-on couldn't read
const MAX_BYTES = 64 * 1024;
const MAX_SELECTOR = 300;
const SELECTORS = ["captions", "line", "author", "words"];

const text = (v, max = MAX_SELECTOR) => typeof v === "string" && v.length > 0 && v.length <= max;

/** One site's rules from `d`, or null. `mutedAttr`, if set, is an attribute that is "true" while muted. */
function rules(d, compiles) {
  const { buttons, label, labelFlags = "", pressed, mutedAttr } = d ?? {};
  if (!Array.isArray(buttons) || !buttons.length || buttons.length > 10 || !buttons.every((b) => text(b))) return null;
  if (!SELECTORS.every((k) => text(d[k])) || ![...buttons, ...SELECTORS.map((k) => d[k])].every(compiles)) return null;
  if (!text(label, 200) || !/^[ims]{0,3}$/.test(labelFlags) || ![undefined, "muted", "unmuted"].includes(pressed)) return null;
  if (mutedAttr !== undefined && !/^[\w-]{1,40}$/.test(mutedAttr)) return null;
  return { buttons, label: new RegExp(label, labelFlags), pressed, mutedAttr, ...Object.fromEntries(SELECTORS.map((k) => [k, d[k]])) };
}

/**
 * The selector table in `data` (parsed JSON) ready to use, or null if it isn't one this add-on understands. It is only
 * ever data: the label's regex is compiled from a short string with whitelisted flags, nothing is evaluated.
 * Teams' rules sit at the top level (older add-ons read only those), Meet's under `meet`.
 * `compiles(selector)` says whether a selector is valid CSS; the worker has no DOM, so it passes a syntax check.
 */
export function validate(data, compiles = () => true) {
  try {
    if (data?.schema !== SCHEMA) return null;
    const version = typeof data.version === "number" ? String(data.version) : data.version;
    if (!/^[\w.-]{1,32}$/.test(version)) return null;
    const [teams, meet] = [rules(data, compiles), rules(data.meet, compiles)];
    return teams && meet && { version, ...teams, meet };
  } catch {
    return null; // a bad regex, a selector that throws
  }
}

/** Remote file text to a validated table, or null when it's too big, isn't JSON or isn't a table we understand. */
export const parseRemote = (raw, compiles) => {
  if (typeof raw !== "string" || raw.length > MAX_BYTES) return null;
  try {
    return validate(JSON.parse(raw), compiles);
  } catch {
    return null;
  }
};

/** A cheap CSS syntax check for the worker: only characters selectors use, balanced brackets and quotes. */
export const looksLikeCss = (s) => /^[\w\s#.,>+~*:()[\]='"^$|-]+$/.test(s) && [...s].filter((c) => c === "[").length === [...s].filter((c) => c === "]").length && (s.match(/'/g) ?? []).length % 2 === 0 && (s.match(/"/g) ?? []).length % 2 === 0;

export const REMOTE = "https://raw.githubusercontent.com/dhruvsaxena1998/myna/main/extension/selectors.json";

/** The newest table from the repo as raw data, or null if it can't be had or isn't valid: the caller keeps what it has. */
export async function fetchRemote(get = fetch) {
  try {
    const r = await get(REMOTE, { cache: "no-cache", credentials: "omit" });
    if (!r.ok) return null;
    const raw = await r.text();
    return parseRemote(raw, looksLikeCss) && JSON.parse(raw);
  } catch {
    return null;
  }
}

/** The table to use: `cached` (raw data from storage) if it is still valid, else `bundled`. */
export const pick = (cached, bundled) => (validate(cached, looksLikeCss) ? cached : bundled);

// The table in force. Starts empty: the content script sets the bundled one before anything is read.
// shortcut: module state rather than an argument everywhere; fine while a page has one table.
let RULES = null;
export const useRules = (r) => (RULES = r);

/** The state from a button's attributes (plain strings, null when absent): "muted", "unmuted" or "unknown", never a guess. */
export function readMute({ label, dataState, muted, pressed }, rules = RULES) {
  // The words say what a click does: "Unmute" is shown while you are muted.
  const words = label ?? "";
  const [un, mute] = [/\bunmute\b/i.test(words), /\bmute\b/i.test(words)];
  if (un !== mute) return un ? "muted" : "unmuted"; // both or neither: the words don't say
  if (un && mute) return "unknown";
  if (dataState === "muted" || dataState === "unmuted") return dataState;
  if (muted === "true" || muted === "false") return muted === "true" ? "muted" : "unmuted";
  if (rules.pressed && (pressed === "true" || pressed === "false")) return (pressed === "true") === (rules.pressed === "muted") ? "muted" : "unmuted";
  return "unknown";
}

const attrs = (b, rules) => ({
  label: b.getAttribute("aria-label") ?? b.getAttribute("title"),
  dataState: b.getAttribute("data-state"),
  muted: rules.mutedAttr ? b.getAttribute(rules.mutedAttr) : null,
  pressed: b.getAttribute("aria-pressed"),
});

/** The mute state on this page (`root` is the document), or "unknown" when the button isn't found. */
export function readPage(root, rules = RULES) {
  for (const sel of rules.buttons) {
    const b = root.querySelector(sel);
    if (b) return readMute(attrs(b, rules), rules);
  }
  const named = [...root.querySelectorAll("button")].filter((b) => rules.label.test(`${b.getAttribute("aria-label") ?? ""} ${b.getAttribute("title") ?? ""}`));
  return named.length === 1 ? readMute(attrs(named[0], rules), rules) : "unknown";
}

const NAME_MAX = 80;
const TEXT_MAX = 300;
const clip = (s, n) => (s ?? "").trim().slice(0, n);

/**
 * The caption lines on this page as `{ name, text }`, or null when the caption container isn't there (captions off, or
 * the markup isn't what the rules expect): never a guess. Lines without a name are skipped.
 */
export function readCaptions(root, rules = RULES) {
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

const HOSTS = { "teams.microsoft.com": "teams", "teams.live.com": "teams", "teams.cloud.microsoft": "teams", "meet.google.com": "meet" }; // as in manifest.json

/** The call site `url` is on: "teams", "meet", or null for any other page. */
export function site(url) {
  try {
    const u = new URL(url);
    return (u.protocol === "https:" && Object.hasOwn(HOSTS, u.hostname) && HOSTS[u.hostname]) || null;
  } catch {
    return null;
  }
}
