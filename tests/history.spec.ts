import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const ROWS = `(() => {
  const ago = (h) => Date.now() - h * 3600e3;
  return [
    { id: 4, started: ago(1), app: "Slack", outcome: "pasted", heard: "Um so ship the post gres fix", text: "So ship the Postgres fix" },
    { id: 3, started: ago(2), app: "Terminal", outcome: "nothing_heard", heard: "Um.", text: "" },
    { id: 2, started: ago(30), app: "Mail", outcome: "pasted", heard: "Send the deck to Priya before lunch.", text: "Send the deck to Priya before lunch." },
    { id: 1, started: ago(50), app: null, outcome: "failed", heard: null, text: null },
  ];
})()`;

async function open(page: Page, rows = ROWS) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna",
    traces_list: () => ${rows},
    text_copy: () => {}, trace_delete: () => {}, traces_clear: () => {}, settings_open: () => {},
  }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: /^Voice typing history/ }).click(); // Meetings is the first view
  await expect(page.getByRole("heading", { name: "Voice typing history" })).toBeVisible(); // mounted: keys and events land
}
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter(([c]: [string]) => c === cmd).map(([, a]: [string, unknown]) => a), cmd);
const nav = (page: Page) => page.getByRole("navigation", { name: "Sidebar" });
const list = (page: Page) => page.getByRole("list", { name: "Voice typing history" }).getByRole("listitem");

test("the sidebar has Voice typing history with its count and Settings, and nothing that isn't built yet", async ({ page }) => {
  await open(page);
  await expect(nav(page).getByRole("button")).toHaveText(["All meetings0", "Voice typing history2", "Your dictation", "Settings⌘,"]);
  await expect(nav(page).getByRole("button", { name: "Voice typing history" })).toHaveAttribute("aria-current", "true");
});

test("lists what was said, newest first; dictations that left no text aren't there", async ({ page }) => {
  await open(page);
  await expect(list(page)).toHaveCount(2);
  await expect(list(page).nth(0)).toContainText("So ship the Postgres fix");
  await expect(list(page).nth(0)).toContainText("Slack · Today");
  await expect(list(page).nth(1)).toContainText("Mail · Yesterday");
});

test("Copy again copies what was pasted; Copy what I said shows only when it differs", async ({ page }) => {
  await open(page);
  const [cleaned, same] = [list(page).nth(0), list(page).nth(1)];
  await expect(same.getByRole("button", { name: "Copy what I said" })).toHaveCount(0);
  await cleaned.getByRole("button", { name: "Copy again" }).click();
  await cleaned.getByRole("button", { name: "Copy what I said" }).click();
  expect(await calls(page, "text_copy")).toEqual([{ text: "So ship the Postgres fix" }, { text: "Um so ship the post gres fix" }]);
});

test("Delete removes the dictation's trace and the row, and the count follows", async ({ page }) => {
  await open(page);
  await list(page).nth(0).getByRole("button", { name: "Delete" }).click();
  await expect(list(page)).toHaveCount(1);
  await expect(list(page).nth(0)).toContainText("Send the deck");
  await expect(nav(page).getByRole("button", { name: "Voice typing history" })).toContainText("1");
  expect(await calls(page, "trace_delete")).toEqual([{ id: 4 }]);
});

test("a failed delete keeps the row and says why", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", traces_list: () => ${ROWS}, trace_delete: () => { throw "Couldn't delete the dictation: disk full"; } }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: /^Voice typing history/ }).click(); // Meetings is the first view
  await list(page).nth(0).getByRole("button", { name: "Delete" }).click();
  await expect(page.getByRole("status").filter({ hasText: "disk full" })).toBeVisible();
  await expect(list(page)).toHaveCount(2);
});

