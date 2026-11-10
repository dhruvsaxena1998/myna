// Chrome runs content scripts as classic scripts, not modules: anything only a module allows (a top-level await,
// import/export statements) makes the whole file fail to load. `node --check` reads .js here as a module, so it can't tell.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

test("content.js parses as a classic script", () => {
  const src = readFileSync(new URL("../../extension/content.js", import.meta.url), "utf8");
  assert.doesNotThrow(() => new vm.Script(src, { filename: "content.js" }));
});
