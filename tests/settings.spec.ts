import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// The Settings shell: sidebar order, ‹ › history across pages and drill-ins, the Experimental listing.
async function open(page: Page, experimental = false) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna",
    models_list: () => [{ id: "stt", active: "p", models: [{ id: "p", name: "Model P", vendor: "V", desc: "d", acc: 4, spd: 4, default: true, size: 1, min_macos: 0, repo: "r", revision: "x", files: [{}], system: false, runtime: false, installed: true, needs: null }] }, { id: "diar", active: null, models: [] }],
    look_get: () => ({ theme: "dark", accent: "blue", glass: "clear", liquid: null, experimental: ${experimental}, nerds: false }),
    look_set: () => null,
  }`);
  await page.goto("/settings");
  await expect(page.getByRole("option", { name: "Listen" })).toBeVisible();
}
const pages = (page: Page) => page.getByRole("listbox", { name: "Settings pages" }).getByRole("option").evaluateAll((os) => os.map((o) => o.textContent!.trim()));
const title = (page: Page) => page.locator("main header h1");
const back = (page: Page) => page.getByRole("button", { name: "Back" });
const fwd = (page: Page) => page.getByRole("button", { name: "Forward" });

test("the sidebar follows your voice, and arrow keys follow the sidebar", async ({ page }) => {
  await open(page);
  expect(await pages(page)).toEqual(["Listen", "Understand", "Write", "Meeting notes", "General", "AI providers"]);
  await expect(page.getByRole("navigation", { name: "Settings" }).getByText("Voice typing", { exact: true })).toBeVisible(); // the caption over the three stages
  await expect(title(page)).toHaveText("Listen");
  await page.getByRole("option", { name: "Listen" }).click();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(title(page)).toHaveText("Write");
  await page.keyboard.press("ArrowUp");
  await expect(page.getByRole("option", { name: "Understand" })).toHaveAttribute("aria-selected", "true");
});

test("‹ › and ⌘[ ⌘] walk one history across pages and a drill-in; Esc leaves the drill-in", async ({ page }) => {
  await open(page);
  await expect(back(page)).toBeDisabled();
  await expect(fwd(page)).toBeDisabled();
  await page.getByRole("option", { name: "Understand" }).click();
  await page.getByRole("button", { name: "All models" }).click();
  await expect(title(page)).toHaveText("Speech-to-text · Voice typing");
  await expect(page.getByRole("option", { name: "Understand" })).toHaveAttribute("aria-selected", "true");
  await back(page).click();
  await expect(title(page)).toHaveText("Understand");
  await page.keyboard.press("Meta+BracketLeft");
  await expect(title(page)).toHaveText("Listen");
  await expect(back(page)).toBeDisabled();
  await page.keyboard.press("Meta+BracketRight");
  await fwd(page).click();
  await expect(title(page)).toHaveText("Speech-to-text · Voice typing");
  await expect(fwd(page)).toBeDisabled();
  await page.locator("main header").click(); // focus off the buttons
  await page.keyboard.press("Escape");
  await expect(title(page)).toHaveText("Understand");
  // A new page from the sidebar drops what was ahead.
  await page.getByRole("option", { name: "General" }).click();
  await expect(fwd(page)).toBeDisabled();
});

test("Experimental is listed only while its switch in General is on, and leaves history when it goes off", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("option", { name: "Experimental" })).toHaveCount(0);
  await page.getByRole("option", { name: "General" }).click();
  await page.getByRole("switch", { name: "Use experimental features" }).check();
  await page.getByRole("option", { name: "Experimental" }).click();
  await expect(title(page)).toHaveText("Experimental");
  await back(page).click();
  await page.getByRole("switch", { name: "Use experimental features" }).uncheck();
  await expect(page.getByRole("option", { name: "Experimental" })).toHaveCount(0);
  await expect(fwd(page)).toBeDisabled(); // nowhere to go: Experimental left the stack
  await expect(title(page)).toHaveText("General");
});