test("/ focuses search; search filters by words and app; no matches can be cleared", async ({ page }) => {
  await open(page);
  const search = page.getByRole("searchbox", { name: "Search what you said" });
  await page.keyboard.press("/");
  await expect(search).toBeFocused();
  await page.keyboard.type("priya");
  await expect(list(page)).toHaveCount(1);
  await search.fill("slack postgres");
  await expect(list(page)).toHaveCount(1);
  await search.fill("nowhere");
  await expect(page.getByRole("heading", { name: "No matches" })).toBeVisible();
  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(list(page)).toHaveCount(2);
  await expect(search).toBeFocused();
  await search.fill("priya");
  await search.press("Escape");
  await expect(list(page)).toHaveCount(2);
});

test("an empty history says how to fill it", async ({ page }) => {
  await open(page, "[]");
  await expect(page.getByRole("heading", { name: "Nothing here yet" })).toBeVisible();
  await expect(nav(page).getByRole("button", { name: "Voice typing history" })).toContainText("0");
});

test("coming back to the window picks up dictations made since", async ({ page }) => {
  await open(page, "[]");
  await page.evaluate(() => {
    (window as any).__TAURI_INTERNALS__.invoke = ((inv) => async (cmd: string, a: unknown) =>
      cmd === "traces_list" ? [{ id: 9, started: Date.now(), app: "Notes", outcome: "pasted", heard: "new one", text: "New one" }] : inv(cmd, a))((window as any).__TAURI_INTERNALS__.invoke);
    (window as any).__emit("tauri://focus", true);
  });
  await expect(list(page)).toHaveText([/New one/]);
});

test("Settings and ⌘, open the Settings window", async ({ page }) => {
  await open(page);
  await nav(page).getByRole("button", { name: "Settings" }).click();
  await page.keyboard.press("Meta+Comma");
  expect(await calls(page, "settings_open")).toHaveLength(2);
});

test("deleting doesn't move the header or the rows above", async ({ page }) => {
  await open(page);
  const head = await page.getByRole("heading", { name: "Voice typing history" }).boundingBox();
  const first = await list(page).nth(0).boundingBox();
  await list(page).nth(1).getByRole("button", { name: "Delete" }).click();
  await expect(list(page)).toHaveCount(1);
  expect(await page.getByRole("heading", { name: "Voice typing history" }).boundingBox()).toEqual(head);
  expect((await list(page).nth(0).boundingBox())?.y).toEqual(first?.y);
});

test("Clear all asks first; Cancel and Esc keep everything", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Clear all" }).click();
  const ask = page.getByRole("alertdialog");
  await expect(ask).toContainText("Delete all dictations, with their stats? This can’t be undone.");
  await expect(ask.getByRole("button", { name: "Cancel" })).toBeFocused();
  await ask.getByRole("button", { name: "Cancel" }).click();
  await expect(ask).toHaveCount(0);
  await page.getByRole("button", { name: "Clear all" }).click();
  await page.keyboard.press("Escape");
  await expect(ask).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Clear all" })).toBeFocused();
  expect(await calls(page, "traces_clear")).toEqual([]);
  await expect(list(page)).toHaveCount(2);
});

test("Delete all clears the history, and Clear all waits for the next dictation", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Clear all" }).click();
  await page.getByRole("button", { name: "Delete all" }).click();
  expect(await calls(page, "traces_clear")).toHaveLength(1);
  await expect(page.getByRole("heading", { name: "Nothing here yet" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Clear all" })).toBeDisabled();
  await expect(nav(page).getByRole("button", { name: /^Voice typing history/ })).toHaveText("Voice typing history0");
});

test("a failed Clear all keeps the history and says why", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", traces_list: () => ${ROWS}, traces_clear: () => { throw "Couldn't clear the history: disk full"; } }`);
  await page.goto("/main");
  await page.getByRole("navigation", { name: "Sidebar" }).getByRole("button", { name: /^Voice typing history/ }).click(); // Meetings is the first view
  await page.getByRole("button", { name: "Clear all" }).click();
  await page.getByRole("button", { name: "Delete all" }).click();
  await expect(page.getByRole("alertdialog")).toContainText("disk full");
  await expect(list(page)).toHaveCount(2);
});
