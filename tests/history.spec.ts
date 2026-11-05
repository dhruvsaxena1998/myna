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
    text_copy: () => {}, trace_delete: () => {}, settings_open: () => {},
  }`);
  await page.goto("/main");
  await expect(page.getByRole("heading", { name: "Voice typing history" })).toBeVisible(); // mounted: keys and events land
}
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter(([c]: [string]) => c === cmd).map(([, a]: [string, unknown]) => a), cmd);
const nav = (page: Page) => page.getByRole("navigation", { name: "Sidebar" });
const list = (page: Page) => page.getByRole("list", { name: "Voice typing history" }).getByRole("listitem");

test("the sidebar has Voice typing history with its count and Settings, and nothing that isn't built yet", async ({ page }) => {
  await open(page);
  await expect(nav(page).getByRole("button")).toHaveText(["Voice typing history2", "Settings⌘,"]);
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
