import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";
import { rebuild, type Edit } from "../src/lib/trace";

// Spans are in characters of the raw text; build them from the text so the samples stay readable.
const SAMPLES = `(() => {
  const E = (raw, kind, find, to, nth = 0, extra = {}) => { let at = -1; for (let i = 0; i <= nth; i++) at = raw.indexOf(find, at + 1); return { kind, start: at, end: at + find.length, to, ...extra }; };
  const ago = (h) => Date.now() - h * 3600e3;
  const step = (raw, edits, language = "en", tidy = true, cleaned = raw) => [
    { kind: "speech-to-text", text: raw, ms: 312, settings: null, edits: null },
    { kind: "rules", text: cleaned, ms: 1, settings: { language, tidy }, edits },
  ];
  const base = { mode: "hold", app: "Slack", model: "Parakeet TDT 0.6B v3", audio_ms: 7400, outcome: "pasted", reason: null, load_ms: 0, paste_ms: 388 };
  const r1 = "Um, so I I think we should uh ship the post gres fix with twenty uh five pods. Ask Drove to ch- check Kubernetes.";
  const r4 = "Ich glaube, äh, wir sollten das das mit Myna machen.";
  return [
    { ...base, id: 1, started: ago(1), steps: step(r1, [E(r1, "filler", "Um, s", "S"), E(r1, "filler", "uh ", "", 0), E(r1, "filler", "uh ", "", 1), E(r1, "repeat", "I ", "", 1), E(r1, "cutoff", "ch- ", ""),
      E(r1, "number", "twenty uh five", "25"), E(r1, "word", "post gres", "Postgres", 0, { matched: "spelling", spelling: "Postgres", heard: null, unchanged: false }),
      E(r1, "word", "Drove", "Dhruv", 0, { matched: "heard", spelling: "Dhruv", heard: "Drove", unchanged: false }),
      E(r1, "word", "Kubernetes", "Kubernetes", 0, { matched: "spelling", spelling: "Kubernetes", heard: null, unchanged: true })]) },
    { ...base, id: 2, started: ago(30), app: "Mail", mode: "tap", load_ms: 1420, paste_ms: 1710, steps: step("Send the deck to Priya before lunch.", []) },
    { ...base, id: 3, started: ago(60), app: "Notes", steps: step("Uh I I think postgres is fine.", [E("Uh I I think postgres is fine.", "word", "postgres", "Postgres", 0, { matched: "spelling", spelling: "Postgres", heard: null, unchanged: false })], "en", false) },
    { ...base, id: 4, started: ago(90), app: "Safari", steps: step(r4, [E(r4, "word", "Myna", "Myna", 0, { matched: "spelling", spelling: "Myna", heard: null, unchanged: true })], "de") },
    { ...base, id: 5, started: ago(120), app: "Terminal", outcome: "nothing_heard", audio_ms: 900, paste_ms: null, steps: step("Um.", [E("Um.", "filler", "Um.", "")], "en", true, "") },
    { ...base, id: 6, started: ago(150), app: null, outcome: "failed", reason: "The speech model didn't load.", paste_ms: null, load_ms: 80, steps: [] },
  ];
})()`;

