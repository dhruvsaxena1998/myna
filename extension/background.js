// Relays what the content scripts read (the mute button, who the captions show speaking) to Myna on this Mac, over
// http://127.0.0.1 with the pairing token.
// Each message is {type, ...}; Myna rejects types it doesn't know. To relay something new, call post({ type: "...", ... }).
const MYNA = "http://127.0.0.1:47615/message";
const SELECTORS_URL = "https://raw.githubusercontent.com/dhruvsaxena1998/myna/main/extension/selectors.json";
const DAY = 24 * 60 * 60 * 1000;
const FRESH = 10 * 1000; // a frame silent for this long is no longer in the call

// The selector list: fetched from SELECTORS_URL once a day so a broken selector is fixed without an app release.
// The bundled selectors.json is the default and wins if it has a higher version than what was fetched.
const valid = (j) => {
  const ok = (r) => ["selector", "attr", "muted", "live"].every((k) => typeof r?.[k] === "string");
  const cap = (r) => typeof r?.selector === "string";
  const caps = j?.captions === undefined || (j.captions && Object.values(j.captions).every((l) => Array.isArray(l) && l.every(cap)));
  return typeof j?.version === "number" && j.sites && Object.values(j.sites).every((l) => Array.isArray(l) && l.every(ok)) && caps ? j : null;
};

async function selectors() {
  const bundled = valid(await (await fetch(chrome.runtime.getURL("selectors.json"))).json());
  let { cache } = await chrome.storage.local.get("cache");
  if (!cache || Date.now() - cache.at > DAY) {
    try {
      const fetched = valid(await (await fetch(SELECTORS_URL, { cache: "no-store" })).json());
      if (fetched) await chrome.storage.local.set({ cache: (cache = { at: Date.now(), list: fetched }) });
    } catch {} // offline or blocked: keep what we have
  }
  return cache?.list && cache.list.version > bundled.version ? cache.list : bundled;
}

async function post(message) {
  const { token } = await chrome.storage.local.get("token");
  let status = "unpaired";
  if (token) {
    try {
      const r = await fetch(MYNA, { method: "POST", headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" }, body: JSON.stringify(message) });
      status = r.ok ? "connected" : r.status === 401 ? "refused" : "error";
    } catch {
      status = "offline"; // Myna isn't running
    }
  }
  await chrome.storage.local.set({ status });
}

// Every frame in a call page reports; a frame with no mute button (or no captions) says null. One known value beats a
// null, so a second Meet tab that isn't in the call can't hide the one that is.
const frames = new Map();
const captionFrames = new Map();
function best(map, key, entry, field) {
  const now = Date.now();
  map.set(key, { ...entry, at: now });
  const live = [...map.values()].filter((f) => now - f.at < FRESH).sort((a, b) => b.at - a.at);
  return live.find((f) => f[field] !== null) || live[0];
}
function relayMute(key, m) {
  const f = best(frames, key, { site: m.site, muted: m.muted }, "muted");
  return post({ type: "mute", site: f.site, muted: f.muted });
}
function relayCaption(key, m) {
  const f = best(captionFrames, key, { site: m.site, name: m.name }, "name");
  return post({ type: "caption", site: f.site, name: f.name });
}

chrome.runtime.onMessage.addListener((m, sender, reply) => {
  if (m?.type === "rules" && (m.site === "meet" || m.site === "teams")) {
    selectors().then((s) => reply({ mute: s.sites[m.site] || [], captions: s.captions?.[m.site] || [] }));
    return true;
  }
  if (m?.type === "mute" && sender.tab && (m.site === "meet" || m.site === "teams") && (m.muted === null || typeof m.muted === "boolean")) {
    relayMute(`${sender.tab.id}/${sender.frameId}`, m);
  }
  const named = m?.name === null || (typeof m?.name === "string" && m.name.length <= 80);
  if (m?.type === "caption" && sender.tab && (m.site === "meet" || m.site === "teams") && named) {
    relayCaption(`${sender.tab.id}/${sender.frameId}`, m);
  }
});

chrome.tabs.onRemoved.addListener((id) => { for (const map of [frames, captionFrames]) for (const k of map.keys()) if (k.startsWith(id + "/")) map.delete(k); });
