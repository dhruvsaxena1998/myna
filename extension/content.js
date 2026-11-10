// Runs on the Teams web pages: tells Myna (through the service worker) whether the mute button says muted.
const { readPage, readCaptions, newCaptions } = await import(chrome.runtime.getURL("selectors.js"));

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

chrome.runtime.onMessage.addListener((m) => m?.type === "announce" && (report(true), (told = false), scanCaptions()));
addEventListener("pagehide", () => (last = "unknown", chrome.runtime.sendMessage({ type: "mute", state: "unknown", at: Date.now() }).catch(() => {})));
report(true);
