// Run with `pnpm test:extension`. The snippets are what the rules in extension/selectors.js expect of Teams' markup,
// and were not saved from a real call: replace them with the real ones when we have them.
import assert from "node:assert/strict";
import test from "node:test";
import { readMute, readPage } from "../../extension/selectors.js";

/** A page of buttons, each `{ id?, attrs }`; answers the two selector shapes the table uses. */
const page = (...buttons) => {
  const el = (b) => ({ getAttribute: (n) => (n === "id" ? b.id : b.attrs?.[n]) ?? null });
  const match = (sel, b) => (sel.startsWith("#") ? b.id === sel.slice(1) : sel === `button[data-tid='${b.attrs?.["data-tid"]}']`);
  return {
    querySelector: (sel) => buttons.filter((b) => match(sel, b)).map(el)[0] ?? null,
    querySelectorAll: () => buttons.map(el),
  };
};

test("the words on the button say what a click does", () => {
  assert.equal(readMute({ label: "Unmute mic" }), "muted");
  assert.equal(readMute({ label: "Mute mic (Ctrl+Shift+M)" }), "unmuted");
  assert.equal(readMute({ label: "Mute / Unmute" }), "unknown", "both words: it doesn't say");
});

test("without words, data-state then aria-pressed, in the polarity the table says", () => {
  assert.equal(readMute({ dataState: "muted" }), "muted");
  assert.equal(readMute({ pressed: "true" }, { pressed: "unmuted" }), "unmuted");
  assert.equal(readMute({ pressed: "true" }, { pressed: "muted" }), "muted");
  assert.equal(readMute({ pressed: "false" }, { pressed: "muted" }), "unmuted");
});

test("nothing readable is unknown, never a guess", () => {
  assert.equal(readMute({}), "unknown");
  assert.equal(readMute({ label: "Microphone", dataState: "weird", pressed: "mixed" }), "unknown");
});

test("the known button is read, wherever its label is", () => {
  assert.equal(readPage(page({ id: "microphone-button", attrs: { "aria-label": "Unmute mic" } })), "muted");
  assert.equal(readPage(page({ id: "microphone-button", attrs: { title: "Mute mic" } })), "unmuted");
  assert.equal(readPage(page({ attrs: { "data-tid": "toggle-mute", "aria-label": "Unmute" } })), "muted");
});

test("with no known button, one button named mute is used and two are not", () => {
  const other = { attrs: { "aria-label": "Share" } };
  assert.equal(readPage(page(other, { attrs: { "aria-label": "Unmute" } })), "muted");
  assert.equal(readPage(page({ attrs: { "aria-label": "Mute" } }, { attrs: { "aria-label": "Mute all" } })), "unknown");
  assert.equal(readPage(page(other)), "unknown", "not in a call");
});
