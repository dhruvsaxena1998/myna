import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const attrs = (page: Page) => page.evaluate(() => ({ ...document.documentElement.dataset }));

async function open(page: Page, setFails = false) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    look_get: () => ({ theme: "dark", accent: "green", glass: "tinted" }),
    look_set: () => { if (${setFails}) throw "Couldn't save Myna's look: disk full"; },
  }`);
  // The saved look must be on <html> before anything paints: record the attributes at the first frame with content.
  await page.addInitScript(() => {
    new MutationObserver((_, o) => {
      if (document.querySelector("main")) ((window as any).__first = { ...document.documentElement.dataset }), o.disconnect();
    }).observe(document, { childList: true, subtree: true });
  });
  await page.goto("/settings");
  await expect(page.locator("main")).toBeVisible();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "plugin:event|listen" && c[1].event === "look"))).toBe(true);
}
const sets = (page: Page) => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "look_set").map((c: any) => c[1].look));

test("the saved look is applied before the page first shows", async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => (window as any).__first)).toEqual({ theme: "dark", accent: "green", glass: "tinted" });
});

test("shows the three rows with the saved choices", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("heading", { name: "Appearance", exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Appearance" }).getByRole("radio", { name: "Dark" })).toBeChecked();
  await expect(page.getByRole("group", { name: "Highlight colour" }).getByRole("radio")).toHaveCount(8);
  await expect(page.getByRole("radio", { name: "Green" })).toBeChecked();
  await expect(page.getByText("Used for buttons and selected items. Now: Green.")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Tinted" })).toBeChecked();
  await expect(page.getByText("The sidebar and menus take on a hint of your highlight colour.")).toBeVisible();
});

test("each choice saves and restyles at once, without moving the rows", async ({ page }) => {
  await open(page);
  const row = page.getByText("See-through panels");
  const before = await row.boundingBox();
  await page.getByRole("radio", { name: "Light" }).check();
  await page.getByRole("radio", { name: "Graphite" }).check();
  await page.getByRole("radio", { name: "Clear" }).check();
  expect(await attrs(page)).toEqual({ theme: "light", accent: "graphite", glass: "clear" });
  await expect(page.getByText("Now: Graphite.")).toBeVisible();
  await expect(page.getByText("The sidebar and menus show a little of what’s behind them.")).toBeVisible();
  expect(await sets(page)).toEqual([
    { theme: "light", accent: "green", glass: "tinted" },
    { theme: "light", accent: "graphite", glass: "tinted" },
    { theme: "light", accent: "graphite", glass: "clear" },
  ]);
  expect(await row.boundingBox()).toEqual(before);
});

test("the keyboard moves between swatches", async ({ page }) => {
  await open(page);
  await page.getByRole("radio", { name: "Green" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("radio", { name: "Graphite" })).toBeChecked();
  expect((await attrs(page)).accent).toBe("graphite");
});

test("a change made in another window restyles this one", async ({ page }) => {
  await open(page);
  await page.evaluate(() => (window as any).__emit("look", { theme: "light", accent: "pink", glass: "clear" }));
  expect(await attrs(page)).toEqual({ theme: "light", accent: "pink", glass: "clear" });
});

test("a failed save puts the old choice back and says why", async ({ page }) => {
  await open(page, true);
  await page.getByRole("radio", { name: "Orange" }).click();
  await expect(page.getByText("Couldn't save Myna's look: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Green" })).toBeChecked();
  expect((await attrs(page)).accent).toBe("green");
});

test("if the look can't be read, the page says why and the default look applies", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => { throw "Couldn't read Myna's look: locked"; } }`);
  await page.goto("/settings");
  await expect(page.getByText("Couldn't read Myna's look: locked")).toBeVisible();
  expect(await attrs(page)).toEqual({ theme: "auto", accent: "blue", glass: "clear" });
});
