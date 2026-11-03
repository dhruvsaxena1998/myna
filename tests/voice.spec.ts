import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// `voice_set` fails for any shortcut using Ctrl+Alt, like a shortcut another app holds.
async function open(page: Page) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    voice_get: () => ({ on: true, key: "Alt+Space", mode: "hold" }),
    voice_set: ({ voice }) => { if (voice.key.startsWith("Ctrl+Alt")) throw "Another app is using that shortcut. Pick a different one."; },
    voice_pause: () => {},
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Voice typing" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("shows the shortcut", async ({ page }) => {
  await open(page);
  await expect(page.getByLabel("Current shortcut: Option Space")).toHaveText("⌥Space");
});

test("Change records a new shortcut and saves it; the old one is paused meanwhile", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await expect(page.getByRole("button", { name: "Press a key…" })).toBeFocused();
  await expect(page.getByText("Press the key or keys you want to use. Esc cancels.")).toBeVisible();
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("KeyD");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await expect(page.getByLabel("Current shortcut: Control Shift D")).toHaveText("⌃⇧D");
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }]);
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Ctrl+Shift+KeyD", mode: "hold" } }]);
});

test("Esc cancels and gives the old shortcut back", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Current shortcut: Option Space")).toBeVisible();
  await expect(page.getByRole("button", { name: "Change" })).toBeFocused();
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }, { paused: false }]);
  expect(await calls(page, "voice_set")).toEqual([]);
});

test("a key that types on its own is refused, and recording goes on", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("KeyD");
  await expect(page.getByText("Add ⌘, ⌥ or ⌃, so the key still types normally.")).toBeVisible();
  await page.keyboard.press("F5");
  await expect(page.getByLabel("Current shortcut: F5")).toBeVisible();
  await expect(page.getByText("Add ⌘")).toBeHidden();
});

test("a shortcut another app holds says so and keeps the old one", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(page.getByLabel("Current shortcut: Option Space")).toBeVisible();
});

test("Tap to start and stop saves, says how it stops, and the preview follows", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Release to paste");
  await page.getByText("Tap to start and stop").click();
  await expect(page.getByText("Press once to start and again to stop. Stops by itself after 5 minutes.")).toBeVisible();
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Press again to paste");
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", mode: "tap" } }]);
});

test("turning voice typing off hides its settings; on brings them back", async ({ page }) => {
  await open(page);
  const sw = page.getByRole("switch", { name: "Use voice typing" });
  await sw.click();
  await expect(sw).not.toBeChecked();
  for (const h of ["Shortcut", "Pill", "Permissions"]) await expect(page.getByRole("heading", { name: h })).toBeHidden();
  await sw.click();
  await expect(page.getByRole("heading", { name: "Shortcut" })).toBeVisible();
  expect((await calls(page, "voice_set")).map((c: any) => c.voice.on)).toEqual([false, true]);
});
