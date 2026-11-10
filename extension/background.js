import { PORTS, REPLACED, WRONG_CODE, hello, parse, ping } from "./messages.js";

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
    if (parse(e.data)?.type !== "welcome") return;
    welcome = true;
    trying = false;
    ws = s;
    delay = 1000;
    setStatus("connected");
  };
  s.onclose = (e) => {
    if (welcome) {
      ws = null;
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
