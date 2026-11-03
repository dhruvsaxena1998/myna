import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";

const open = async (page: import("@playwright/test").Page, on: boolean) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], debug_on: () => ${on} }`);
  await page.goto("/settings");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "debug_on"))).toBe(true);
  await page.waitForTimeout(50); // its answer lands a tick later
};

test("MYNA_DEBUG shows the window size while resizing, then hides it", async ({ page }) => {
  await open(page, true);
  await page.setViewportSize({ width: 900, height: 600 });
  await expect(page.getByLabel("Window size")).toHaveText("900 × 600");
  await expect(page.getByLabel("Window size")).toHaveCount(0, { timeout: 3000 });
});

test("without MYNA_DEBUG, resizing shows nothing", async ({ page }) => {
  await open(page, false);
  await page.setViewportSize({ width: 900, height: 600 });
  await page.waitForTimeout(300);
  await expect(page.getByLabel("Window size")).toHaveCount(0);
});
