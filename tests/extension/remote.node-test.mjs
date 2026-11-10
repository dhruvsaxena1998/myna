// Run with `pnpm test:extension`. The remote selector table is data from the internet: only a well-formed one is used.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { REMOTE, fetchRemote, looksLikeCss, parseRemote, pick, readCaptions, readPage, useRules, validate } from "../../extension/selectors.js";

const bundled = JSON.parse(readFileSync(new URL("../../extension/selectors.json", import.meta.url), "utf8"));
const with_ = (patch) => JSON.stringify({ ...bundled, ...patch });
const reply = (body, ok = true) => async (url) => (assert.equal(url, REMOTE), { ok, text: async () => body });

test("the bundled file is valid", () => {
  const r = validate(bundled);
  assert.equal(r.version, bundled.version);
  assert.ok(r.label.test("Unmute"));
  assert.ok(parseRemote(JSON.stringify(bundled), looksLikeCss));
});

test("a good remote file is taken, and a numeric version is allowed", async () => {
  const got = await fetchRemote(reply(with_({ version: 7 })));
  assert.equal(got.version, 7);
  assert.equal(validate(got).version, "7");
});

test("anything off is refused", () => {
  const bad = [
    with_({ schema: 2 }),
    with_({ buttons: [] }),
    with_({ buttons: ["#a", 5] }),
    with_({ captions: 5 }),
    with_({ line: "div { color: red }" }),
    with_({ author: "a[" }),
    with_({ label: "(" }),
    with_({ label: "x".repeat(201) }),
    with_({ labelFlags: "g" }),
    with_({ pressed: "maybe" }),
    with_({ version: "../x" }),
    with_({ words: "a".repeat(70_000) }),
    "[1]",
    "not json",
  ];
  for (const b of bad) assert.equal(parseRemote(b, looksLikeCss), null, b.slice(0, 60));
});

test("a selector the page can't compile is refused when the caller can check", () => {
  assert.equal(validate(bundled, (s) => s !== "#microphone-button"), null);
});

test("unreachable, failing or invalid remote falls back to the bundled copy", async () => {
  const none = [async () => { throw new Error("offline"); }, reply("{}", false), reply("nonsense"), reply(with_({ schema: 99 }))];
  for (const get of none) {
    const got = await fetchRemote(get);
    assert.equal(got, null);
    assert.equal(pick(got, bundled), bundled);
  }
  assert.equal(pick({ schema: 99 }, bundled), bundled, "a stale cache from another schema too");
  const newer = { ...bundled, version: "2030-01-01" };
  assert.equal(pick(newer, bundled), newer);
});

test("a miss with a remote table is still unknown and unavailable, never a guess", () => {
  useRules(validate({ ...bundled, version: "x", buttons: ["#nope"], captions: "#nope-captions" }));
  const empty = { querySelector: () => null, querySelectorAll: () => [] };
  assert.equal(readPage(empty), "unknown");
  assert.equal(readCaptions(empty), null);
});
