import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

async function open(page: Page, extra = "") {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    // Saved in localStorage, so a reload plays a restart with the same database.
    updates_get: () => JSON.parse(localStorage.getItem("updates") ?? '{"mode":"auto","channel":"beta"}'),
    updates_set: ({ prefs }) => void localStorage.setItem("updates", JSON.stringify(prefs)),
    update_check: () => null, update_download: () => null,
    ${extra}
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "About" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "plugin:event|listen" && c[1].event === "update-state"))).toBe(true);
}
const emit = (page: Page, s: object) => page.evaluate((s) => (window as any).__emit("update-state", s), s);
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c[0] === cmd).length, cmd);

test("the Version row shows each update state without moving", async ({ page }) => {
  await open(page);
  const row = page.getByRole("listitem").filter({ hasText: "0.1.0" });
  const before = await row.boundingBox();
  const slot = row.locator(".slot");
  const width = (await slot.boundingBox())!.width;
  const states: [object, string][] = [
    [{ state: "checking" }, "Checking…"],
    [{ state: "available", version: "0.0.2-beta.2", notes: "" }, "Update available · 0.0.2-beta.2"],
    [{ state: "downloading", pct: 42 }, "Downloading 42%"],
    [{ state: "downloaded" }, "Restart to update"],
    [{ state: "error", kind: "check", message: "Couldn’t reach GitHub. Check your connection." }, "Update failedTry again"],
    [{ state: "uptodate" }, "Check for updates"],
  ];
  for (const [s, text] of states) {
    await emit(page, s);
    await expect(slot).toHaveText(text);
    expect(await row.boundingBox()).toEqual(before);
    expect((await slot.boundingBox())!.width).toBe(width);
  }
});

test("Check for updates and Try again check; an error shows its message", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Check for updates" }).click();
  await emit(page, { state: "error", kind: "signature", message: "This update failed its security check and wasn’t installed." });
  await expect(page.getByText("This update failed its security check and wasn’t installed.")).toBeVisible();
  await page.getByRole("button", { name: "Try again" }).click();
  expect(await calls(page, "update_check")).toBe(2);
});

test("Restart to update says why it can't while dictating", async ({ page }) => {
  await open(page, `update_restart: () => { throw "Finish recording first"; }`);
  await emit(page, { state: "downloaded" });
  await page.getByRole("button", { name: "Restart to update" }).click();
  await expect(page.getByText("Finish recording first")).toBeVisible();
});

test("Update mode and Channel show their hints and persist across a restart", async ({ page }) => {
  await open(page);
  const mode = page.getByRole("combobox", { name: "Update mode" });
  await expect(mode).toHaveValue("auto");
  await expect(page.getByText("Checks on its own, downloads in the background, then asks you to restart.")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
  await expect(page.getByText("Beta also gets newer stable releases.", { exact: false })).toBeVisible();

  await mode.selectOption("off");
  await expect(page.getByText("Never checks on its own. Use Check for updates when you want to.")).toBeVisible();
  await page.getByRole("radio", { name: "Stable" }).check();

  await page.reload();
  await page.getByRole("option", { name: "About" }).click();
  await expect(page.getByRole("combobox", { name: "Update mode" })).toHaveValue("off");
  await expect(page.getByRole("radio", { name: "Stable" })).toBeChecked();
});

test("a failed save puts the old choice back and says why", async ({ page }) => {
  await open(page, `updates_set: () => { throw "Couldn't save the update settings: disk full"; }`);
  await page.getByRole("radio", { name: "Stable" }).click();
  await expect(page.getByText("Couldn't save the update settings: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
});
