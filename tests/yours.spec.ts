import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";
import { minutes, yours } from "../src/lib/yours";
import type { Trace } from "../src/lib/trace";

const NOW = new Date(2026, 10, 6, 10, 30).getTime(); // Fri 6 Nov, 10:30 local
const at = (d: number, h = 9) => new Date(2026, 10, d, h).getTime();
let id = 0;
// A trace pasting `n` words.
const T = (o: Partial<Trace> & { n?: number } = {}): Trace => {
  const { n = 4, ...t } = o, text = Array.from({ length: n }, (_, i) => `w${i}`).join(" ");
  return { id: ++id, started: at(6), mode: "hold", app: "Slack", model: "Parakeet", audio_ms: 1000, outcome: "pasted", reason: null, load_ms: 0, paste_ms: 100,
    steps: [{ kind: "speech-to-text", text: "raw", ms: 1, settings: null, edits: null }, { kind: "rules", text, ms: 1, settings: null, edits: [] }], ...t };
};

test("words and dictations count only pasted traces in the period", () => {
  const ts = [T({ n: 10 }), T({ n: 5, started: at(1) }), T({ n: 99, outcome: "failed" }), T({ n: 7, started: at(5) }), T({ n: 50, started: new Date(2026, 9, 20).getTime() })];
  expect(yours(ts, 7, NOW)).toMatchObject({ words: 22, n: 3 });
  expect(yours(ts, 30, NOW)).toMatchObject({ words: 72, n: 4 });
});

test("time saved is typing at 40 wpm minus speaking time, never below 0", () => {
  // 80 words = 120 s of typing, 20 s spoken.
  expect(yours([T({ n: 80, audio_ms: 20000 })], 7, NOW).saved).toBe(100000);
  expect(yours([T({ n: 4, audio_ms: 60000 })], 7, NOW).saved).toBe(0);
  expect(yours([], 7, NOW).saved).toBe(0);
  expect([minutes(30e3), minutes(12 * 6e4), minutes(65 * 6e4), minutes(120 * 6e4)]).toEqual(["under a minute", "12 min", "1 h 5 min", "2 h"]);
});

test("a streak may end yesterday, a gap breaks it, and today with none keeps it", () => {
  expect(yours([T({ started: at(6) }), T({ started: at(5) }), T({ started: at(4) })], 7, NOW).streak).toBe(3);
  expect(yours([T({ started: at(5) }), T({ started: at(4) })], 7, NOW).streak).toBe(2);
  expect(yours([T({ started: at(6) }), T({ started: at(5) }), T({ started: at(3) })], 7, NOW).streak).toBe(2);
  expect(yours([T({ started: at(4) })], 7, NOW).streak).toBe(0);
  expect(yours([T({ started: at(6), outcome: "failed" })], 7, NOW).streak).toBe(0);
  // Not limited by the period.
  expect(yours(Array.from({ length: 12 }, (_, i) => T({ started: at(6 - i) })), 7, NOW).streak).toBe(12);
});

test("one bucket a day, today last", () => {
  const y7 = yours([T({ n: 3 }), T({ n: 2 }), T({ n: 9, started: at(5) })], 7, NOW);
  expect(y7.buckets).toHaveLength(7);
  expect(y7.buckets.at(-1)).toEqual({ label: "6 Nov", words: 5 });
  expect(y7.buckets.at(-2)).toEqual({ label: "5 Nov", words: 9 });
  expect(yours([], 30, NOW).buckets).toHaveLength(30);
});

test("top apps: three by words, a missing app is Unknown app", () => {
  const ts = [T({ n: 5, app: "A" }), T({ n: 9, app: "B" }), T({ n: 1, app: "C" }), T({ n: 7, app: null }), T({ n: 2, app: "B" })];
  expect(yours(ts, 7, NOW).apps).toEqual([{ name: "B", words: 11 }, { name: "Unknown app", words: 7 }, { name: "A", words: 5 }]);
});

test("the page: nav entry without Experimental, four tiles, 30 days draws 30 bars", async ({ page }) => {
  const now = Date.now(), DAY = 864e5;
  const traces = [
    T({ n: 80, audio_ms: 20000, started: now - 60e3 }), T({ n: 40, audio_ms: 10000, started: now - DAY }),
    T({ n: 100, audio_ms: 10000, started: now - 20 * DAY, app: "Mail" }),
  ];
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], traces_all: () => ${JSON.stringify(traces)} }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: "Your dictation" }).click();
  const tile = (n: string) => page.locator(".tile", { has: page.locator(".label", { hasText: n }) });
  await expect(page.getByRole("radio", { name: "7 days" })).toBeChecked();
  await expect(tile("Words dictated")).toContainText("120");
  await expect(tile("Time saved")).toContainText("2 min"); // 180 s typing - 30 s speaking
  await expect(tile("Dictations")).toContainText("2");
  await expect(tile("Streak")).toContainText("2 days");
  await expect(page.locator(".col")).toHaveCount(7);
  await expect(page.getByRole("region", { name: "Top apps" })).toContainText("Slack");
  await page.getByRole("radio", { name: "30 days" }).check();
  await expect(page.locator(".col")).toHaveCount(30);
  await expect(tile("Words dictated")).toContainText("220");
});
