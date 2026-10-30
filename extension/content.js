// Reads the call's mute button and reports it, every 2 s, to the background script. null means "can't tell":
// no button on this page, or a value the selector list doesn't recognise. It never guesses.
const site = location.hostname === "meet.google.com" ? "meet" : "teams";
let rules = [];

const loadRules = () => chrome.runtime.sendMessage({ type: "rules", site }).then((r) => { if (Array.isArray(r)) rules = r; }).catch(() => {});

function read() {
  for (const { selector, attr, muted, live } of rules) {
    let value;
    try { value = document.querySelector(selector)?.getAttribute(attr); } catch { continue; } // a bad selector is skipped
    if (value === muted) return true;
    if (value === live) return false;
  }
  return null;
}

loadRules();
setInterval(loadRules, 10 * 60 * 1000);
const beat = setInterval(() => {
  // After the extension is reloaded this page's copy is orphaned; stop rather than throw forever.
  if (!chrome.runtime?.id) return clearInterval(beat);
  chrome.runtime.sendMessage({ type: "mute", site, muted: read() }).catch(() => {});
}, 2000);
