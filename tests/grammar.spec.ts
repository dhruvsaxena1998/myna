import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// The shared Settings grammar (design/tokens.css, DESIGN.md › Grouped list).
async function open(page: Page, width: number) {
  await page.setViewportSize({ width, height: 520 });
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [] }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "General" }).click();
}
const box = async (page: Page, name: string) => (await page.locator(".prefs .row", { hasText: name }).first().boundingBox())!;

test("one-line rows are 60px, separators add no height", async ({ page }) => {
  await open(page, 880);
  const theme = await box(page, "Theme"), accent = await box(page, "Accent colour");
  expect(theme.height).toBe(60);
  expect(accent.y - theme.y).toBe(60);
});

test("below 480px of content, a segmented control drops under its text; a switch stays right", async ({ page }) => {
  await open(page, 880);
  const wide = await page.locator(".prefs .row", { hasText: "Theme" }).locator(".seg").boundingBox();
  await open(page, 640);
  const row = await box(page, "Theme");
  const seg = (await page.locator(".prefs .row", { hasText: "Theme" }).locator(".seg").boundingBox())!;
  expect(seg.x - row.x).toBe(16); // under the text, left-aligned
  expect(seg.width).toBe(wide!.width); // not stretched
  const sw = (await page.locator(".prefs .row", { hasText: "Use experimental features" }).locator(".switch").boundingBox())!;
  const r = await box(page, "Use experimental features");
  expect(r.x + r.width - (sw.x + sw.width)).toBe(16);
});
