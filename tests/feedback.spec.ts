import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const ROWS = `[{ id: 4, started: Date.now() - 3600e3, app: "Slack", outcome: "pasted", rating: null, heard: "Mina is here", text: "Mina is here" },
  { id: 3, started: Date.now() - 7200e3, app: "Mail", outcome: "pasted", rating: "up", heard: "Send it", text: "Send it" }]`;
const FIX = `{ key: "a:mina:myna", kind: "alias", from: "Mina", to: "Myna", count: 2 }`;
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter(([c]: [string]) => c === cmd).map(([, a]: [string, unknown]) => a), cmd);

async function history(page: Page, extra = "") {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", traces_list: () => ${ROWS}, fixes_get: () => [], ${extra} }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: /^Voice typing history/ }).click(); // Meetings is the first view
  await expect(page.getByRole("heading", { name: "Voice typing history" })).toBeVisible();
}
const row = (page: Page, i: number) => page.getByRole("list", { name: "Voice typing history" }).getByRole("listitem").nth(i);

test("👍 saves at once and clicking it again clears it", async ({ page }) => {
  await history(page);
  const up = row(page, 0).getByRole("button", { name: "Good dictation" });
  await up.click();
  await expect(up).toHaveAttribute("aria-pressed", "true");
  await up.click();
  await expect(up).toHaveAttribute("aria-pressed", "false");
  expect(await calls(page, "feedback_set")).toEqual([{ trace: 4, rating: "up", corrected: null }, { trace: 4, rating: null, corrected: null }]);
  await expect(row(page, 1).getByRole("button", { name: "Good dictation" })).toHaveAttribute("aria-pressed", "true");
});

test("a failed rating reverts and says why", async ({ page }) => {
  await history(page, `feedback_set: () => { throw "Couldn't save: disk full"; },`);
  await row(page, 0).getByRole("button", { name: "Good dictation" }).click();
  await expect(page.getByText("Couldn't save: disk full")).toBeVisible();
  await expect(row(page, 0).getByRole("button", { name: "Good dictation" })).toHaveAttribute("aria-pressed", "false");
});

test("👎 opens the box with the pasted text; Save sends what was meant and shows a new fix", async ({ page }) => {
  await history(page, `feedback_set: ({ corrected }) => corrected ? [${FIX}] : [], fix_add: () => [],`);
  await row(page, 0).getByRole("button", { name: "Bad dictation" }).click();
  const box = page.getByLabel("What did you mean?");
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("Mina is here");
  await box.fill("Myna is here");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Add “Mina → Myna” to Words to remember")).toBeVisible();
  await expect(page.getByText("Came up in 2 dictations")).toBeVisible();
  expect(await calls(page, "feedback_set")).toEqual([{ trace: 4, rating: "down", corrected: null }, { trace: 4, rating: "down", corrected: "Myna is here" }]);
  await page.getByRole("button", { name: "Add", exact: true }).click();
  expect(await calls(page, "fix_add")).toEqual([{ key: "a:mina:myna" }]);
  await expect(page.getByText("Add “Mina → Myna”")).toHaveCount(0);
});

test("saving the text unchanged sends no correction and says thanks", async ({ page }) => {
  await history(page);
  await row(page, 0).getByRole("button", { name: "Bad dictation" }).click();
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Thanks. Myna will suggest a fix if this happens again.")).toBeVisible();
  expect((await calls(page, "feedback_set")).at(-1)).toEqual({ trace: 4, rating: "down", corrected: null });
});

test("Skip and Esc close the box and put focus back on 👎", async ({ page }) => {
  await history(page);
  const down = row(page, 0).getByRole("button", { name: "Bad dictation" });
  await down.click();
  await page.getByRole("button", { name: "Skip" }).click();
  await expect(page.getByLabel("What did you mean?")).toHaveCount(0);
  await expect(down).toBeFocused();
  await down.click(); // pressed: clears
  await down.click();
  await expect(page.getByLabel("What did you mean?")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("What did you mean?")).toHaveCount(0);
  await expect(down).toBeFocused();
});

test("the opened trace in Stats for nerds has the same 👍/👎", async ({ page }) => {
  const trace = `({ id: 4, started: Date.now(), mode: "hold", app: "Slack", model: "Parakeet", audio_ms: 1000, outcome: "pasted", rating: null, reason: null, load_ms: 0, paste_ms: 100,
    steps: [{ kind: "speech-to-text", text: "Mina is here", ms: 5, settings: null, edits: null }, { kind: "rules", text: "Mina is here", ms: 1, settings: { language: "en", tidy: true }, edits: [] }] })`;
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", traces_list: () => [{ id: 4, started: Date.now(), app: "Slack", outcome: "pasted", rating: null, heard: "Mina is here", text: "Mina is here" }], traces_all: () => [${trace}], trace_get: () => ${trace}, look_get: () => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: true, nerds: true }),
    rules_get: () => ({ settings: { language: "en", tidy: true, words: [], keep: [] }, languages: [] }), fixes_get: () => [] }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: /^Voice typing history/ }).click(); // Meetings is the first view
  await expect(page.getByRole("heading", { name: "Voice typing history" })).toBeVisible();
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: "Stats for nerds" }).click();
  await page.getByRole("list", { name: "Dictations" }).getByRole("button").first().click();
  await page.getByRole("button", { name: "Bad dictation" }).click();
  await expect(page.getByLabel("What did you mean?")).toHaveValue("Mina is here");
  await page.getByRole("button", { name: "Skip" }).click();
  await expect(page.getByRole("button", { name: "Bad dictation" })).toHaveAttribute("aria-pressed", "true");
  expect(await calls(page, "feedback_set")).toEqual([{ trace: 4, rating: "down", corrected: null }]);
});

