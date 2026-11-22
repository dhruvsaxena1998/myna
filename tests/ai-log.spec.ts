import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// The #247 contract: ai_log_list -> Summary[] (newest first), ai_log_get -> Entry | null, ai_log_clear, event "ai-log" with a Summary.
const ENTRIES = `(() => {
  const ago = (m) => Date.now() - m * 60e3;
  return [
    { id: "c", at: ago(1), feature: "Summary", step: "part 3/7", provider: "OpenRouter", kind: "endpoint", model: "openai/gpt-5-mini", method: "chat_stream",
      system: "You write meeting summaries.", prompt: "Summarise this transcript part.", text: "The team agreed to ship Friday.",
      raw: '{"choices":[{"message":{"content":"The team agreed to ship Friday."}}]}', outcome: "ok", ttft_ms: 420, total_ms: 2310, limit_s: 60,
      tokens_in: 1200, tokens_out: 85, cost: { kind: "usd", usd: 0.0004 } },
    { id: "b", at: ago(5), feature: "LLM cleanup", provider: "Claude Code", kind: "agent", model: "sonnet", method: "chat",
      system: "Clean up dictation.", prompt: "so um ship it", outcome: "timeout", error: "AI took too long", total_ms: 15000, limit_s: 15, cost: { kind: "unknown" } },
    { id: "a", at: ago(9), feature: "Decision", provider: "OpenRouter", kind: "endpoint", model: "openai/gpt-5-mini", method: "decide",
      decision: { question: "Which app?", options: ["Mail", "Slack"], instructions: "Pick one.", via: "chat", choice: "Slack", confidence: 0.8 },
      outcome: "ok", total_ms: 900, cost: { kind: "usd", usd: 0.0001 } },
  ];
})()`;
const SUMMARY = `(e) => ({ id: e.id, at: e.at, feature: e.feature, step: e.step, provider: e.provider, kind: e.kind, model: e.model, method: e.method, outcome: e.outcome, total_ms: e.total_ms, cost: e.cost, preview: (e.text ?? e.error ?? "").slice(0, 120) })`;
const look = (experimental: boolean, ailog: boolean) => ({ theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental, nerds: false, agents: false, ailog, frost: 50 });

