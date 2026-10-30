// Reads the call's mute button and who the captions show speaking, and reports both, every 2 s, to the background
// script. null means "can't tell": no button or captions on this page, or a value the selector list doesn't
// recognise. It never guesses.
const site = location.hostname === "meet.google.com" ? "meet" : "teams";
let rules = [];
let captionRules = [];

const loadRules = () => chrome.runtime.sendMessage({ type: "rules", site }).then((r) => {
  if (Array.isArray(r?.mute)) rules = r.mute;
  if (Array.isArray(r?.captions)) captionRules = r.captions;
}).catch(() => {});

function read() {
  for (const { selector, attr, muted, live } of rules) {
    let value;
    try { value = document.querySelector(selector)?.getAttribute(attr); } catch { continue; } // a bad selector is skipped
    if (value === muted) return true;
    if (value === live) return false;
  }
  return null;
}

// The newest caption is the last one on the page: its speaker's name is who was talking in the last few seconds.
function speaker() {
  for (const { selector } of captionRules) {
    try {
      const all = document.querySelectorAll(selector);
      const name = all[all.length - 1]?.textContent.trim();
      if (name && name.length <= 80) return name;
    } catch {} // a bad selector is skipped
  }
  return null;
}

loadRules();
setInterval(loadRules, 10 * 60 * 1000);
const beat = setInterval(() => {
  // After the extension is reloaded this page's copy is orphaned; stop rather than throw forever.
  if (!chrome.runtime?.id) return clearInterval(beat);
  chrome.runtime.sendMessage({ type: "mute", site, muted: read() }).catch(() => {});
  chrome.runtime.sendMessage({ type: "caption", site, name: speaker() }).catch(() => {});
}, 2000);
