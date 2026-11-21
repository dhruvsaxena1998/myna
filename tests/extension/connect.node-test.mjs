// The service worker's connect loop, with chrome and WebSocket faked: a Myna on an earlier port that refuses the code
// (the installed app beside a dev build) must not stop the add-on from reaching the Myna that takes it.
import assert from "node:assert/strict";
import test from "node:test";
import { PORTS, V, WELCOME, WRONG_CODE } from "../../extension/messages.js";

const statuses = [];
let answer = () => "none"; // port → "wrong" | "welcome" | "none"
const tried = [];
const noop = () => {};
const event = { addListener: noop };
globalThis.chrome = {
  storage: { local: { get: async () => ({ code: "123456" }) }, session: { set: async (o) => "status" in o && statuses.push(o.status) }, onChanged: event },
  tabs: { onRemoved: event, onUpdated: event, query: async () => [], sendMessage: async () => {} },
  runtime: { onMessage: event, onStartup: event, onInstalled: event, getManifest: () => ({ host_permissions: [] }), getURL: (p) => p },
  offscreen: { closeDocument: async () => {} },
  alarms: { get: async () => ({}), create: noop, onAlarm: event },
};
globalThis.fetch = async () => ({ json: async () => ({ version: 1 }) });
globalThis.WebSocket = class {
  static OPEN = 1;
  constructor(url) {
    const port = +url.split(":").pop();
    tried.push(port);
    setTimeout(() => {
      const a = answer(port);
      if (a === "none") return this.onclose({ code: 1006 });
      this.readyState = 1;
      this.onopen();
      if (a === "wrong") this.onclose({ code: WRONG_CODE });
      else this.onmessage({ data: JSON.stringify({ v: V, type: WELCOME }) });
    });
  }
  send() {}
};
const settle = () => new Promise((r) => setTimeout(r, 50));

test("a Myna that refuses the code doesn't stop the add-on reaching the one that takes it, and only all refusing is Wrong code", async () => {
  answer = (p) => (p === PORTS[0] ? "wrong" : p === PORTS[1] ? "welcome" : "none");
  await import("../../extension/background.js");
  await settle();
  assert.deepEqual(tried, PORTS.slice(0, 2));
  assert.equal(statuses.at(-1), "connected");

  // A fresh worker, and now every Myna refuses the code.
  answer = (p) => (p === PORTS[0] ? "wrong" : "none");
  tried.length = 0;
  await import("../../extension/background.js?again");
  await settle();
  assert.deepEqual(tried, PORTS, "every port is tried before giving up");
  assert.equal(statuses.at(-1), "wrong-code");
});
