// The add-on's side of the protocol fixture; Myna's side is a test in src-tauri/src/addon.rs.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { frame } from "../../extension/audio.js";
import * as msg from "../../extension/messages.js";

const fx = JSON.parse(readFileSync(new URL("./protocol.json", import.meta.url)));
const fromMyna = [msg.WELCOME, msg.STOP_TAB];

test("the constants are the fixture's", () => {
  assert.equal(msg.V, fx.version);
  assert.deepEqual(msg.PORTS, fx.ports);
  assert.equal(msg.WRONG_CODE, fx.close.wrongCode);
  assert.equal(msg.REPLACED, fx.close.replaced);
});

test("every message matches the fixture", () => {
  for (const { from, wire, args } of fx.messages) {
    const name = JSON.stringify(wire);
    if (from === "addon") assert.deepEqual(msg[wire.type](...args), wire, name);
    else {
      assert.deepEqual(msg.parse(JSON.stringify(wire)), wire, name);
      assert.ok(fromMyna.includes(wire.type), name);
    }
  }
});

test("what the add-on doesn't understand is ignored", () => {
  for (const raw of fx.ignored) {
    const type = msg.parse(raw)?.type;
    assert.ok(!fromMyna.includes(type), raw);
  }
});

test("frames match the fixture", () => {
  for (const f of fx.frames) assert.deepEqual([...frame(f.seq, f.ts, f.samples)], f.bytes, `seq ${f.seq}`);
});
