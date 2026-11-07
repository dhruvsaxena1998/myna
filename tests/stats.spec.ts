import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";
import { overview, select, steps, quantile, type Opts } from "../src/lib/stats";
import type { Edit, Trace } from "../src/lib/trace";

// A pasted trace with sensible defaults; `heard` is what speech to text wrote.
let id = 0;
const T = (o: Partial<Trace> & { heard?: string; edits?: Edit[]; stt?: number; rules?: number } = {}): Trace => {
  const { heard = "one two three four", edits = [], stt = 100, rules = 10, ...t } = o;
  return {
    id: ++id, started: NOW - 3600e3, mode: "hold", app: "Slack", model: "Parakeet", audio_ms: 4000, outcome: "pasted", rating: null, reason: null, load_ms: 0, paste_ms: 200,
    steps: [{ kind: "speech-to-text", text: heard, ms: stt, settings: null, edits: null }, { kind: "rules", text: heard, ms: rules, settings: { language: "en", tidy: true }, edits }],
    ...t,
  };
};
const NOW = new Date(2026, 10, 6, 10, 30).getTime(); // Fri 6 Nov, 10:30 local
const at = (d: number, h = 0, m = 0, s = 0, ms = 0) => new Date(2026, 10, d, h, m, s, ms).getTime();
const O = (o: Partial<Opts> = {}): Opts => ({ period: "all", model: "all", app: "all", now: NOW, words: [], ...o });
const many = (n: number, o: Parameters<typeof T>[0] = {}) => Array.from({ length: n }, () => T(o));
const word = (spelling: string, heard: string | null, unchanged: boolean): Edit => ({ kind: "word", start: 0, end: 3, to: spelling, matched: "spelling", spelling, heard, unchanged });

test("periods start at local midnight: today, and 7 days is today and the 6 days before it", () => {
  const oct31 = new Date(2026, 9, 31).getTime(), oct30late = new Date(2026, 9, 30, 23, 59, 59, 999).getTime();
  const all = [T({ started: at(6, 0) }), T({ started: at(5, 23, 59, 59, 999) }), T({ started: oct31 }), T({ started: oct30late })];
  const ids = (p: Opts["period"]) => select(all, O({ period: p })).map((t) => t.started);
  expect(ids("today")).toEqual([at(6, 0)]);
  expect(ids("7")).toEqual([at(6, 0), at(5, 23, 59, 59, 999), oct31]);
  expect(ids("all")).toHaveLength(4);
});

test("the outcome split counts not pasted as failed", () => {
  const ov = overview([T(), T({ outcome: "nothing_heard" }), T({ outcome: "failed" }), T({ outcome: "not_pasted" })], O());
  expect(ov.split).toEqual({ pasted: 1, nothing_heard: 1, failed: 2 });
  expect(ov.models[0].failed.v).toBe(0.5);
  expect(ov.apps[0].failedShare.v).toBe(0.5);
});

test("speed is audio over speech-to-text time, pasted dictations only", () => {
  const ts = [...many(10, { audio_ms: 4000, stt: 100 }), T({ audio_ms: 4000, stt: 300 }), T({ outcome: "failed", audio_ms: 99999, stt: 1 })];
  expect(overview(ts, O()).models[0].speed.v).toBeCloseTo(44000 / 1300);
  expect(overview(ts, O()).models[0].speed.n).toBe(11);
});

test("step medians: paste is what's left of let go → pasted after the other three", () => {
  expect(steps(T({ load_ms: 50, stt: 100, rules: 10, paste_ms: 200 }))).toEqual([50, 100, 10, 0, 40]);
  const ts = [...many(6, { stt: 100, paste_ms: 200 }), ...many(5, { stt: 300, paste_ms: 400 }), T({ outcome: "nothing_heard", stt: 999, paste_ms: null })];
  expect(overview(ts, O()).models[0].steps.v).toEqual([0, 100, 10, 0, 90]);
});

test("cleanup time is its own step, and zero for a dictation without cleanup", () => {
  const withCleanup = T({ paste_ms: 1200 });
  withCleanup.steps.push({ kind: "cleanup", text: "x", ms: 900, settings: { model: "m", cost: null, fallback: null, detail: null }, edits: null });
  expect(steps(withCleanup)).toEqual([0, 100, 10, 900, 190]);
});

