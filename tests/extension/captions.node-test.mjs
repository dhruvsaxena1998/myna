// Run with `pnpm test:extension`. The markup is what the guessed rules in extension/selectors.js expect of Teams'
// live captions, not a saved page: replace it with the real thing when we have it.
import assert from "node:assert/strict";
import test from "node:test";
import { newCaptions, readCaptions } from "../../extension/selectors.js";

const text = (t) => ({ textContent: t });
/** A caption box of `[name, words]` lines, answering the selectors the TEAMS table uses. */
const page = (lines, rules = { captions: "#box", line: ".line", author: ".who", words: ".said" }) => ({
  querySelector: (sel) =>
    sel === rules.captions && lines
      ? { querySelectorAll: () => lines.map(([n, w]) => ({ querySelector: (s) => (s === rules.author ? (n == null ? null : text(n)) : s === rules.words ? text(w) : null) })) }
      : null,
});
const rules = { captions: "#box", line: ".line", author: ".who", words: ".said" };

test("each caption line gives its speaker and words, trimmed and capped", () => {
  const got = readCaptions(page([[" Ana Ruiz ", " hello there "], ["B".repeat(200), "w".repeat(500)]]), rules);
  assert.deepEqual(got[0], { name: "Ana Ruiz", text: "hello there" });
  assert.equal(got[1].name.length, 80);
  assert.equal(got[1].text.length, 300);
});

test("no caption container is null, never an empty guess; a line without a name is skipped", () => {
  assert.equal(readCaptions(page(null), rules), null);
  assert.deepEqual(readCaptions(page([[null, "orphan"], ["Bo", "hi"]]), rules), [{ name: "Bo", text: "hi" }]);
});

test("a caption that grows is one line, an old one scrolling by is not new, a new one is", () => {
  const seen = new Map();
  const ana = (t) => ({ name: "Ana", text: t });
  assert.equal(newCaptions([ana("so the")], seen).length, 1);
  assert.equal(newCaptions([ana("so the launch is")], seen).length, 0, "the same line, longer");
  assert.equal(newCaptions([ana("so the launch is")], seen).length, 0);
  assert.equal(newCaptions([ana("so the launch is"), { name: "Bo", text: "ok" }, ana("and the budget")], seen).map((c) => c.name).join(), "Bo,Ana");
  assert.equal(newCaptions([ana("so the launch is"), ana("and the budget")], seen).length, 0, "old lines still on screen");
});