async function open(page: Page, experimental = true) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    look_get: () => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: ${experimental} }),
    look_set: () => {},
    rules_get: () => ({ settings: { language: "en", tidy: true, words: [{ spelling: "Postgres", heard: null }, { spelling: "Dhruv", heard: "Drove" }, { spelling: "Kubernetes", heard: null }, { spelling: "Desert Ant", heard: null }] }, languages: [["en", "English"], ["de", "German"]] }),
    traces_list: () => ${SAMPLES}.map((t) => ({ id: t.id, started: t.started, app: t.app, outcome: t.outcome, heard: t.steps[0]?.text ?? null })),
    trace_get: ({ id }) => ${SAMPLES}.find((t) => t.id === id),
  }`);
  await page.goto("/settings");
}
const go = async (page: Page, n: number) => {
  await page.getByRole("option", { name: "Stats for nerds" }).click();
  await page.getByRole("list", { name: "Dictations" }).getByRole("button").nth(n).click(); // the list is newest first: 0 is trace 1
};

test("Stats for nerds is in the sidebar only while Experimental features is on", async ({ page }) => {
  await open(page, false);
  await expect(page.getByRole("option", { name: "Stats for nerds" })).toHaveCount(0);
  await page.getByRole("option", { name: "Experimental features" }).click();
  await page.getByLabel("Use experimental features").check();
  await expect(page.getByRole("option", { name: "Stats for nerds" })).toBeVisible();
  await page.getByLabel("Use experimental features").uncheck();
  await expect(page.getByRole("option", { name: "Stats for nerds" })).toHaveCount(0);
});

test("lists traces newest first with title, app, time and outcome", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "Stats for nerds" }).click();
  const rows = page.getByRole("list", { name: "Dictations" }).getByRole("button");
  await expect(rows).toHaveCount(6);
  await expect(rows.nth(0)).toContainText("Um, so I I think we");
  await expect(rows.nth(0)).toContainText("Slack · Today");
  await expect(rows.nth(0)).toContainText("Pasted");
  await expect(rows.nth(1)).toContainText("Mail · Yesterday");
  await expect(rows.nth(4)).toContainText("Didn’t hear anything");
  await expect(rows.nth(5)).toContainText("Nothing heard");
  await expect(rows.nth(5)).toContainText("Failed");
});

test("an empty list says so", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: true }), traces_list: () => [] }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Stats for nerds" }).click();
  await expect(page.getByText("No dictations yet")).toBeVisible();
});

test("a trace with many edits: nested ones show the outermost, chips switch rules off, words are marked", async ({ page }) => {
  await open(page);
  await go(page, 0);
  await expect(page.getByRole("heading", { name: "Um, so I I think we…" })).toBeVisible();
  await expect(page.getByText("Today", { exact: false }).first()).toContainText("Slack · Hold to talk · Parakeet TDT 0.6B v3");
  const timing = page.getByLabel("Timing", { exact: true }).first();
  await expect(timing).toContainText("388 ms");
  await expect(timing).toContainText("7.4 s of audio");
  await expect(timing).toContainText("Speech to text 312 ms");
  await expect(timing).not.toContainText("Model load wait");
  const card = page.getByRole("region", { name: "What Myna changed" });
  await expect(card).toContainText("8 edits · English · Tidy up on");
  // "twenty uh five" is one number edit: the filler inside it isn't marked on its own.
  await expect(card.locator('.e[data-k="number"] del')).toHaveText("twenty uh five");
  await expect(card.locator('.e[data-k="number"] ins')).toHaveText("25");
  await expect(card.locator('.e[data-k="filler"]')).toHaveCount(2); // "Um, s" and one "uh "; the third (inside the number) counts in the chip only
  await expect(card.locator('.e.same')).toHaveText("Kubernetes");
  await expect(card.getByRole("button", { name: /Filler words\s*3/ })).toBeVisible();
  await expect(card.getByRole("button", { name: /Words to remember\s*2/ })).toBeVisible();
  await card.getByRole("button", { name: /Numbers/ }).click();
  await expect(card.locator('.e[data-k="number"]')).toHaveCount(0);
  await expect(card).toContainText("with 25 pods");
  await card.getByRole("button", { name: /Numbers/ }).click();
  await expect(card.locator('.e[data-k="number"]')).toHaveCount(1);
  const words = card.locator(".words");
  await expect(words.locator(".w", { hasText: "Drove → Dhruv" })).toContainText("fixed");
  await expect(words.locator(".w", { hasText: "Kubernetes" })).toContainText("already right");
  await expect(words.locator(".w", { hasText: "Desert Ant" })).toContainText("not heard");
  await card.locator('.e[data-k="number"]').hover();
  await expect(page.locator(".tip")).toContainText("Number");
  await expect(page.locator(".tip")).toContainText("“twenty uh five” → “25”");
  await expect(page.locator(".tip")).toContainText("Contains filler word “uh”, removed first");
  await card.locator('.e[data-k="word"]').nth(1).hover();
  await expect(page.locator(".tip")).toContainText("matched by heard-as “Drove”");
});

test("a cold start shows the model load wait; Back returns to the list", async ({ page }) => {
  await open(page);
  await go(page, 1);
  await expect(page.getByText("No edits", { exact: false })).toHaveCount(0);
  await expect(page.getByRole("region", { name: "What Myna changed" })).toContainText("nothing · English · Tidy up on");
  await expect(page.getByLabel("Timing", { exact: true }).first()).toContainText("Model load wait 1.42 s");
  await expect(page.getByText("Tap to start and stop")).toBeVisible();
  await page.getByRole("button", { name: "‹ All dictations" }).click();
  await expect(page.getByRole("list", { name: "Dictations" })).toBeVisible();
});

test("Tidy up off says only Words to remember ran", async ({ page }) => {
  await open(page);
  await go(page, 2);
  await expect(page.getByRole("region", { name: "What Myna changed" })).toContainText("1 edit · English · Tidy up off · only Words to remember ran");
});

test("a non-English trace keeps its words as said", async ({ page }) => {
  await open(page);
  await go(page, 3);
  const card = page.getByRole("region", { name: "What Myna changed" });
  await expect(card).toContainText("German · Tidy up on · only Words to remember ran");
  await expect(card).toContainText("Ich glaube, äh, wir sollten das das mit Myna machen.");
  await expect(card.locator("del")).toHaveCount(0);
});

test("only filler: nothing heard is in the caution style", async ({ page }) => {
  await open(page);
  await go(page, 4);
  await expect(page.getByText("Nothing left after cleanup, so nothing was pasted.")).toBeVisible();
  await expect(page.getByText("from letting go to done")).toBeVisible();
  await expect(page.locator(".badge.bad")).toHaveText("Didn’t hear anything");
});

test("a failure shows why and has no changed card", async ({ page }) => {
  await open(page);
  await go(page, 5);
  await expect(page.getByText("The speech model didn't load.")).toBeVisible();
  await expect(page.locator(".badge.bad")).toHaveText("Failed");
  await expect(page.getByRole("region", { name: "What Myna changed" })).toHaveCount(0);
});

test("opening a trace doesn't move the page's header or reflow the card", async ({ page }) => {
  await open(page);
  await go(page, 0);
  const card = page.getByRole("region", { name: "What Myna changed" });
  const before = await card.boundingBox();
  await card.getByRole("button", { name: /Filler words/ }).click();
  await card.getByRole("button", { name: /Filler words/ }).click();
  expect((await card.boundingBox())!.y).toBe(before!.y);
});

// "Twenty uh five": the filler sits inside the number; the outermost edit wins.
test("rebuilding the cleaned text from raw + edits: outermost wins, spans count characters", () => {
  const raw = "Um, I got twenty uh five 😀 pods";
  const at = (s: string) => Array.from(raw.slice(0, raw.indexOf(s))).length;
  const e = (kind: Edit["kind"], s: string, to: string) => ({ kind, start: at(s), end: at(s) + Array.from(s).length, to }) as Edit;
  const filler = e("filler", "uh ", ""), number = e("number", "twenty uh five", "25"), um = e("filler", "Um, I", "I");
  expect(rebuild(raw, [um, filler, number])).toBe("I got 25 😀 pods");
  expect(rebuild(raw, [number, filler, um])).toBe("I got 25 😀 pods"); // order applied doesn't matter
  expect(rebuild(raw, [])).toBe(raw);
});
