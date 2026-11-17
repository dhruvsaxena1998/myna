import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Meetings on this fake day: two today, one yesterday, one three days ago, and one whose notes failed.
const MEETINGS = `(() => {
  const at = (days, h, m) => { const d = new Date(); d.setDate(d.getDate() - days); d.setHours(h, m, 0, 0); return d.getTime(); };
  return [
    { id: 5, title: "Untitled meeting", started: at(0, 11, 0), duration_ms: 42 * 60000, source_app: "Google Meet", state: "done", failure: null, note: null },
    { id: 4, title: "Untitled meeting", started: at(0, 9, 30), duration_ms: 5 * 60000, source_app: null, state: "failed", failure: "The model fell over.", note: null },
    { id: 3, title: "Untitled meeting", started: at(1, 15, 30), duration_ms: 35 * 60000, source_app: "Microsoft Teams", state: "done", failure: null, note: null },
    { id: 2, title: "Untitled meeting", started: at(3, 10, 0), duration_ms: 28 * 60000, source_app: "Google Meet", state: "done", failure: null, note: null },
  ];
})()`;
const line = (id: number, speaker: string, start_ms: number, text: string, flags = {}) => ({ id, speaker, start_ms, text, muted: false, included: false, hidden: false, ...flags });
const SEGMENTS = JSON.stringify([line(1, "Me", 12000, "Okay, let's start."), line(2, "Them", 75000, "Launch is still the 18th.")]);

