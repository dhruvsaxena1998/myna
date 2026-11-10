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
