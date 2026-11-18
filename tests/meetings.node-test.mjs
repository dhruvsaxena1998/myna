// Run with `pnpm test:unit`. Node strips the types from meetings.ts itself.
import assert from "node:assert/strict";
import test from "node:test";
import { changes, groups } from "../src/lib/meetings.ts";

const marked = (heard, clean) => changes(heard, clean).filter((c) => c.heard !== undefined).map((c) => [c.text, c.heard]);
const shown = (heard, clean) => changes(heard, clean).map((c) => c.text).join(" ");

test("the cleaned line shows as it is, with each changed run marked with what was heard", () => {
  assert.deepEqual(marked("so um we ship on friday", "So we ship on Friday."), [["So", "so um"], ["Friday.", "friday"]]);
  assert.equal(shown("so um we ship on friday", "So we ship on Friday."), "So we ship on Friday.");
});

test("an unchanged line has nothing marked, and words only taken out don't show", () => {
  assert.deepEqual(changes("ship it now", "ship it now"), [{ text: "ship it now" }]);
  assert.deepEqual(marked("ship uh it", "ship it"), []);
  assert.equal(shown("ship uh it", "ship it"), "ship it");
});

test("a word cleanup added is marked with nothing heard", () => {
  assert.deepEqual(marked("call me at nine", "Call me at 9."), [["Call", "call"], ["9.", "nine"]]);
  assert.deepEqual(marked("ship it", "ship it today"), [["today", ""]]);
  assert.deepEqual(changes("", "Hi."), [{ text: "Hi.", heard: "" }]);
});

const l = (id, speaker, start_ms, end_ms, flags = {}) => ({ id, speaker, start_ms, end_ms, text: `t${id}`, clean: null, muted: false, included: false, hidden: false, ...flags });
const ids = (rows) => rows.map((g) => ("left" in g ? ["left", ...g.left.map((x) => x.id)] : g.lines.map((x) => x.id)));

test("one speaker's lines with short pauses read as one paragraph; a long pause, a new speaker or a minute starts another", () => {
  const rows = groups([
    l(1, "Me", 0, 1000), l(2, "Me", 1500, 3000), l(3, "Me", 5000, 6000), // 2 s pause: new paragraph
    l(4, "Them", 6100, 7000), l(5, "Me", 7100, 8000), // speaker changes
  ]);
  assert.deepEqual(ids(rows), [[1, 2], [3], [4], [5]]);
  const long = Array.from({ length: 70 }, (_, i) => l(i, "Me", i * 1000, i * 1000 + 900));
  assert.deepEqual(groups(long).map((g) => g.lines.length), [60, 10]);
});

test("left-out lines collapse into their own row, and an included line never joins ordinary ones", () => {
  const rows = groups([l(1, "Me", 0, 1000), l(2, "Me", 1100, 2000, { hidden: true }), l(3, "Me", 2100, 3000, { hidden: true }), l(4, "Me", 3100, 4000), l(5, "Me", 4100, 5000, { included: true })]);
  assert.deepEqual(ids(rows), [[1], ["left", 2, 3], [4], [5]]);
});