test("medians and p90 are nearest rank", () => {
  expect(quantile([5, 1, 4, 2, 3], 0.5)).toBe(3);
  expect(quantile([1, 2, 3, 4], 0.5)).toBe(2);
  expect(quantile(Array.from({ length: 10 }, (_, i) => i + 1), 0.9)).toBe(9);
  expect(quantile([], 0.5)).toBe(0);
});

test("already-right words to remember aren't edits, but count as heard", () => {
  const ts = [T({ heard: "kub is fine", edits: [word("Kubernetes", null, true), word("Postgres", null, false), { kind: "filler", start: 0, end: 4, to: "" }] })];
  const ov = overview(ts, O({ words: [{ spelling: "Kubernetes", heard: null }, { spelling: "Postgres", heard: null }, { spelling: "Myna", heard: null }] }));
  expect(ov.rules.find((r) => r.kind === "word")!.edits).toBe(1);
  expect(ov.per100.v).toBeCloseTo((2 / 3) * 100);
  expect(ov.heardWords.map((w) => [w.spelling, w.fixed, w.right])).toEqual([["Kubernetes", 0, 1], ["Postgres", 1, 0]]);
  expect(ov.neverHeard.map((w) => w.spelling)).toEqual(["Myna"]);
});

test("fillers group by what was removed, lowercased and trimmed", () => {
  const raw = "Um, it works, uh. Um ok";
  const f = (s: string) => { const i = raw.indexOf(s); return { kind: "filler" as const, start: i, end: i + s.length, to: "" }; };
  expect(overview([T({ heard: raw, edits: [f("Um, "), f(", uh"), f("Um ")] })], O()).fillers).toEqual([["um", 2], ["uh", 1]]);
});

test("rates, medians and shares wait for 10 dictations behind them; counts don't", () => {
  const nine = overview([...many(9), T({ outcome: "failed" })], O());
  expect(nine.n).toBe(10);
  expect(nine.cold.n).toBe(10); // a share over every dictation
  expect(nine.med.n).toBe(9); // let go → pasted is over pasted ones only
  expect(nine.wait.n).toBe(0); // median wait is over cold starts only
  const cold = overview(many(3, { load_ms: 900 }), O());
  expect([cold.wait.n, cold.wait.v, cold.cold.v]).toEqual([3, 900, 1]);
});

test("By app: the top 8 by count, then one Other row", () => {
  const ts = Array.from({ length: 10 }, (_, i) => many(10 - i, { app: `App ${i}` })).flat();
  const apps = overview(ts, O()).apps;
  expect(apps.map((a) => a.name)).toEqual(["App 0", "App 1", "App 2", "App 3", "App 4", "App 5", "App 6", "App 7", null]);
  expect([apps[8].others, apps[8].n]).toEqual([2, 3]);
  expect(overview(many(3), O()).apps).toHaveLength(1);
});

test("the chart: by hour today, a day a column otherwise, and All goes back to the first trace", () => {
  const ts = [T({ started: at(6, 9, 15) }), T({ started: at(6, 9, 45), outcome: "failed" }), T({ started: at(1, 12) })];
  const today = overview(ts, O({ period: "today" })).chart;
  expect(today.buckets).toHaveLength(24);
  expect([today.buckets[9].label, today.buckets[9].n, today.buckets[9].split.failed, today.top]).toEqual(["09:00", 2, 1, 4]);
  const week = overview(ts, O({ period: "7" })).chart;
  expect(week.buckets.map((b) => b.n)).toEqual([0, 1, 0, 0, 0, 0, 2]); // Sun 1 Nov is 5 days back
  expect(week.buckets.at(-1)!.label).toBe("Today");
  expect(overview(ts, O({ period: "all" })).chart.buckets).toHaveLength(7);
  expect(overview([T({ started: new Date(2026, 9, 1, 12).getTime() })], O()).chart.buckets).toHaveLength(37);
});

// ---------- The main window ----------

