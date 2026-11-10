// Runs on the Teams web pages: tells Myna (through the service worker) whether the mute button says muted.
// Content scripts run as classic scripts, where a top-level await is a syntax error: the whole script sits in an async function.
(async () => {
const { readPage, readCaptions, newCaptions, useRules, validate } = await import(chrome.runtime.getURL("selectors.js"));

// The worker hands over the table in force (a fresher one arrives as a message); one that doesn't compile here falls
// back to the bundled file. shortcut: Myna is still told the worker's version then; the worker has no DOM to check with.
const compiles = (s) => {
  try {
    document.createDocumentFragment().querySelector(s);
    return true;
  } catch {
    return false;
  }
};
const load = async (data) => useRules(validate(data, compiles) ?? validate(await (await fetch(chrome.runtime.getURL("selectors.json"))).json(), compiles));
await load(await chrome.runtime.sendMessage({ type: "selectors-get" }).catch(() => null));

let last;
function report(force) {
  const state = readPage(document);
  if (state === last && !force) return;
  last = state;
  chrome.runtime.sendMessage({ type: "mute", state, at: Date.now() }).catch(() => {}); // the worker may be asleep or Myna away: it asks again on connect
}

// Live captions: a line per new caption. In a call (the mute button is found) with no caption container for a while,
// captions are off or Teams changed: say so once rather than guess.
const seen = new Map();
let missing, told;
function scanCaptions() {
  const lines = readCaptions(document);
  if (lines) {
    clearTimeout(missing);
    missing = told = undefined;
    for (const c of newCaptions(lines, seen)) chrome.runtime.sendMessage({ type: "caption", name: c.name, at: Date.now(), text: c.text }).catch(() => {});
  } else if (last && last !== "unknown" && !missing && !told) {
    missing = setTimeout(() => {
      if (readCaptions(document)) return;
      told = true;
      chrome.runtime.sendMessage({ type: "captions", state: "unavailable" }).catch(() => {});
    }, 5000);
  }
}

let timer;
new MutationObserver(() => {
  clearTimeout(timer);
  timer = setTimeout(() => (report(), scanCaptions()), 250);
}).observe(document, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ["aria-pressed", "aria-label", "title", "data-state"] });

chrome.runtime.onMessage.addListener((m) => {
  if (m?.type === "announce") (report(true), (told = false), scanCaptions());
  else if (m?.type === "selectors") load(m.data).then(() => (report(true), (told = false), scanCaptions()));
});
addEventListener("pagehide", () => (last = "unknown", chrome.runtime.sendMessage({ type: "mute", state: "unknown", at: Date.now() }).catch(() => {})));
report(true);
})();
