import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const attrs = (page: Page) => page.evaluate(() => ({ ...document.documentElement.dataset }));

async function open(page: Page, setFails = false, liquid: boolean | null = null, experimental = true) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    look_get: () => ({ theme: "dark", accent: "green", glass: "tinted", liquid: ${liquid}, experimental: ${experimental} }),
    look_set: () => { if (${setFails}) throw "disk full"; },
  }`);
  // The saved look must be on <html> before anything paints: record the attributes at the first frame with content.
  await page.addInitScript(() => {
    new MutationObserver((_, o) => {
      if (document.querySelector("main")) ((window as any).__first = { ...document.documentElement.dataset }), o.disconnect();
    }).observe(document, { childList: true, subtree: true });
  });
  await page.goto("/settings");
  await expect(page.locator("main")).toBeVisible();
  await page.getByRole("option", { name: "General" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "plugin:event|listen" && c[1].event === "look"))).toBe(true);
}
const sets = (page: Page) => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "look_set").map((c: any) => c[1].look));
const L = (o: object) => ({ liquid: null, experimental: true, ...o });

test("the saved look is applied before the page first shows", async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => (window as any).__first)).toEqual({ theme: "dark", accent: "green", glass: "tinted" });
});

test("shows the Appearance rows with the saved choices", async ({ page }) => {
  await open(page);
  await expect(page.getByText("Theme", { exact: true })).toBeVisible();
  await expect(page.getByRole("group", { name: "Theme" }).getByRole("radio", { name: "Dark" })).toBeChecked();
  await expect(page.getByRole("group", { name: "Accent colour" }).getByRole("radio")).toHaveCount(8);
  await expect(page.getByRole("radio", { name: "Green" })).toBeChecked();
  await expect(page.getByText("Buttons, switches and selection.")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Tinted" })).toBeChecked();
  await expect(page.getByText("The sidebar takes a hint of your highlight colour.")).toBeVisible();
});

test("each choice saves and restyles at once, without moving the rows", async ({ page }) => {
  await open(page);
  const row = page.getByText("Window tint");
  const before = await row.boundingBox();
  await page.getByRole("radio", { name: "Light" }).check();
  await page.getByRole("radio", { name: "Graphite" }).check();
  await page.getByRole("radio", { name: "Clear" }).check();
  expect(await attrs(page)).toEqual({ theme: "light", accent: "graphite", glass: "clear" });
  await expect(page.getByRole("radio", { name: "Graphite" })).toBeChecked();
  await expect(page.getByText("The sidebar shows your desktop through it.")).toBeVisible();
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
  await expect(page.getByText("Couldn't save the accent colour: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Green" })).toBeChecked();
  expect((await attrs(page)).accent).toBe("green");
});

test("if the look can't be read, the page says why and the default look applies", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], look_get: () => { throw "Couldn't read Myna's look: locked"; } }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "General" }).click();
  await expect(page.getByText("Couldn't read Myna's look: locked")).toBeVisible();
  expect(await attrs(page)).toEqual({ theme: "auto", accent: "blue", glass: "clear" });
});

test("General's Appearance has just its three rows; Liquid Glass lives in Experimental", async ({ page }) => {
  await open(page, false, false);
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
});

test("Liquid Glass isn't offered below macOS 26", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("switch", { name: "Use experimental features" })).toBeChecked();
  await page.getByRole("option", { name: "Experimental" }).click();
  await expect(page.getByRole("switch", { name: "Stats for nerds" })).toBeVisible();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
});

test("Liquid Glass: a switch in Experimental features, off by default, that saves and marks the page", async ({ page }) => {
  await open(page, false, false);
  await page.getByRole("option", { name: "Experimental" }).click();
  const sw = page.getByRole("switch", { name: "Liquid Glass" });
  await expect(page.getByRole("heading", { name: "Appearance", exact: true })).toBeVisible();
  await expect(sw).not.toBeChecked();
  await expect(page.getByText("Uses macOS’s own glass for the pill and the Settings window.")).toBeVisible();
  await sw.press("Space");
  await expect(sw).toBeChecked();
  expect(await sets(page)).toEqual([{ theme: "dark", accent: "green", glass: "tinted", liquid: true, experimental: true }]);
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(true);
});

test("both pages share one look: Liquid Glass never saves a stale highlight colour", async ({ page }) => {
  await open(page, false, false);
  await page.getByRole("radio", { name: "Pink" }).check();
  await page.getByRole("option", { name: "Experimental" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).check();
  await page.getByRole("option", { name: "General" }).click();
  await expect(page.getByRole("radio", { name: "Pink" })).toBeChecked();
  expect((await sets(page)).at(-1)).toEqual({ theme: "dark", accent: "pink", glass: "tinted", liquid: true, experimental: true });
});

test("a failed Liquid Glass save turns the switch back and says why", async ({ page }) => {
  await open(page, true, false);
  await page.getByRole("option", { name: "Experimental" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect(page.getByText("Couldn't save Liquid Glass: disk full")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).not.toBeChecked();
});

test("experimental features are off by default; the switch in General lists the Experimental page and keeps their own switches", async ({ page }) => {
  await open(page, false, true, false);
  const master = page.getByRole("switch", { name: "Use experimental features" });
  await expect(master).not.toBeChecked();
  await expect(page.getByRole("option", { name: "Experimental" })).toHaveCount(0);
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(false); // saved on, but gated
  await master.press("Space");
  expect(await page.evaluate(() => document.documentElement.hasAttribute("data-liquid"))).toBe(true);
  await page.getByRole("option", { name: "Experimental" }).click();
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toBeChecked();
  await page.getByRole("option", { name: "General" }).click();
  await master.press("Space");
  await expect(page.getByRole("option", { name: "Experimental" })).toHaveCount(0);
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
  await expect(page.getByRole("option", { name: "General" })).toHaveCSS("background-color", "rgb(30, 123, 52)"); // a solid highlight, not a wash lost in the glass
  await page.getByRole("option", { name: "Experimental" }).click();
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect.poll(last).toEqual([]);
  await expect(page.locator("main")).toHaveCSS("background-color", "rgb(29, 29, 31)"); // today's opaque page
});

test("Glass opacity: a slider that previews live, saves the new frost, and is disabled while Liquid Glass is off", async ({ page }) => {
  await open(page, false, true);
  await page.getByRole("option", { name: "Experimental" }).click();
  const slider = page.getByRole("slider", { name: "Glass opacity" });
  await expect(slider).toBeEnabled();
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(slider).toHaveValue("51");
  expect(await page.evaluate(() => document.documentElement.style.getPropertyValue("--frost"))).toBe("0.51");
  expect((await sets(page)).at(-1)).toMatchObject({ liquid: true, frost: 51 });
  await page.getByRole("switch", { name: "Liquid Glass" }).click();
  await expect(slider).toBeDisabled();
});

test("the selected swatch's inner dot is dark on yellow and white elsewhere", async ({ page }) => {
  await open(page);
  const pip = (name: string) => page.getByRole("radio", { name }).locator("xpath=following-sibling::span[1]/span");
  await expect(pip("Green")).toBeVisible();
  await expect(pip("Green")).toHaveCSS("background-color", "rgb(255, 255, 255)");
  await expect(pip("Yellow")).toBeHidden();
  await page.getByRole("radio", { name: "Yellow" }).check();
  await expect(pip("Yellow")).toBeVisible();
  await expect(pip("Yellow")).toHaveCSS("background-color", "rgb(29, 29, 31)");
  await expect(pip("Green")).toBeHidden();
});

test("a failed Stats for nerds save names it, not the look", async ({ page }) => {
  await open(page, true, null);
  await page.getByRole("option", { name: "Experimental" }).click();
  await page.getByRole("switch", { name: "Stats for nerds" }).click();
  await expect(page.getByText("Couldn't save Stats for nerds: disk full")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Stats for nerds" })).not.toBeChecked();
});

test("Glass opacity is dimmed as a whole while Liquid Glass is off", async ({ page }) => {
  await open(page, false, false);
  await page.getByRole("option", { name: "Experimental" }).click();
  const slider = page.locator(".slider");
  await expect(slider).toHaveCSS("opacity", "0.45");
  await page.getByRole("switch", { name: "Liquid Glass" }).check();
  await expect(slider).toHaveCSS("opacity", "1");
});

test("older macOS: no Liquid Glass rows and no Appearance section on the Experimental page", async ({ page }) => {
  await open(page, false, null);
  await page.getByRole("option", { name: "Experimental" }).click();
  await expect(page.getByRole("heading", { name: "Voice typing" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Appearance" })).toHaveCount(0);
  await expect(page.getByRole("switch", { name: "Liquid Glass" })).toHaveCount(0);
  await expect(page.getByRole("slider", { name: "Glass opacity" })).toHaveCount(0);
});
