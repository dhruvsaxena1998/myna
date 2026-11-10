// Runs on the Teams web pages: tells Myna (through the service worker) whether the mute button says muted.
const { readPage } = await import(chrome.runtime.getURL("selectors.js"));

let last;
function report(force) {
  const state = readPage(document);
  if (state === last && !force) return;
  last = state;
  chrome.runtime.sendMessage({ type: "mute", state, at: Date.now() }).catch(() => {}); // the worker may be asleep or Myna away: it asks again on connect
}

let timer;
new MutationObserver(() => {
  clearTimeout(timer);
  timer = setTimeout(report, 250);
}).observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-pressed", "aria-label", "title", "data-state"] });

chrome.runtime.onMessage.addListener((m) => m?.type === "announce" && report(true));
addEventListener("pagehide", () => (last = "unknown", chrome.runtime.sendMessage({ type: "mute", state: "unknown", at: Date.now() }).catch(() => {})));
report(true);
