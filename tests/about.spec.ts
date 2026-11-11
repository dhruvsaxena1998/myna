import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

async function open(page: Page, extra = "") {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    // Saved in localStorage, so a reload plays a restart with the same database.
    updates_get: () => JSON.parse(localStorage.getItem("updates") ?? '{"mode":"auto","channel":"beta","every":"launch"}'),
    updates_set: ({ prefs }) => void localStorage.setItem("updates", JSON.stringify(prefs)),
    update_check: () => null, update_download: () => null,
    ${extra}
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "General" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "plugin:event|listen" && c[1].event === "update-state"))).toBe(true);
}
const emit = (page: Page, s: object) => page.evaluate((s) => (window as any).__emit("update-state", s), s);
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c[0] === cmd).length, cmd);

test("the Version row shows each update state without moving", async ({ page }) => {
  await open(page);
  const row = page.getByRole("listitem").filter({ hasText: "Version 0.1.0" });
  const before = await row.boundingBox();
  const btn = row.getByRole("button");
  const width = (await btn.boundingBox())!.width;
  const v = "0.0.2-beta.2";
  const states: [object, string, string][] = [
    [{ state: "checking" }, "Checking…", ""],
    [{ state: "available", version: v, notes: "" }, "View update", ` · ${v} is available`],
    [{ state: "downloading", version: v, notes: "", pct: 42 }, "Downloading 42%", ` · ${v} is available`],
    [{ state: "downloaded", version: v, notes: "" }, "Restart to update", ` · ${v} is ready`],
    [{ state: "error", kind: "check", message: "Couldn’t reach GitHub. Check your connection." }, "Try again", ""],
    [{ state: "uptodate" }, "Check for updates", " · Up to date"],
  ];
  for (const [s, text, note] of states) {
    await emit(page, s);
    await expect(btn).toHaveText(text);
    await expect(row).toContainText("Version 0.1.0 · dev");
    if (note) await expect(row).toContainText(note);
    expect(await row.boundingBox()).toEqual(before);
    expect((await btn.boundingBox())!.width).toBe(width);
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

test("a failed update says so once", async ({ page }) => {
  await open(page);
  await emit(page, { state: "error", kind: "check", message: "Couldn’t reach GitHub. Check your connection." });
  await expect(page.getByText("Couldn’t reach GitHub. Check your connection.")).toHaveCount(1);
  await expect(page.getByText("Update failed")).toHaveCount(0);
});

test("Show in Finder reveals the data folder, and says why if it can't", async ({ page }) => {
  await open(page, `storage_reveal: () => { throw "Couldn't open /tmp/Myna: nope"; }`);
  await expect(page.locator(".mono")).toHaveText("/tmp/Myna");
  await page.getByRole("button", { name: "Show in Finder" }).click();
  expect(await calls(page, "storage_reveal")).toBe(1);
  await expect(page.getByText("Couldn't open /tmp/Myna: nope")).toBeVisible();
});

test("Check frequency is hidden while Update mode is Off, and the rows below keep their height", async ({ page }) => {
  await open(page);
  const mode = page.getByRole("combobox", { name: "Update mode" });
  await expect(page.getByRole("combobox", { name: "Check frequency" })).toBeVisible();
  const row = page.getByRole("listitem").filter({ hasText: "Update channel" });
  const h = (await row.boundingBox())!.height;
  await mode.selectOption("off");
  await expect(page.getByRole("combobox", { name: "Check frequency" })).toHaveCount(0);
  expect((await row.boundingBox())!.height).toBe(h);
  await mode.selectOption("check");
  await expect(page.getByRole("combobox", { name: "Check frequency" })).toBeVisible();
});

test("Update available and Restart to update open the update pop-up", async ({ page }) => {
  await open(page, `update_open: () => null`);
  await emit(page, { state: "available", version: "0.0.2-beta.2", notes: "" });
  await page.getByRole("button", { name: "View update" }).click();
  await emit(page, { state: "downloaded", version: "0.0.2-beta.2", notes: "" });
  await page.getByRole("button", { name: "Restart to update" }).click();
  expect(await calls(page, "update_open")).toBe(2);
  expect(await calls(page, "update_download") + (await calls(page, "update_restart"))).toBe(0);
});

test("Update mode and Update channel show their hints and persist across a restart", async ({ page }) => {
  await open(page);
  const mode = page.getByRole("combobox", { name: "Update mode" });
  await expect(mode).toHaveValue("auto");
  await expect(page.getByText("Downloads in the background, then asks you to restart.")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
  await expect(page.getByText("Gets pre-releases as well as stable releases.")).toBeVisible();

  await mode.selectOption("off");
  await expect(page.getByText("Only checks when you click Check for updates.")).toBeVisible();
  await page.getByRole("radio", { name: "Stable" }).check();

  await page.reload();
  await page.getByRole("option", { name: "General" }).click();
  await expect(page.getByRole("combobox", { name: "Update mode" })).toHaveValue("off");
  await expect(page.getByRole("radio", { name: "Stable" })).toBeChecked();
});

test("Check frequency persists", async ({ page }) => {
  await open(page);
  const every = page.getByRole("combobox", { name: "Check frequency" });
  await expect(every).toHaveValue("launch");
  await every.selectOption("week");
  await expect(page.getByText("Checks once a week, even if you quit and reopen Myna.")).toBeVisible();

  await page.reload();
  await page.getByRole("option", { name: "General" }).click();
  await expect(page.getByRole("combobox", { name: "Check frequency" })).toHaveValue("week");
});

test("a failed save puts the old choice back and says why", async ({ page }) => {
  await open(page, `updates_set: () => { throw "Couldn't save the update settings: disk full"; }`);
  await page.getByRole("radio", { name: "Stable" }).click();
  await expect(page.getByText("Couldn't save the update settings: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Beta" })).toBeChecked();
});

test("the dev build label sits after the version with a space either side of the dot", async ({ page }) => {
  await open(page);
  // The dev server always has a build label (vite.config.js), so the row reads "Version 0.1.0 · dev …".
  await expect(page.getByText(/^Version 0\.1\.0 · dev/)).toBeVisible();
});
