import { expect, test, type Page } from "@playwright/test";
import { fakeTauri, MODELS } from "./tauri";

async function open(page: Page, words: { spelling: string; heard: string | null }[], opts: { tidy?: boolean; fails?: boolean; drill?: boolean } = {}) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => ${MODELS},
    rules_get: () => ({ settings: { language: "de", tidy: ${opts.tidy ?? true}, words: ${JSON.stringify(words)}, keep: [] }, languages: [["en", "English"], ["de", "German"]] }),
    rules_set: () => { if (${!!opts.fails}) throw "Couldn't save your words: disk full"; },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Understand" }).click();
  if (opts.drill !== false) await page.getByRole("button", { name: "Custom vocabulary" }).click();
}

const w = (spelling: string, heard: string | null = null) => ({ spelling, heard });
const sets = (page: Page) => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "rules_set").map((c: any) => c[1].rules.words));
const field = (page: Page) => page.getByLabel("Add words for Myna to remember");

test("shows the saved language, switch and words", async ({ page }) => {
  await open(page, [w("Myna"), w("Dhruv", "Drove")], { drill: false });
  await expect(page.getByLabel("Spoken language")).toHaveValue("de");
  await expect(page.getByRole("switch", { name: "Filler and repeat removal" })).toBeChecked();
  const row = page.getByRole("listitem").filter({ hasText: "Custom vocabulary" });
  await expect(row).toContainText("Myna, Drove → Dhruv");
  await expect(row.locator(".count")).toHaveText("2");
  await row.getByRole("button").click();
  await expect(page.getByRole("heading", { name: "Custom vocabulary", level: 1 })).toBeVisible();
  await expect(page.getByRole("button", { name: "Remove Myna" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Remove Drove → Dhruv" })).toBeVisible();
});

test("the switch and the language save, and the help text follows the switch", async ({ page }) => {
  await open(page, [], { drill: false });
  await expect(page.getByText("Removes um, uh and repeats")).toBeVisible();
  await page.getByRole("switch", { name: "Filler and repeat removal" }).click();
  await expect(page.getByText("Pastes exactly what the model heard.")).toBeVisible();
  await page.getByLabel("Spoken language").selectOption("en");
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "rules_set").map((c: any) => c[1].rules));
  expect(saved).toEqual([{ language: "de", tidy: false, words: [], keep: [] }, { language: "en", tidy: false, words: [], keep: [] }]);
});

test("comma and Return add words, skipping ones already there", async ({ page }) => {
  await open(page, [w("Myna")]);
  await field(page).fill("Parakeet");
  await field(page).press("Enter");
  await field(page).pressSequentially("myna, Desert Ant,");
  await expect(page.getByRole("button", { name: "Remove Desert Ant" })).toBeVisible();
  await expect(field(page)).toHaveValue("");
  expect(await sets(page)).toEqual([[w("Myna"), w("Parakeet")], [w("Myna"), w("Parakeet"), w("Desert Ant")]]);
});

test("Drove -> Dhruv makes an alias chip", async ({ page }) => {
  await open(page, []);
  await field(page).fill("Drove -> Dhruv");
  await field(page).press("Enter");
  await expect(page.getByRole("button", { name: "Remove Drove → Dhruv" })).toBeVisible();
  expect(await sets(page)).toEqual([[w("Dhruv", "Drove")]]);
});

test("× and Backspace in the empty box remove words", async ({ page }) => {
  await open(page, [w("Myna"), w("Parakeet"), w("Desert Ant")]);
  await page.getByRole("button", { name: "Remove Myna" }).click();
  await expect(field(page)).toBeFocused();
  await field(page).press("Backspace");
  await expect(page.getByRole("button", { name: /^Remove / })).toHaveCount(1);
  expect(await sets(page)).toEqual([[w("Parakeet"), w("Desert Ant")], [w("Parakeet")]]);
});

test("a failed save puts the words back and says why", async ({ page }) => {
  await open(page, [w("Myna")], { fails: true });
  await field(page).fill("Parakeet");
  await field(page).press("Enter");
  await expect(page.getByText("Couldn't save your words: disk full")).toBeVisible();
  await expect(page.getByRole("button", { name: /^Remove / })).toHaveCount(1);
});

test("a list's row shows its first three words, then how many more, and its count", async ({ page }) => {
  await open(page, [w("A"), w("B"), w("C"), w("D"), w("E")], { drill: false });
  const row = page.getByRole("listitem").filter({ hasText: "Custom vocabulary" });
  await expect(row).toContainText("A, B, C and 2 more");
  await expect(row.locator(".count")).toHaveText("5");
  const none = page.getByRole("listitem").filter({ hasText: "Filler exceptions" });
  await expect(none).toContainText("Words Myna keeps even when they sound like filler.");
  await expect(none.locator(".count")).toHaveText("");
});

test("counts update after adding a word in a drill-in, and on the way back", async ({ page }) => {
  await open(page, [w("Myna")]);
  await field(page).fill("Parakeet");
  await field(page).press("Enter");
  await page.getByRole("button", { name: "Back" }).click();
  const row = page.getByRole("listitem").filter({ hasText: "Custom vocabulary" });
  await expect(row.locator(".count")).toHaveText("2");
  await expect(row).toContainText("Myna, Parakeet");
});

test("Backspace in the empty field removes the last word, and Filler exceptions keep lowercase words", async ({ page }) => {
  await open(page, [w("Myna"), w("Parakeet")]);
  await field(page).press("Backspace");
  await expect(page.getByRole("button", { name: /^Remove / })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Remove Myna" })).toBeVisible();
  await page.getByRole("button", { name: "Back" }).click();
  await page.getByRole("button", { name: "Filler exceptions" }).click();
  const keep = page.getByLabel("Add words for Myna to never remove");
  await keep.fill("Oh, Like");
  await keep.press("Enter");
  await keep.press("Backspace");
  await expect(page.getByRole("button", { name: /^Remove / })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Remove oh from Filler exceptions" })).toBeVisible();
  await expect(page.getByText("Return or comma adds a word. Backspace in an empty field removes the last one.")).toBeVisible();
});
