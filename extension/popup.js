import { site } from "./selectors.js";

// Per status: the word beside the dot, and the line under the code.
const WORDS = {
  connected: ["Connected", "Myna hears when you’re muted in Teams or Meet, and who is talking."],
  idle: ["Not connected", "Paste the code from Myna’s Settings › Meeting notes › Browser add-on."],
  offline: ["Waiting for Myna", "Myna isn’t running, or Meeting notes are off. Trying again."],
  "wrong-code": ["Wrong code", "No Myna took this code. Copy it again from Myna’s Settings › Meeting notes."],
};
const show = (status = "idle") => {
  const [word, line] = WORDS[status] ?? WORDS.idle;
  const state = document.getElementById("state");
  state.textContent = word;
  state.dataset.s = status;
  const help = document.getElementById("status");
  help.textContent = line;
  help.classList.toggle("bad", status === "wrong-code");
  document.getElementById("code").setAttribute("aria-invalid", String(status === "wrong-code"));
  document.getElementById("go").classList.toggle("primary", status !== "connected"); // one primary per view: Connect until it's done
};

const { code } = await chrome.storage.local.get("code");
document.getElementById("code").value = code ?? "";
show((await chrome.storage.session.get("status")).status);
chrome.storage.session.onChanged.addListener((c) => c.status && show(c.status.newValue));

document.getElementById("f").addEventListener("submit", (e) => {
  e.preventDefault();
  chrome.storage.local.set({ code: document.getElementById("code").value.trim().toUpperCase() });
});

// "Record this tab": the click is the gesture tabCapture needs, so the stream id is asked for here and handed to the worker.
const rec = document.getElementById("rec");
const [page] = await chrome.tabs.query({ active: true, currentWindow: true });
const call = page?.url ? site(page.url) : null;
async function sync() {
  const { status, tab } = await chrome.storage.session.get(["status", "tab"]);
  const mine = tab != null && tab === page?.id;
  rec.textContent = mine ? "Stop" : "Record";
  rec.disabled = !call || (!mine && status !== "connected");
  const name = document.getElementById("rec-name");
  name.replaceChildren(...(mine ? [Object.assign(document.createElement("span"), { className: "rec-dot", ariaHidden: "true" }), "Recording this tab"] : ["Record this tab"]));
  document.getElementById("hint").textContent = !call ? "Open your Teams or Meet call tab to record its sound."
    : status !== "connected" && !mine ? "Connect to Myna first."
    : tab != null && !mine ? "Another tab is recording."
    : mine ? "Myna takes the call’s sound from this tab." : "Myna takes the call’s sound from this tab while you record.";
}
await sync();
chrome.storage.session.onChanged.addListener(sync);
rec.addEventListener("click", async () => {
  const { tab } = await chrome.storage.session.get("tab");
  if (tab === page.id) return chrome.runtime.sendMessage({ type: "stop-tab-click" });
  const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: page.id });
  chrome.runtime.sendMessage({ type: "record-tab", tabId: page.id, title: page.title, streamId, site: call });
});
