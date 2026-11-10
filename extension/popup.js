import { isTeams } from "./selectors.js";

const WORDS = {
  connected: "Connected.",
  idle: "Not connected.",
  offline: "Myna isn’t running, or Meeting notes are off. Trying again.",
  "wrong-code": "Wrong code. Copy it again from Myna’s Settings.",
};
const show = (status = "idle") => (document.getElementById("status").textContent = WORDS[status]);

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
const teams = Boolean(page?.url && isTeams(page.url));
async function sync() {
  const { status, tab } = await chrome.storage.session.get(["status", "tab"]);
  const mine = tab != null && tab === page?.id;
  rec.textContent = mine ? "Stop recording this tab" : "Record this tab";
  rec.disabled = !teams || (!mine && status !== "connected");
  document.getElementById("hint").textContent = !teams ? "Open your Teams call tab to record its sound." : status !== "connected" && !mine ? "Connect to Myna first." : tab != null && !mine ? "Another tab is recording." : "";
}
await sync();
chrome.storage.session.onChanged.addListener(sync);
rec.addEventListener("click", async () => {
  const { tab } = await chrome.storage.session.get("tab");
  if (tab === page.id) return chrome.runtime.sendMessage({ type: "stop-tab-click" });
  const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: page.id });
  chrome.runtime.sendMessage({ type: "record-tab", tabId: page.id, title: page.title, streamId });
});
