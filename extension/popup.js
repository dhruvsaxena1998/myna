const TEXT = {
  connected: "Connected to Myna",
  unpaired: "Not paired yet",
  refused: "Myna refused the token. Paste the current one.",
  offline: "Myna isn't running",
  saved: "Saved. Open a Meet or Teams call to connect.",
  error: "Myna didn't accept the message",
};
const state = document.getElementById("state");
const input = document.getElementById("token");
const show = ({ status, token }) => {
  state.textContent = TEXT[token ? status || "saved" : "unpaired"];
  if (document.activeElement !== input) input.value = token || "";
};
chrome.storage.local.get(["status", "token"]).then(show);
chrome.storage.onChanged.addListener(() => chrome.storage.local.get(["status", "token"]).then(show));
document.getElementById("save").onclick = () => chrome.storage.local.set({ token: input.value.trim(), status: "saved" });
