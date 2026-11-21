// The add-on's popup uses the app's tokens and components; its copy must not drift from design/tokens.css.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

test("the add-on's tokens.css is the app's design/tokens.css", () => {
  const read = (p) => readFileSync(new URL(p, import.meta.url), "utf8");
  assert.equal(read("../../extension/tokens.css"), read("../../design/tokens.css"), "run: cp design/tokens.css extension/tokens.css");
});