const DAY = 864e5;
// 30 traces in the last 7 days across two models and three apps, plus one from a removed model in an unknown app 10 days ago.
const TRACES = `(() => {
  const now = Date.now(), out = [];
  const step = (heard, ms, edits) => [{ kind: "speech-to-text", text: heard, ms, settings: null, edits: null }, { kind: "rules", text: heard, ms: 2, settings: { language: "en", tidy: true }, edits }];
  for (let i = 0; i < 30; i++) {
    const p = i < 20, outcome = i === 25 ? "not_pasted" : i % 10 === 9 ? "nothing_heard" : i > 26 ? "failed" : "pasted";
    out.push({ id: i + 1, started: now - i * 3600e3 * 4, mode: i % 2 ? "tap" : "hold", app: ["Slack", "Mail", "Notes"][i % 3], model: p ? "Parakeet TDT 0.6B v3" : "Whisper Large v3 Turbo",
      audio_ms: 4000, outcome, reason: null, load_ms: i % 5 === 0 ? 1200 : 0, paste_ms: outcome === "pasted" ? 420 : null,
      steps: step("Um, ship the fix today please", p ? 100 : 400, i === 0 ? [{ kind: "filler", start: 0, end: 4, to: "" }, { kind: "number", start: 5, end: 9, to: "x" }, { kind: "word", start: 10, end: 13, to: "Fix", matched: "spelling", spelling: "Fix", heard: null, unchanged: false }] : []) });
  }
  out.push({ id: 99, started: now - 10 * ${DAY}, mode: "hold", app: "Unknown app", model: "whisper-small (removed)", audio_ms: 2000, outcome: "pasted", reason: null, load_ms: 0, paste_ms: 300,
    steps: step("an old one", 50, []) });
  return out;
})()`;

async function open(page: Page, traces = TRACES) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    look_get: () => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: true, nerds: true }),
    rules_get: () => ({ settings: { language: "en", tidy: true, words: [{ spelling: "Fix", heard: null }, { spelling: "Kubernetes", heard: null }] }, languages: [["en", "English"]] }),
    traces_all: () => ${traces},
    trace_get: ({ id }) => ${traces}.find((t) => t.id === id),
  }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: "Stats for nerds" }).click();
}
const list = (page: Page) => page.getByRole("list", { name: "Dictations" }).getByRole("listitem");
const tile = (page: Page, name: string) => page.locator(".tile", { has: page.locator(".label", { hasText: name }) });

test("the tiles, chart and tables for the last 7 days", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("radio", { name: "7 days" })).toBeChecked();
  await expect(tile(page, "Dictations")).toContainText("30");
  await expect(tile(page, "Dictations")).toContainText("24 pasted · 3 nothing heard · 3 failed");
  await expect(tile(page, "Let go → pasted")).toContainText("420 ms");
  await expect(tile(page, "Cold starts")).toContainText("20%");
  await expect(tile(page, "Cold starts")).toContainText("median wait —"); // 6 cold starts, needs 10
  await expect(tile(page, "Cold starts").locator(".gap")).toHaveAttribute("title", "6 dictations, needs 10");
  await expect(tile(page, "Edits per 100 words")).toContainText("180 words heard, before cleanup");
  await expect(page.getByRole("heading", { name: "Dictations per day" })).toBeVisible();
  await expect(page.locator(".col")).toHaveCount(7);
  const models = page.getByRole("region", { name: "By model" });
  await expect(models.getByRole("row").nth(1)).toContainText("Parakeet TDT 0.6B v3");
  await expect(models.getByRole("row").nth(1)).toContainText("40×");
  await expect(models.getByRole("row", { name: /Whisper/ }).locator(".gap").first()).toHaveAttribute("title", /needs 10/);
  await expect(page.getByRole("region", { name: "By rule" }).getByRole("row")).toHaveCount(6);
  await expect(page.getByRole("region", { name: "By rule" })).toContainText("Kubernetes. Myna hasn’t heard these in this period.");
  await expect(page.getByRole("region", { name: "By app" }).getByRole("row")).toHaveCount(4);
  await expect(list(page)).toHaveCount(30);
  await expect(list(page).first()).toContainText("420 ms · 3 edits");
  await expect(list(page).first()).toContainText("Hold");
});

test("switching period changes every number, the chart and the list, without moving the page", async ({ page }) => {
  await open(page);
  const chart = await page.getByRole("region", { name: "Dictations per day" }).boundingBox();
  const tileBox = await tile(page, "Cold starts").boundingBox();
  await page.getByRole("radio", { name: "All" }).check();
  await expect(tile(page, "Dictations")).toContainText("31");
  await expect(page.locator(".col")).toHaveCount(11);
  await expect(page.getByRole("region", { name: "By model" })).toContainText("whisper-small (removed)");
  await expect(page.getByRole("region", { name: "By app" })).toContainText("Unknown app");
  await expect(list(page)).toHaveCount(31);
  await page.getByRole("radio", { name: "Today" }).check();
  await expect(page.getByRole("heading", { name: "Dictations by hour" })).toBeVisible();
  await expect(page.locator(".col")).toHaveCount(24);
  expect(await page.getByRole("region", { name: "Dictations by hour" }).boundingBox()).toEqual(chart);
  expect(await tile(page, "Cold starts").boundingBox()).toEqual(tileBox);
});

