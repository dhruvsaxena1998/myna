// Run with `pnpm test:unit`. Node strips the types from meetings.ts itself.
import assert from "node:assert/strict";
import test from "node:test";
import { changes } from "../src/lib/meetings.ts";

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