async function open(page: Page, extra = "", meetings = MEETINGS, on = true) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", meetings_list: () => ${meetings}, meeting_get: () => ({ on: ${on}, key: "Ctrl+Alt+KeyM" }),
    meeting_page: ({ id }) => ({ ...${MEETINGS}.find((m) => m.id === id), segments: id === 5 ? ${SEGMENTS} : [] }),
    text_copy: () => {}, meeting_text: () => "Me 00:12\\nOkay, let's start.\\n\\nThem 01:15\\nLaunch is still the 18th.", meeting_toggle: () => {}, meeting_retry: () => {}, meeting_set: () => {}, summary_get: () => ({ versions: [], showing: null }), ${extra}
  }`);
  await page.goto("/main");
  await expect(page.getByRole("heading", { name: "All meetings" })).toBeVisible();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);
const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);
const nav = (page: Page) => page.getByRole("navigation", { name: "Sidebar" });

test("Meetings is the first view, with a count, and lists meetings by day", async ({ page }) => {
  await open(page);
  await expect(nav(page).getByRole("button").first()).toHaveText("All meetings4");
  await expect(nav(page).getByRole("button").first()).toHaveAttribute("aria-current", "true");
  const today = page.getByRole("region", { name: /^Today/ });
  await expect(today.getByRole("listitem")).toHaveCount(2);
  await expect(today.getByRole("listitem").first()).toContainText("11:00 · 42 min · Google Meet");
  await expect(page.getByRole("region", { name: /^Yesterday/ }).getByRole("listitem")).toContainText("15:30 · 35 min · Microsoft Teams");
  await expect(page.getByRole("button", { name: "Start notes" })).toBeEnabled();
});

test("a failed meeting says why and Try again retries it", async ({ page }) => {
  await open(page);
  const row = page.getByRole("listitem").filter({ hasText: "The model fell over." });
  await row.getByRole("button", { name: "Try again" }).click();
  expect(await calls(page, "meeting_retry")).toEqual([{ id: 4 }]);
});

test("Start notes starts, the live card shows the recording, and Stop ends it", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Start notes" }).click();
  expect(await calls(page, "meeting_toggle")).toHaveLength(1);
  await emit(page, "meeting-running", { id: 6, started: Date.now() - 65_000, source: "Google Meet", mic: null, words: "Mic live" });
  const card = page.getByRole("region", { name: "Meeting recording in progress" });
  await expect(card).toContainText("Recording · Google Meet");
  await expect(card).toContainText("Mic live");
  await expect(card.getByText(/^01:0\d$/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Start notes" })).toBeHidden();
  await card.getByRole("button", { name: "Stop" }).click();
  expect(await calls(page, "meeting_toggle")).toHaveLength(2);
});

test("after Stop the card says it is writing up the notes, then the meeting joins the list in place", async ({ page }) => {
  let fresh = `{ id: 6, title: "Untitled meeting", started: Date.now(), duration_ms: 60000, source_app: null, state: "recorded", failure: null, note: null }`;
  await open(page, `meetings_list: () => ${MEETINGS}.concat(window.__fresh ? [window.__fresh] : []).sort((a, b) => b.started - a.started)`);
  await page.evaluate((f) => ((window as any).__fresh = new Function(`return (${f})`)()), fresh);
  await emit(page, "meeting-notes", { id: 6, state: "transcribing", progress: 0, error: null });
  const card = page.getByRole("status").filter({ hasText: "Writing up your notes…" });
  await expect(card).toBeVisible();
  await expect(page.getByRole("button", { name: "Start notes" })).toBeDisabled();
  await emit(page, "meeting-notes", { id: 6, state: "transcribing", progress: 0.4, error: null });
  await expect(card).toContainText("40%");
  const before = await page.getByRole("region", { name: /^Yesterday/ }).boundingBox();
  await page.evaluate(() => ((window as any).__fresh.state = "done"));
  await emit(page, "meeting-notes", { id: 6, state: "done", progress: 1, error: null });
  await expect(card).toBeHidden();
  await expect(page.getByRole("region", { name: /^Today/ }).getByRole("listitem")).toHaveCount(3);
  // The card is replaced by a row of the same height: what's below doesn't move.
  const after = await page.getByRole("region", { name: /^Yesterday/ }).boundingBox();
  expect(Math.abs(after!.y - before!.y)).toBeLessThanOrEqual(1);
});

test("meeting notes off: says so and Turn on meeting notes turns them on", async ({ page }) => {
  await open(page, "", "[]", false);
  await expect(page.getByRole("heading", { name: "Meeting notes are off" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start notes" })).toBeHidden();
  await page.getByRole("button", { name: "Turn on meeting notes" }).click();
  expect(await calls(page, "meeting_set")).toEqual([{ notes: { on: true, key: "Ctrl+Alt+KeyM" } }]);
  await expect(page.getByRole("heading", { name: "No meetings yet" })).toBeVisible();
});

test("nothing recorded yet", async ({ page }) => {
  await open(page, "", "[]");
  await expect(page.getByRole("heading", { name: "No meetings yet" })).toBeVisible();
  await expect(nav(page).getByRole("button").first()).toHaveText("All meetings0");
});

test("a meeting opens as a page with its transcript, Copy, and a way back", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
  await expect(page.getByRole("heading", { name: "Untitled meeting", level: 1 })).toBeVisible();
  await expect(page.getByText("11:00 · 42 min · Google Meet")).toBeVisible();
  await expect(page.getByText("Okay, let's start.")).toBeVisible();
  await expect(page.locator(".seg").nth(1)).toContainText("Them01:15");
  await page.getByRole("button", { name: "Copy" }).click();
  await expect(page.getByRole("button", { name: "Copied" })).toBeVisible();
  expect(await calls(page, "text_copy")).toEqual([{ text: "Me 00:12\nOkay, let's start.\n\nThem 01:15\nLaunch is still the 18th." }]);
  await page.getByRole("button", { name: "Back to meetings" }).click();
  await expect(page.getByRole("heading", { name: "All meetings" })).toBeVisible();
});

// Two muted lines in a row, one said and kept, and a line between that was never muted.
const MUTED = JSON.stringify([
  line(1, "Me", 12000, "Okay, let's start."),
  line(2, "Me", 20000, "", { muted: true, hidden: true }),
  line(3, "Me", 25000, "", { muted: true, hidden: true }),
  line(4, "Them", 30000, "Go on."),
  line(5, "Me", 40000, "I said this muted but keep it.", { muted: true, included: true }),
]);

test("lines said while muted collapse into one row, Include keeps them with a marker, and Copy asks the backend", async ({ page }) => {
  await open(page, `meeting_page: () => ({ ...${MEETINGS}[0], segments: window.__lines ?? ${MUTED} }),
    segments_include: ({ ids }) => { window.__lines = ${MUTED}.map((l) => ids.includes(l.id) ? { ...l, hidden: false, included: true, text: "Said muted, kept." } : l); }`);
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
  const row = page.locator(".seg").filter({ hasText: "Said while muted · left out" });
  await expect(row).toHaveCount(1);
  await expect(row).toContainText("Me00:20");
  await expect(page.locator(".seg")).toHaveCount(4);
  await expect(page.locator(".seg").filter({ hasText: "I said this muted but keep it." })).toContainText("Said while muted");
  await row.getByRole("button", { name: "Include" }).click();
  expect(await calls(page, "segments_include")).toEqual([{ ids: [2, 3] }]);
  await expect(page.getByText("Said muted, kept.")).toHaveCount(2);
  await expect(page.getByText("Said while muted · left out")).toBeHidden();
  await page.getByRole("button", { name: "Copy" }).click();
  expect(await calls(page, "meeting_text")).toEqual([{ id: 5 }]);
});

test("the live card says how the mic is while the add-on mirrors it", async ({ page }) => {
  await open(page);
  await emit(page, "meeting-running", { id: 6, started: Date.now(), source: "Microsoft Teams", mic: "live", words: "Mic live · mirrored from Teams" });
  const card = page.getByRole("region", { name: "Meeting recording in progress" });
  await expect(card).toContainText("Mic live · mirrored from Teams");
  await emit(page, "meeting-running", { id: 6, started: Date.now(), source: "Microsoft Teams", mic: "muted", words: "Muted · mirrored from Teams" });
  await expect(card).toContainText("Muted · mirrored from Teams");
});

test("meeting-open, from the pill or the menubar, goes to that meeting from any view", async ({ page }) => {
  await open(page);
  await nav(page).getByRole("button", { name: "Your dictation" }).click();
  await emit(page, "meeting-open", 5);
  await expect(page.getByText("Launch is still the 18th.")).toBeVisible();
  expect(await calls(page, "meeting_page")).toEqual([{ id: 5 }]);
});

test("the other side's lines show their speaker names, and a note says why voices weren't told apart", async ({ page }) => {
  const named = JSON.stringify([line(1, "Me", 12000, "Okay, let's start."), line(2, "Ana Ruiz", 30000, "Go on."), line(3, "Speaker 3", 40000, "Launch is the 18th.")]);
  await open(page, `meeting_page: () => ({ ...${MEETINGS}[0], note: window.__note ?? null, segments: ${named} })`);
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
  await expect(page.locator(".seg").nth(1)).toContainText("Ana Ruiz00:30");
  await expect(page.locator(".seg").nth(2)).toContainText("Speaker 300:40");
  await expect(page.locator(".note")).toHaveCount(0);
  await page.evaluate(() => ((window as any).__note = "Couldn't tell voices apart: Download Sortformer in Settings › Speech models"));
  await page.getByRole("button", { name: "Back to meetings" }).click();
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
  await expect(page.locator(".note")).toHaveText("Couldn't tell voices apart: Download Sortformer in Settings › Speech models");
});

// Story 16: the Summary is the document; the transcript opens beside it.
const version = (id: number, template: string, text: string, more = {}) => ({ id, template, prompt: "p", provider: "OpenRouter", model: "big", created: Date.now(), parts: 2, cost: { kind: "usd", usd: 0.012 }, text, edited: false, ...more });
const AI = JSON.stringify({ providers: [{ id: 1, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true }], main: { provider: 1, model: "big", gone: false }, fast: null });
async function summary(page: Page, versions: unknown[], extra = "") {
  const sum = JSON.stringify({ versions, showing: (versions[0] as { id: number } | undefined)?.id ?? null });
  await open(page, `ai_get: () => (${AI}), summary_get: () => (window.__sum ??= ${sum}), settings_open: () => {}, ai_set: () => {},
    summary_templates_get: () => [{ id: "recap", name: "Recap", text: "Write a Recap." }, { id: "actions", name: "Action items", text: "List them." }],
    ai_models: () => [{ id: "big", input_per_m: 3, output_per_m: 15, context: null, images: null }, { id: "small", input_per_m: null, output_per_m: null, context: null, images: null }],
    summary_estimate: ({ model }) => (model === "big" ? 1.4 : null), summary_run: () => {},
    summary_pick: ({ version }) => { window.__sum.showing = version; },
    summary_delete: ({ version }) => { window.__sum = { versions: window.__sum.versions.filter((v) => v.id !== version), showing: null }; window.__sum.showing = window.__sum.versions[0].id; },
    summary_edit: ({ text }) => { const v = { ...window.__sum.versions[0], id: 99, text, edited: true, cost: { kind: "free" } }; window.__sum = { versions: [v, ...window.__sum.versions], showing: 99 }; },
    ${extra}`);
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
}

test("a Summary is the document: Markdown with links as text, a cost line, its timestamps open the transcript at that line, and Copy copies it", async ({ page }) => {
  const text = "## Decisions\n- Launch stays on the 18th [00:01:10]\n- See [the plan](https://x.com) <b>now</b>";
  await summary(page, [version(9, "Recap", text)]);
  await expect(page.getByRole("heading", { name: "Decisions", level: 2 })).toBeVisible();
  await expect(page.getByText("See [the plan](https://x.com) <b>now</b>")).toBeVisible();
  await expect(page.locator(".sum a, .sum b")).toHaveCount(0);
  await expect(page.locator(".sum-cost")).toHaveText("$0.012 · Recap · big · Summarized in 2 parts");
  await expect(page.locator(".seg")).toHaveCount(0);
  await page.getByRole("button", { name: "Show 00:01:10 in the transcript" }).click();
  await expect(page.locator(".seg.hit")).toContainText("Launch is still the 18th.");
  await page.getByRole("button", { name: "Copy" }).first().click();
  expect(await calls(page, "text_copy")).toEqual([{ text }]);
  await page.getByRole("button", { name: "Close transcript" }).click();
  await expect(page.locator(".seg")).toHaveCount(0);
});

test("versions: pick one, delete never the last, and Edit › Save adds an Edited version", async ({ page }) => {
  await summary(page, [version(9, "Action items", "# Actions"), version(8, "Recap", "# Recap")]);
  await page.getByRole("button", { name: "2 versions" }).click();
  await page.getByRole("menuitemradio", { name: /Recap · big/ }).click();
  expect(await calls(page, "summary_pick")).toEqual([{ id: 5, version: 8 }]);
  await expect(page.getByRole("heading", { name: "Recap", level: 1 })).toBeVisible();
  await page.getByRole("button", { name: "2 versions" }).click();
  await page.getByRole("button", { name: /Delete the Action items version/ }).click();
  expect(await calls(page, "summary_delete")).toEqual([{ id: 5, version: 9 }]);
  await expect(page.getByRole("button", { name: "2 versions" })).toBeHidden();
  await page.getByRole("button", { name: "Edit" }).click();
  const box = page.getByRole("textbox", { name: "Summary text, in Markdown" });
  await expect(box).toHaveValue("# Recap");
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(box).toBeHidden();
  await page.getByRole("button", { name: "Edit" }).click();
  await box.fill("# My recap");
  await page.getByRole("button", { name: "Save" }).click();
  expect(await calls(page, "summary_edit")).toEqual([{ id: 5, text: "# My recap" }]);
  await expect(page.getByRole("heading", { name: "My recap", level: 1 })).toBeVisible();
  await expect(page.locator(".sum-cost")).toHaveText("Edited · Recap · big · Summarized in 2 parts");
  await page.getByRole("button", { name: "2 versions" }).click();
  await expect(page.getByRole("menuitemradio", { name: /Edited/ })).toHaveCount(1);
});

test("Regenerate: template, prompt and model for this run only, the estimate above $1, then progress, failure and Try again with the same ask", async ({ page }) => {
  await summary(page, [version(9, "Recap", "# Recap")]);
  await page.getByRole("button", { name: "Regenerate" }).click();
  const pop = page.getByRole("dialog", { name: "Regenerate the summary" });
  await expect(pop.getByRole("combobox", { name: "Template" })).toHaveValue("recap");
  await expect(pop.getByRole("textbox", { name: "Instructions" })).toHaveValue("Write a Recap.");
  await expect(pop.getByRole("combobox", { name: "Model" })).toHaveValue("1|big");
  await expect(pop.getByText("About $1.40 for this meeting.")).toBeVisible();
  await expect(pop.getByText("Sends text off this Mac")).toBeVisible();
  await pop.getByRole("combobox", { name: "Template" }).selectOption("actions");
  await expect(pop.getByRole("textbox", { name: "Instructions" })).toHaveValue("List them.");
  await pop.getByRole("textbox", { name: "Instructions" }).fill("Only Priya's items.");
  await pop.getByRole("combobox", { name: "Model" }).selectOption("1|small");
  await expect(pop.getByText(/About \$/)).toBeHidden();
  await pop.getByRole("button", { name: "Regenerate" }).click();
  const ask = { id: 5, template: "Action items", prompt: "Only Priya's items.", provider: 1, model: "small" };
  expect(await calls(page, "summary_run")).toEqual([ask]);
  expect(await calls(page, "ai_set")).toEqual([]);
  await emit(page, "meeting-summary", { id: 5, state: "reading", n: 2, of: 6 });
  await expect(page.getByText("Reading part 2 of 6…")).toBeVisible();
  await expect(page.getByRole("button", { name: "Regenerate" })).toBeDisabled();
  await emit(page, "meeting-summary", { id: 5, state: "merging" });
  await expect(page.getByText("Writing the Action items…")).toBeVisible();
  await emit(page, "meeting-summary", { id: 5, state: "failed", error: "The model is down." });
  await expect(page.getByText("Couldn’t summarize: The model is down.")).toBeVisible();
  await page.getByRole("button", { name: "Try again" }).click();
  expect(await calls(page, "summary_run")).toEqual([ask, ask]);
  const before = (await calls(page, "summary_get")).length;
  await emit(page, "meeting-summary", { id: 5, state: "done", version: version(10, "Action items", "# Actions") });
  await expect.poll(async () => (await calls(page, "summary_get")).length).toBe(before + 1);
});

test("no Summary yet: the transcript shows, Main Not set offers AI providers, and with Main set Summarize opens the pop-up", async ({ page }) => {
  await summary(page, [], `ai_get: () => (window.__ai ?? { providers: [], main: null, fast: null }),`);
  await expect(page.locator(".seg")).toHaveCount(2);
  await expect(page.getByText("Set a Main model to summarize")).toBeVisible();
  await page.getByRole("button", { name: "Set up" }).click();
  expect(await calls(page, "settings_open")).toEqual([{ page: "ai" }]);
  await page.evaluate((ai) => ((window as any).__ai = JSON.parse(ai)), AI);
  await page.getByRole("button", { name: "Back to meetings" }).click();
  await page.getByRole("button", { name: "Untitled meeting" }).first().click();
  await expect(page.getByText("No summary yet")).toBeVisible();
  await page.getByRole("button", { name: "Summarize" }).click();
  const pop = page.getByRole("dialog", { name: "Summarize this meeting" });
  await pop.getByRole("button", { name: "Summarize" }).click();
  expect(await calls(page, "summary_run")).toEqual([{ id: 5, template: "Recap", prompt: "Write a Recap.", provider: 1, model: "big" }]);
});
