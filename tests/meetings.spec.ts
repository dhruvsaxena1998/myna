import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Meetings on this fake day: two today, one yesterday, one three days ago, and one whose notes failed.
const MEETINGS = `(() => {
  const at = (days, h, m) => { const d = new Date(); d.setDate(d.getDate() - days); d.setHours(h, m, 0, 0); return d.getTime(); };
  return [
    { id: 5, title: "Untitled meeting", started: at(0, 11, 0), duration_ms: 42 * 60000, source_app: "Google Meet", state: "done", failure: null },
    { id: 4, title: "Untitled meeting", started: at(0, 9, 30), duration_ms: 5 * 60000, source_app: null, state: "failed", failure: "The model fell over." },
    { id: 3, title: "Untitled meeting", started: at(1, 15, 30), duration_ms: 35 * 60000, source_app: "Microsoft Teams", state: "done", failure: null },
    { id: 2, title: "Untitled meeting", started: at(3, 10, 0), duration_ms: 28 * 60000, source_app: "Google Meet", state: "done", failure: null },
  ];
})()`;
const SEGMENTS = `[{ speaker: "Me", start_ms: 12000, text: "Okay, let's start." }, { speaker: "Them", start_ms: 75000, text: "Launch is still the 18th." }]`;

async function open(page: Page, extra = "", meetings = MEETINGS, on = true) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", meetings_list: () => ${meetings}, meeting_get: () => ({ on: ${on}, key: "Ctrl+Alt+KeyM" }),
    meeting_page: ({ id }) => ({ ...${MEETINGS}.find((m) => m.id === id), segments: id === 5 ? ${SEGMENTS} : [] }),
    text_copy: () => {}, meeting_toggle: () => {}, meeting_retry: () => {}, meeting_set: () => {}, ${extra}
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
  await emit(page, "meeting-running", { id: 6, started: Date.now() - 65_000, source: "Google Meet" });
  const card = page.getByRole("region", { name: "Meeting recording in progress" });
  await expect(card).toContainText("Recording · Google Meet");
  await expect(card).toContainText("Mic live");
  await expect(card.getByText(/^01:0\d$/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Start notes" })).toBeHidden();
  await card.getByRole("button", { name: "Stop" }).click();
  expect(await calls(page, "meeting_toggle")).toHaveLength(2);
});

test("after Stop the card says it is writing up the notes, then the meeting joins the list in place", async ({ page }) => {
  let fresh = `{ id: 6, title: "Untitled meeting", started: Date.now(), duration_ms: 60000, source_app: null, state: "recorded", failure: null }`;
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

test("meeting-open, from the pill or the menubar, goes to that meeting from any view", async ({ page }) => {
  await open(page);
  await nav(page).getByRole("button", { name: "Your dictation" }).click();
  await emit(page, "meeting-open", 5);
  await expect(page.getByText("Launch is still the 18th.")).toBeVisible();
  expect(await calls(page, "meeting_page")).toEqual([{ id: 5 }]);
});
