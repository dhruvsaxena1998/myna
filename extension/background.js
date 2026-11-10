import { PORTS, REPLACED, WRONG_CODE, hello, mute, parse, ping, tab as tabMsg } from "./messages.js";
import { isTeams } from "./selectors.js";

// The service worker sleeps, so everything here can start over: the status lives in storage, and an alarm
// reconnects after a nap.
let ws = null;
let trying = false;
let delay = 1000;
let timer = null;

const setStatus = (status) => chrome.storage.session.set({ status });

async function connect() {
  if (trying || ws) return;
  const { code } = await chrome.storage.local.get("code");
  if (!code) return setStatus("idle");
  trying = true;
  tryPort(0, code);
}

function tryPort(i, code) {
  if (i >= PORTS.length) {
    trying = false;
    setStatus("offline");
    return retry();
  }
  const s = new WebSocket(`ws://127.0.0.1:${PORTS[i]}`);
  let welcome = false;
  s.onopen = () => s.send(JSON.stringify(hello(code)));
  s.onmessage = (e) => {
    const type = parse(e.data)?.type;
    if (type === "stop-tab") return stopTab();
    if (type !== "welcome") return;
    welcome = true;
    trying = false;
    ws = s;
    delay = 1000;
    setStatus("connected");
    announce();
  };
  s.onclose = (e) => {
    if (welcome) {
      ws = null;
      stopTab(false); // frames have nowhere to go; Myna has already put the process tap back
      if (e.code === WRONG_CODE) return setStatus("wrong-code");
      if (e.code === REPLACED) return setStatus("idle");
      setStatus("offline");
      return retry();
    }
    if (e.code === WRONG_CODE) {
      trying = false;
      return setStatus("wrong-code");
    }
    tryPort(i + 1, code); // not Myna here, or Myna isn't running: try the next port
  };
}

// Asks the Teams tabs for the mute button's state again: what they said while Myna wasn't listening was dropped.
async function announce() {
  for (const tab of await chrome.tabs.query({ url: chrome.runtime.getManifest().host_permissions })) {
    chrome.tabs.sendMessage(tab.id, { type: "announce" }).catch(() => {});
  }
}

// The Teams tab being captured, if any. The worker owns the one socket (a second would replace it), so the offscreen
// document hands its frames here, base64 because extension messages are JSON.
let tab = null;
const setTab = (id) => {
  tab = id;
  chrome.storage.session.set({ tab: id });
};

async function startTab(id, title, streamId) {
  if (ws?.readyState !== WebSocket.OPEN) return;
  if (tab !== null) await stopTab();
  setTab(id);
  if (!(await chrome.runtime.getContexts({ contextTypes: ["OFFSCREEN_DOCUMENT"] })).length) {
    await chrome.offscreen.createDocument({ url: "offscreen.html", reasons: ["USER_MEDIA"], justification: "Capture the Teams call tab's sound for Myna." });
  }
  ws.send(JSON.stringify(tabMsg("start", title)));
  chrome.runtime.sendMessage({ to: "offscreen", type: "start", streamId });
}

async function stopTab(tell = true) {
  if (tab === null) return;
  setTab(null);
  if (tell && ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(tabMsg("stop", "")));
  chrome.offscreen.closeDocument().catch(() => {}); // its stream ends with it
}

chrome.tabs.onRemoved.addListener((id) => id === tab && stopTab());
chrome.tabs.onUpdated.addListener((id, info) => id === tab && info.url && !isTeams(info.url) && stopTab());
// A worker that starts fresh has lost track of any capture, and so has Myna's socket.
setTab(null);
chrome.offscreen.closeDocument().catch(() => {});

// From our own pages: a Teams tab's content script (mute), the popup, the offscreen document.
chrome.runtime.onMessage.addListener((m, sender) => {
  if (sender.id !== chrome.runtime.id) return;
  if (m?.type === "mute") {
    if (["muted", "unmuted", "unknown"].includes(m.state) && Number.isFinite(m.at) && ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(mute(m.state, m.at)));
  } else if (m?.type === "record-tab") {
    if (Number.isInteger(m.tabId) && typeof m.streamId === "string") startTab(m.tabId, String(m.title ?? ""), m.streamId);
  } else if (m?.type === "stop-tab-click" || m?.type === "tab-ended") {
    stopTab();
  } else if (m?.type === "frame" && tab !== null && ws?.readyState === WebSocket.OPEN && typeof m.data === "string") {
    ws.send(Uint8Array.from(atob(m.data), (c) => c.charCodeAt(0)));
  }
});

function retry() {
  clearTimeout(timer);
  timer = setTimeout(connect, delay);
  delay = Math.min(delay * 2, 30000);
}

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(() => {
  chrome.alarms.create("myna", { periodInMinutes: 0.5 });
  connect();
});
// Wakes a sleeping worker; also a message every 30 s keeps an open socket from being cut as idle.
chrome.alarms.onAlarm.addListener(() => (ws ? ws.send(JSON.stringify(ping())) : connect()));
// A new code from the popup starts over.
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local" || !changes.code) return;
  ws?.close();
  ws = null;
  trying = false;
  delay = 1000;
  connect();
});
connect();
