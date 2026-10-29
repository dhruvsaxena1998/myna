// Screenshot a page of the dev harness in light or dark, with headless Chrome over the DevTools protocol.
//   node dev/shot.mjs "http://localhost:8765/dev/?tab=chat#meetings/1" out.png dark [width height]
// Needs Google Chrome in /Applications and Node 22+. Run shots one at a time (they share a port).
// Optional JS (the body of an async function, so it may await) to run before the shot: SHOT_JS="document.querySelector('#q').focus()".
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const [url, out, scheme = "light", w = 1100, h = 720] = process.argv.slice(2);
const chrome = spawn("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", ["--headless=new", "--remote-debugging-port=9333", "--hide-scrollbars", "--user-data-dir=/tmp/myna-shot-profile", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  let page;
  for (let i = 0; i < 50 && !page; i++) {
    await sleep(200);
    page = await fetch("http://127.0.0.1:9333/json").then((r) => r.json()).then((l) => l.find((t) => t.type === "page")).catch(() => null);
  }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pending = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); pending.get(m.id)?.(m.result); };
  const cdp = (method, params = {}) => new Promise((r) => { pending.set(++id, r); ws.send(JSON.stringify({ id, method, params })); });
  await cdp("Emulation.setDeviceMetricsOverride", { width: +w, height: +h, deviceScaleFactor: 1, mobile: false });
  await cdp("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  // Page errors would otherwise leave a blank screenshot with no clue why.
  await cdp("Page.addScriptToEvaluateOnNewDocument", { source: "window.__errors = []; const log = (e) => __errors.push(String(e.error?.stack || e.reason?.stack || e.reason || e.message)); addEventListener('error', log); addEventListener('unhandledrejection', log);" });
  await cdp("Page.navigate", { url });
  // The harness writes the real page over itself, so wait until the app has drawn something, then let async panes settle.
  for (let i = 0; i < 100; i++) {
    await sleep(100);
    if ((await cdp("Runtime.evaluate", { expression: "!!document.getElementById('app')?.children.length" })).result?.value) break;
  }
  await sleep(1500);
  if (process.env.SHOT_JS) {
    const r = await cdp("Runtime.evaluate", { expression: `(async () => { ${process.env.SHOT_JS} })()`, awaitPromise: true });
    if (r.exceptionDetails) console.error("SHOT_JS failed:", r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  }
  const errors = (await cdp("Runtime.evaluate", { expression: "JSON.stringify(window.__errors || [])" })).result?.value;
  if (errors && errors !== "[]") console.error("Page errors:", errors);
  writeFileSync(out, Buffer.from((await cdp("Page.captureScreenshot", { format: "png" })).data, "base64"));
} finally {
  chrome.kill();
}
process.exit(0);
