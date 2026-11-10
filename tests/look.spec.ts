import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const attrs = (page: Page) => page.evaluate(() => ({ ...document.documentElement.dataset }));

async function open(page: Page, setFails = false, liquid: boolean | null = null, experimental = true) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    look_get: () => ({ theme: "dark", accent: "green", glass: "tinted", liquid: ${liquid}, experimental: ${experimental} }),
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
const L = (o: object) => ({ liquid: null, experimental: true, ...o });

test("the saved look is applied before the page first shows", async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => (window as any).__first)).toEqual({ theme: "dark", accent: "green", glass: "tinted" });
});

test("shows the three rows with the saved choices", async ({ page }) => {
  await open(page);
  await expect(page.getByText("Theme", { exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Theme" }).getByRole("radio", { name: "Dark" })).toBeChecked();
  await expect(page.getByRole("group", { name: "Highlight colour" }).getByRole("radio")).toHaveCount(8);
  await expect(page.getByRole("radio", { name: "Green" })).toBeChecked();
  await expect(page.getByText("Used for buttons and selected items.")).toBeVisible();
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
  await expect(page.getByRole("radio", { name: "Graphite" })).toBeChecked();
  await expect(page.getByText("The sidebar and menus show a little of what’s behind them.")).toBeVisible();
  expect(await sets(page)).toEqual([
    L({ theme: "light", accent: "green", glass: "tinted" }),
    L({ theme: "light", accent: "graphite", glass: "tinted" }),
    L({ theme: "light", accent: "graphite", glass: "clear" }),
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
  await page.evaluate(() => (window as any).__emit("look", { theme: "light", accent: "pink", glass: "clear", liquid: null, experimental: false }));
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

test("Look & appearance has just its three rows; Liquid Glass lives in Experimental features", async ({ page }) => {
  await open(page, false, false);
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
});

test("Liquid Glass isn't offered below macOS 26", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "Experimental features" }).click();
  await expect(page.getByRole("switch", { name: "Use experimental features" })).toBeChecked();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
});

test("Liquid Glass: a switch in Experimental features, off by default, that saves and marks the page", async ({ page }) => {
  await open(page, false, false);
  await page.getByRole("option", { name: "Experimental features" }).click();
  const sw = page.getByRole("switch", { name: "Liquid Glass" });
  await expect(page.getByRole("heading", { name: "Look", exact: true })).toBeVisible();
  await expect(sw).not.toBeChecked();
  await expect(page.getByText("Uses macOS’s own glass for the pill and the Settings window. Still being tested.")).toBeVisible();
  await sw.press("Space");
  await expect(sw).toBeChecked();
  expect(await sets(page)).toEqual([{ theme: "dark", accent: "green", glass: "tinted", liquid: true, experimental: true }]);
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(true);
});

test("both pages share one look: Liquid Glass never saves a stale highlight colour", async ({ page }) => {
  await open(page, false, false);
  await page.getByRole("radio", { name: "Pink" }).check();
  await page.getByRole("option", { name: "Experimental features" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).check();
  await page.getByRole("option", { name: "Look & appearance" }).click();
  await expect(page.getByRole("radio", { name: "Pink" })).toBeChecked();
  expect((await sets(page)).at(-1)).toEqual({ theme: "dark", accent: "pink", glass: "tinted", liquid: true, experimental: true });
});

test("a failed Liquid Glass save turns the switch back and says why", async ({ page }) => {
  await open(page, true, false);
  await page.getByRole("option", { name: "Experimental features" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect(page.getByText("Couldn't save Myna's look: disk full")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).not.toBeChecked();
});

test("experimental features are off by default; the master switch shows them and keeps their own switches", async ({ page }) => {
  await open(page, false, true, false);
  await page.getByRole("option", { name: "Experimental features" }).click();
  const master = page.getByRole("switch", { name: "Use experimental features" });
  await expect(master).not.toBeChecked();
  await expect(page.getByText("Turning this off turns them all off.")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(false); // saved on, but gated
  await master.press("Space");
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toBeChecked();
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(true);
  await master.press("Space");
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
  expect(await sets(page)).toEqual([
    { theme: "dark", accent: "green", glass: "tinted", liquid: true, experimental: true },
    { theme: "dark", accent: "green", glass: "tinted", liquid: true, experimental: false },
  ]);
});

test("Liquid Glass on Settings: one sheet of glass for the whole window, frosted panels, gone when off", async ({ page }) => {
  await open(page, false, true);
  const last = () => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "glass_set").at(-1)?.[1].glass);
  const vp = page.viewportSize()!;
  await expect.poll(last).toEqual([{ x: 0, y: 0, w: vp.width, h: vp.height, radius: 0, shown: true, grow: true }]);
  await expect(page.locator("main")).toHaveCSS("background-color", "rgba(20, 20, 22, 0.42)"); // the page thins so the glass shows
  await expect(page.getByRole("option", { name: "Look & appearance" })).toHaveCSS("background-color", "rgb(30, 123, 52)"); // a solid highlight, not a wash lost in the glass
  await page.getByRole("option", { name: "Experimental features" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect.poll(last).toEqual([]);
  await expect(page.locator("main")).toHaveCSS("background-color", "rgb(29, 29, 31)"); // today's opaque page
});

test("Liquid Glass Opacity: a slider that previews live, saves the new frost, and is disabled while Liquid Glass is off", async ({ page }) => {
  await open(page, false, true);
  await page.getByRole("option", { name: "Experimental features" }).click();
  const slider = page.getByRole("slider", { name: "Opacity" });
  await expect(slider).toBeEnabled();
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(slider).toHaveValue("51");
  expect(await page.evaluate(() => document.documentElement.style.getPropertyValue("--frost"))).toBe("0.51");
  expect((await sets(page)).at(-1)).toMatchObject({ liquid: true, frost: 51 });
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect(slider).toBeDisabled();
});

test("a page that isn't built yet says so, with what will live there", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [] }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Decision provider" }).click();
  await expect(page.getByRole("heading", { name: "Not in this version yet" })).toBeVisible();
  await expect(page.getByText("Coming later")).toBeVisible();
});