async function open(page: Page, o: { experimental?: boolean; ailog?: boolean; empty?: boolean; theme?: string } = {}) {
  const l = { ...look(o.experimental ?? true, o.ailog ?? true), theme: o.theme ?? "auto" };
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => (${JSON.stringify(l)}), look_set: () => {},
    ai_log_list: () => ${o.empty ? "[]" : `${ENTRIES}.map(${SUMMARY})`},
    ai_log_get: ({ id }) => ${ENTRIES}.find((e) => e.id === id) ?? null,
    ai_log_clear: () => {}, text_copy: () => {},
  }`);
  await page.goto("/main");
}
const nav = (page: Page) => page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: "AI log" });
const rows = (page: Page) => page.getByRole("list", { name: "AI calls" }).getByRole("button");
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);
async function go(page: Page, o = {}) {
  await open(page, o);
  await nav(page).click();
  await expect(page.getByRole("heading", { name: "AI log", level: 1 })).toBeVisible(); // mounted: events land
}

test("the AI log switch sits in Experimental › AI, under Agents, and saves", async ({ page }) => {
  await open(page, { ailog: false });
  await page.goto("/settings");
  await page.getByRole("option", { name: "Experimental" }).click();
  const sw = page.getByRole("switch", { name: "AI log" });
  await expect(sw).not.toBeChecked();
  await expect(page.getByText("Keeps every AI request and reply on this Mac, with timings and cost, and adds a page to the main window to read them.")).toBeVisible();
  const ai = page.getByRole("region", { name: "AI" });
  await expect(ai.getByRole("switch")).toHaveCount(2);
  await expect(ai.getByRole("switch").nth(1)).toHaveAccessibleName("AI log");
  await sw.click();
  await expect(sw).toBeChecked();
  expect((await calls(page, "look_set")).at(-1).look.ailog).toBe(true);
});

test("the main window shows AI log only while its switch and Experimental features are on", async ({ page }) => {
  await open(page, { ailog: false });
  const emit = (experimental: boolean, ailog: boolean) => page.evaluate((l) => (window as any).__emit("look", l), look(experimental, ailog));
  await expect(page.getByRole("heading", { name: "All meetings" })).toBeVisible();
  await expect(nav(page)).toHaveCount(0);
  await emit(true, true);
  await nav(page).click();
  await expect(page.getByRole("heading", { name: "AI log", level: 1 })).toBeVisible();
  await emit(true, false);
  await expect(nav(page)).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "AI log", level: 1 })).toHaveCount(0); // not a blank page
  await emit(false, true);
  await expect(nav(page)).toHaveCount(0);
});

test("lists calls newest first: time, feature and step, provider · model, total time and outcome", async ({ page }) => {
  await go(page);
  await expect(rows(page)).toHaveCount(3);
  await expect(rows(page).nth(0)).toContainText("Summary · part 3/7");
  await expect(rows(page).nth(0)).toContainText("OpenRouter · openai/gpt-5-mini");
  await expect(rows(page).nth(0)).toContainText("Today");
  await expect(rows(page).nth(0)).toContainText("2.31 s");
  await expect(rows(page).nth(0)).toContainText("OK");
  await expect(rows(page).nth(1)).toContainText("LLM cleanup");
  await expect(rows(page).nth(1)).toContainText("Timed out");
  await expect(rows(page).nth(2)).toContainText("Decision");
  // A failed call stands out in the error colour; an OK one doesn't.
  const color = (n: number) => rows(page).nth(n).locator(".badge").evaluate((e) => getComputedStyle(e).color);
  const danger = await page.evaluate(() => { const i = document.createElement("i"); i.style.color = "var(--danger-text)"; document.body.append(i); return getComputedStyle(i).color; });
  expect(await color(1)).toBe(danger);
  expect(await color(0)).not.toBe(danger);
});

test("an empty AI log says so", async ({ page }) => {
  await go(page, { empty: true });
  await expect(page.getByRole("heading", { name: "No AI calls yet" })).toBeVisible(); // DESIGN.md: a heading, one sentence
  await expect(page.getByText("Use a feature that calls an AI provider and it shows up here.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Clear all" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Errors only" })).toHaveCount(0);
  await page.getByRole("button", { name: "Open AI providers" }).click();
  expect(await calls(page, "settings_open")).toEqual([{ page: "ai" }]);
});

test("opening a call shows its details and every section it has, the reason it failed first", async ({ page }) => {
  await go(page);
  await rows(page).nth(0).click();
  expect(await calls(page, "ai_log_get")).toEqual([{ id: "c" }]);
  await expect(page.getByRole("heading", { name: "Summary · part 3/7", level: 1 })).toBeVisible(); // the toolbar names the call
  await expect(page.locator(".content .meta").first()).toContainText("Streamed chat"); // not "chat_stream"
  const details = page.getByRole("region", { name: "Details" });
  for (const t of ["OpenRouter", "Endpoint", "openai/gpt-5-mini", "OK", "420 ms", "2.31 s", "60 s", "1,200 in · 85 out", "$0.00040"]) await expect(details).toContainText(t);
  for (const s of ["System prompt", "Prompt", "Text"]) await expect(page.getByRole("region", { name: s, exact: true })).toBeVisible();
  await expect(page.getByRole("region", { name: "Error", exact: true })).toHaveCount(0); // only what the call has
  await expect(page.getByRole("region", { name: /Raw reply/ })).toHaveCount(0); // the raw reply isn't shown
  await page.getByRole("main").getByRole("button", { name: "AI log" }).click();
  await rows(page).nth(1).click();
  await expect(page.getByRole("region", { name: "Details" })).toContainText("On your plan");
  await expect(page.locator("section.card").nth(1)).toHaveAccessibleName("Error"); // straight under Details
  await expect(page.getByRole("region", { name: "Error", exact: true })).toContainText("AI took too long");
  await page.getByRole("main").getByRole("button", { name: "AI log" }).click();
  await rows(page).nth(2).click();
  const decision = page.getByRole("region", { name: "Decision details" });
  for (const t of ["Which app?", "Mail, Slack", "Pick one.", "chat", "Slack (80% sure)"]) await expect(decision).toContainText(t);
});

test("a long section scrolls inside itself", async ({ page }) => {
  await go(page);
  await rows(page).nth(0).click(); // the fixture's prompt is short: check the cap on the box
  const pre = page.getByRole("region", { name: "Prompt", exact: true }).locator("pre");
  expect(await pre.evaluate((e) => [getComputedStyle(e).overflowY, getComputedStyle(e).maxHeight])).toEqual(["auto", "320px"]);
});

test("Copy puts the section's text on the clipboard", async ({ page }) => {
  await go(page);
  await rows(page).nth(0).click();
  await page.getByRole("button", { name: "Copy system prompt" }).click();
  await expect(page.getByRole("region", { name: "System prompt" }).getByRole("button")).toHaveText("Copied");
  await page.getByRole("button", { name: "Copy prompt" }).click();
  expect(await calls(page, "text_copy")).toEqual([{ text: "You write meeting summaries." }, { text: "Summarise this transcript part." }]);
});

test("Errors only narrows the list", async ({ page }) => {
  await go(page);
  await page.getByRole("button", { name: "Errors only" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).nth(0)).toContainText("LLM cleanup");
  await page.getByRole("button", { name: "Errors only" }).click();
  await expect(rows(page)).toHaveCount(3);
});

test("a meeting's or a dictation's calls are one row, named by what it was; opening it lists them in the order they ran", async ({ page }) => {
  const at = Date.now() - 60e3;
  const entries = [
    { id: "m4", at: at + 9000, feature: "Summary", entity: "meeting:7", method: "chat_stream", outcome: "error", error: "boom", total_ms: 1000, provider: "OpenRouter", model: "gpt" },
    { id: "m3", at: at + 5000, feature: "Title", entity: "meeting:7", method: "chat", outcome: "ok", text: "Weekly standup", total_ms: 1000, provider: "OpenRouter", model: "gpt" },
    { id: "m2", at: at + 500, feature: "Meeting speech-to-text", step: "chunk 2/2", entity: "meeting:7", method: "transcribe", outcome: "ok", total_ms: 2000, provider: "Groq", model: "whisper" },
    { id: "m1", at, feature: "Meeting speech-to-text", step: "chunk 1/2", entity: "meeting:7", method: "transcribe", outcome: "ok", total_ms: 2000, provider: "Groq", model: "whisper" },
    { id: "d1", at: at - 60e3, feature: "LLM cleanup", entity: "dictation:5", method: "chat", outcome: "ok", text: "Ship it on Friday.", total_ms: 50, provider: "OpenRouter", model: "gpt" },
  ];
  const all = JSON.stringify(entries);
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => (${JSON.stringify(look(true, true))}), look_set: () => {},
    ai_log_list: () => ${all}.map((e) => ({ ...(${SUMMARY})(e), entity: e.entity })),
    ai_log_get: ({ id }) => ${all}.find((e) => e.id === id) ?? null, text_copy: () => {},
  }`);
  await page.goto("/main");
  await nav(page).click();
  await expect(rows(page)).toHaveCount(2);
  const meeting = rows(page).nth(0);
  await expect(meeting.locator(".name")).toHaveText("Weekly standup"); // its Title's answer
  await expect(meeting.locator(".meta")).toHaveText(/^Meeting · Today \d\d:\d\d · 4 calls$/);
  await expect(meeting.locator(".nums")).toHaveText("4.50 s"); // the two chunks overlap: 2.5 s, then 1 s and 1 s
  await expect(meeting).toContainText("1 failed");
  await expect(rows(page).nth(1).locator(".name")).toHaveText("Ship it on Friday.");
  await expect(rows(page).nth(1).locator(".meta")).toContainText("Dictation");

  await meeting.click();
  await expect(page.getByRole("heading", { name: "Weekly standup", level: 1 })).toBeVisible();
  await expect(page.locator(".sub")).toHaveText("Meeting · 4 calls · 4.50 s · OpenRouter · gpt"); // the usual model, once
  await expect(rows(page).locator(".name")).toHaveText(["Meeting speech-to-text · chunk 1/2", "Meeting speech-to-text · chunk 2/2", "Title", "Summary"]);
  await expect(rows(page).nth(0).locator(".meta")).toContainText("Groq · whisper"); // only where it differs
  await expect(rows(page).nth(2).locator(".meta")).not.toContainText("gpt");
  await page.getByRole("button", { name: "Errors only" }).click(); // works inside a meeting too
  await expect(rows(page)).toHaveCount(1);
  await rows(page).nth(0).click();
  await expect(page.getByRole("heading", { name: "Summary", level: 1 })).toBeVisible();
  await page.getByRole("button", { name: "Weekly standup" }).click(); // the breadcrumb goes back to the meeting
  await expect(rows(page)).toHaveCount(1);
  await page.getByRole("button", { name: "Errors only" }).click();
  await expect(rows(page)).toHaveCount(4);
  await page.getByRole("main").getByRole("button", { name: "AI log" }).click();
  await expect(rows(page)).toHaveCount(2);
});

