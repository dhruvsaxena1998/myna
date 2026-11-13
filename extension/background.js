import { PORTS, REPLACED, STOP_TAB, WELCOME, WRONG_CODE, caption, captions, hello, mute, parse, ping, selectors as selectorsMsg, tab as tabMsg } from "./messages.js";
import { fetchRemote, isTeams, pick } from "./selectors.js";

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
    if (type === STOP_TAB) return stopTab();
    if (type !== WELCOME) return;
    welcome = true;
    trying = false;
    ws = s;
    delay = 1000;
    setStatus("connected");
    announce();
    publish();
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

// The selector table in force (raw data): the daily-fetched one in storage, else the bundled file.
const DAY = 24 * 60 * 60 * 1000;
async function current() {
  const { selectors } = await chrome.storage.local.get("selectors");
  return pick(selectors, await (await fetch(chrome.runtime.getURL("selectors.json"))).json());
}

// Tells Myna and every Teams tab which table is in force.
async function publish() {
  const data = await current();
  if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(selectorsMsg(String(data.version))));
  for (const tab of await chrome.tabs.query({ url: chrome.runtime.getManifest().host_permissions })) {
    chrome.tabs.sendMessage(tab.id, { type: "selectors", data }).catch(() => {});
  }
}

// At most once a day unless `force`; a failed or invalid fetch keeps whatever is stored.
async function refresh(force) {
  const { selectorsAt = 0 } = await chrome.storage.local.get("selectorsAt");
  if (!force && Date.now() - selectorsAt < DAY) return;
  const data = await fetchRemote();
  if (data) await chrome.storage.local.set({ selectors: data, selectorsAt: Date.now() });
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
chrome.runtime.onMessage.addListener((m, sender, reply) => {
  if (sender.id !== chrome.runtime.id) return;
  if (m?.type === "selectors-get") {
    current().then(reply, () => reply(null));
    return true;
  }
  if (m?.type === "mute") {
    if (["muted", "unmuted", "unknown"].includes(m.state) && Number.isFinite(m.at) && ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(mute(m.state, m.at)));
  } else if (m?.type === "caption") {
    if (typeof m.name === "string" && m.name.trim() && Number.isFinite(m.at) && ws?.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(caption(m.name.trim().slice(0, 80), m.at, typeof m.text === "string" ? m.text.trim().slice(0, 300) : undefined)));
    }
  } else if (m?.type === "captions") {
    if (m.state === "unavailable" && ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify(captions("unavailable")));
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

chrome.runtime.onStartup.addListener(() => (connect(), refresh()));
chrome.runtime.onInstalled.addListener(async () => {
  chrome.alarms.create("myna", { periodInMinutes: 0.5 });
  connect();
  await chrome.storage.local.remove("selectors"); // the bundled file may be newer than what an older version cached
  refresh(true);
});
chrome.alarms.get("selectors").then((a) => a ?? chrome.alarms.create("selectors", { periodInMinutes: 24 * 60 }));
// Wakes a sleeping worker; also a message every 30 s keeps an open socket from being cut as idle.
chrome.alarms.onAlarm.addListener((a) => (a.name === "selectors" ? refresh() : ws ? ws.send(JSON.stringify(ping())) : connect()));
// A new code from the popup starts over.
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  if (changes.selectors) publish();
  if (!changes.code) return;
  ws?.close();
  ws = null;
  trying = false;
  delay = 1000;
  connect();
});
connect();