async function settings(page: Page, fixes: string, extra = "") {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], fixes_get: () => ${fixes}, ${extra} }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Voice typing" }).click();
}

test("Suggested fixes is hidden with none", async ({ page }) => {
  await settings(page, "[]");
  await expect(page.getByRole("heading", { name: "Words to remember" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Suggested fixes" })).toHaveCount(0);
});

test("Not now drops a fix; Add of a keep fix shows the word in Never remove", async ({ page }) => {
  const keep = `{ key: "k:oh", kind: "keep", from: null, to: "oh", count: 3 }`;
  const word = `{ key: "w:myna", kind: "word", from: null, to: "Myna", count: 2 }`;
  await settings(page, `[${FIX}, ${keep}, ${word}]`, `fix_dismiss: ({ key }) => [${keep}, ${word}], fix_add: ({ key }) => { window.__keep = true; return [${word}]; },
    rules_get: () => ({ settings: { language: "en", tidy: true, words: [], keep: window.__keep ? ["oh"] : [] }, languages: [["en", "English"]] }),`);
  await expect(page.getByText("Add “Mina → Myna” to Words to remember")).toBeVisible();
  await expect(page.getByText("Add “Myna” to Words to remember")).toBeVisible();
  await page.getByRole("button", { name: "Not now" }).first().click();
  await expect(page.getByText("Add “Mina → Myna”")).toHaveCount(0);
  await page.getByRole("button", { name: "Add", exact: true }).first().click();
  await expect(page.getByText("Never remove “oh”")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Remove oh from Never remove" })).toBeVisible();
  expect(await calls(page, "fix_dismiss")).toEqual([{ key: "a:mina:myna" }]);
  expect(await calls(page, "fix_add")).toEqual([{ key: "k:oh" }]);
});

test("Never remove adds lowercase words without duplicates and removes them, saving keep", async ({ page }) => {
  await settings(page, "[]");
  const field = page.getByLabel("Add words for Myna to never remove");
  await field.fill("Oh, the, OH");
  await field.press("Enter");
  await expect(page.getByRole("button", { name: "Remove oh from Never remove" })).toBeVisible();
  await page.getByRole("button", { name: "Remove oh from Never remove" }).click();
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "rules_set").map((c: any) => c[1].rules.keep));
  expect(saved).toEqual([["oh", "the"], ["the"]]);
});

test("Export shows Saved to Downloads, or the error", async ({ page }) => {
  await settings(page, "[]", `feedback_export: () => "/Users/me/Downloads/myna-feedback.json",`);
  await page.getByRole("button", { name: "Export…" }).click();
  await expect(page.getByText("Saved to Downloads")).toBeVisible();
});

test("a failed export says why", async ({ page }) => {
  await settings(page, "[]", `feedback_export: () => { throw "Couldn't write the file"; },`);
  await page.getByRole("button", { name: "Export…" }).click();
  await expect(page.getByText("Couldn't write the file")).toBeVisible();
});

// `__fix`: the dictation Fix it left with the backend (`fix_take` hands it over once).
const TAKE = `fix_take: () => { const v = window.__fix ?? null; window.__fix = null; return v; },`;

test("Fix it on the pill opens history on that dictation's box, even from another page or a search", async ({ page }) => {
  await history(page, TAKE);
  await page.getByRole("searchbox").fill("Send");
  await page.getByRole("button", { name: "Your dictation" }).click();
  await page.evaluate(() => (((window as any).__fix = 4), (window as any).__emit("fix", 4)));
  const box = page.getByLabel("What did you mean?");
  await expect(box).toBeFocused();
  await expect(box).toHaveValue("Mina is here");
  await expect(page.getByRole("heading", { name: "Voice typing history" })).toBeVisible();
});

test("Ask how it went is off until switched on, and saves with the other voice settings", async ({ page }) => {
  await settings(page, "[]", `voice_set: () => {},`);
  const ask = page.getByRole("switch", { name: "Ask how it went" });
  await expect(ask).not.toBeChecked();
  await ask.click();
  await expect(ask).toBeChecked();
  expect((await calls(page, "voice_set")).at(-1)).toMatchObject({ voice: { key: "Alt+Space", rate: true } });
});

test("a Fix it the page missed (it was reloading) still opens the box on load", async ({ page }) => {
  await page.addInitScript(() => ((window as any).__fix = 4));
  await history(page, TAKE);
  await expect(page.getByLabel("What did you mean?")).toHaveValue("Mina is here");
  expect(await calls(page, "fix_take")).not.toHaveLength(0);
});