test("each view opens at its top", async ({ page }) => {
  const entries = Array.from({ length: 30 }, (_, i) => ({ id: "t" + i, at: Date.now() - i * 60e3, feature: "Test", method: "check", outcome: "ok", total_ms: 10 }));
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => (${JSON.stringify(look(true, true))}), look_set: () => {},
    ai_log_list: () => ${JSON.stringify(entries)}, ai_log_get: ({ id }) => ${JSON.stringify(entries)}.find((e) => e.id === id),
  }`);
  await page.goto("/main");
  await nav(page).click();
  await rows(page).last().click();
  await expect(page.getByRole("region", { name: "Details" })).toBeVisible();
  expect(await page.locator(".content").evaluate((e) => e.scrollTop)).toBe(0);
});

test("Clear all asks first, then empties the AI log", async ({ page }) => {
  await go(page);
  await page.getByRole("button", { name: "Clear all" }).click();
  await expect(page.getByText("Delete every call in the AI log? This can’t be undone.")).toBeVisible();
  expect(await calls(page, "ai_log_clear")).toEqual([]);
  await page.getByRole("button", { name: "Delete all" }).click();
  await expect(page.getByRole("heading", { name: "No AI calls yet" })).toBeVisible();
  expect((await calls(page, "ai_log_clear")).length).toBe(1);
});

test("a new call shows up at the top as it lands", async ({ page }) => {
  await go(page);
  await expect(rows(page)).toHaveCount(3);
  await page.evaluate(() => (window as any).__emit("ai-log", { id: "d", at: Date.now(), feature: "Title", provider: "Ollama", kind: "endpoint", model: "qwen2.5:7b", method: "chat", outcome: "error", total_ms: 80, preview: "connection refused" }));
  await expect(rows(page)).toHaveCount(4);
  await expect(rows(page).nth(0)).toContainText("Title");
  await expect(rows(page).nth(0)).toContainText("Ollama · qwen2.5:7b");
  await expect(rows(page).nth(0)).toContainText("Error");
});

test("a call that lands before the list loads is kept once, at the top", async ({ page }) => {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => (${JSON.stringify(look(true, true))}), look_set: () => {},
    ai_log_list: () => new Promise((r) => (window.__list = () => r(${ENTRIES}.map(${SUMMARY})))),
  }`);
  await page.goto("/main");
  await nav(page).click();
  await expect.poll(() => calls(page, "plugin:event|listen").then((l) => l.some((a: any) => a.event === "ai-log"))).toBe(true);
  await page.evaluate(() => {
    const at = Date.now();
    (window as any).__emit("ai-log", { id: "c", at, feature: "Summary", step: "part 3/7", method: "chat_stream", outcome: "ok" }); // also in the list
    (window as any).__emit("ai-log", { id: "d", at, feature: "Title", provider: "Ollama", model: "qwen2.5:7b", method: "chat", outcome: "error" });
  });
  await expect.poll(() => page.evaluate(() => typeof (window as any).__list)).toBe("function");
  await page.evaluate(() => (window as any).__list());
  await expect(rows(page)).toHaveCount(4);
  await expect(rows(page).nth(0)).toContainText("Title");
  await expect(rows(page).nth(1)).toContainText("Summary · part 3/7");
});

for (const theme of ["light", "dark"]) {
  test(`the AI log renders in ${theme}`, async ({ page }) => {
    await go(page, { theme });
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await expect(rows(page)).toHaveCount(3);
    await page.screenshot({ path: `test-results/ai-log-list-${theme}.png` });
    await rows(page).nth(1).click();
    await expect(page.getByRole("region", { name: "Error", exact: true })).toBeVisible();
    await page.screenshot({ path: `test-results/ai-log-detail-${theme}.png`, fullPage: true });
  });
}