test("the model and app filters change the Overview and the list", async ({ page }) => {
  await open(page);
  await page.getByRole("combobox", { name: "Model" }).selectOption("Whisper Large v3 Turbo");
  await expect(tile(page, "Dictations")).toContainText("10");
  await expect(list(page)).toHaveCount(10);
  await page.getByRole("combobox", { name: "App" }).selectOption("Mail");
  await expect(list(page)).toHaveCount(3);
  await expect(page.getByRole("region", { name: "By app" }).getByRole("row")).toHaveCount(2);
  await expect(page.getByRole("combobox", { name: "Model" }).locator("option").first()).toHaveText("All models");
});

test("a table row filters only the list, with a chip that clears it", async ({ page }) => {
  await open(page);
  // The chip's slot is kept: picking a row never moves the list under the heading.
  const gap = async () => (await list(page).first().boundingBox())!.y - (await page.getByRole("heading", { name: /^Dictations \d/ }).boundingBox())!.y;
  const before = await gap();
  const row = page.getByRole("region", { name: "By app" }).getByRole("row", { name: /Mail/ });
  await row.click();
  await expect(row).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("Only Mail")).toBeVisible();
  await expect(list(page)).toHaveCount(10);
  await expect(tile(page, "Dictations")).toContainText("30"); // the Overview isn't filtered
  expect(await gap()).toBe(before);
  await row.click(); // the same row again clears it
  await expect(page.getByText("Only Mail")).toHaveCount(0);
  await expect(list(page)).toHaveCount(30);
  const rule = page.getByRole("region", { name: "By rule" }).getByRole("row", { name: /Numbers/ });
  await rule.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText("Only Numbers")).toBeVisible();
  await expect(list(page)).toHaveCount(1);
  await page.getByRole("button", { name: "Show all dictations" }).click();
  await expect(list(page)).toHaveCount(30);
});

test("opening a trace and going back keeps the period, filters and row filter", async ({ page }) => {
  await open(page);
  await page.getByRole("radio", { name: "30 days" }).check();
  await page.getByRole("region", { name: "By model" }).getByRole("row", { name: /Whisper/ }).click();
  await list(page).first().getByRole("button").click();
  await expect(page.getByRole("radio", { name: "30 days" })).toHaveCount(0);
  await page.getByRole("button", { name: "All dictations" }).click();
  await expect(page.getByRole("radio", { name: "30 days" })).toBeChecked();
  await expect(page.getByText("Only Whisper Large v3 Turbo")).toBeVisible();
});

test("no dictations in the period: the chart says so and Show all brings them back", async ({ page }) => {
  await open(page);
  await page.getByRole("combobox", { name: "Model" }).selectOption("whisper-small (removed)");
  await expect(page.locator(".plot")).toContainText("No dictations in this period");
  await expect(page.getByText("Pick a longer period to see older dictations.")).toBeVisible();
  await expect(tile(page, "Let go → pasted").locator(".gap").first()).toHaveAttribute("title", "0 dictations, needs 10");
  await page.getByRole("button", { name: "Show all" }).click();
  await expect(list(page)).toHaveCount(1);
  await expect(list(page).first()).toContainText("Unknown app");
});

test("no dictations at all", async ({ page }) => {
  await open(page, "[]");
  await expect(page.getByRole("heading", { name: "No dictations yet" })).toBeVisible();
  await expect(page.getByText("Hold your voice typing key in any app and talk.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Show all" })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "By rule" }).getByRole("row")).toHaveCount(6); // always all five
});

test("traces_all failing shows the error state", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: true, nerds: true }),
    traces_all: () => { throw "disk on fire"; } }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: "Stats for nerds" }).click();
  await expect(page.getByRole("alert")).toContainText("disk on fire");
});

test("fits the main window at its default size: nothing runs past the panel", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 640 });
  await open(page);
  await expect(page.getByRole("heading", { name: "Stats for nerds" })).toHaveCSS("white-space", "nowrap");
  const over = await page.locator("main .content").evaluate((e) => e.scrollWidth - e.clientWidth);
  expect(over).toBeLessThanOrEqual(0);
  expect((await page.getByRole("heading", { name: "Stats for nerds" }).boundingBox())!.height).toBeLessThanOrEqual(24);
});
