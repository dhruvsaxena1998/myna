import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

async function open(page: Page, words: { spelling: string; heard: string | null }[], opts: { tidy?: boolean; fails?: boolean } = {}) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    rules_get: () => ({ settings: { language: "de", tidy: ${opts.tidy ?? true}, words: ${JSON.stringify(words)} }, languages: [["en", "English"], ["de", "German"]] }),
    rules_set: () => { if (${!!opts.fails}) throw "Couldn't save your words: disk full"; },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Voice typing" }).click();
}

const w = (spelling: string, heard: string | null = null) => ({ spelling, heard });
const sets = (page: Page) => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "rules_set").map((c: any) => c[1].rules.words));
const field = (page: Page) => page.getByLabel("Add words for Myna to remember");

test("shows the saved language, switch and words", async ({ page }) => {
  await open(page, [w("Myna"), w("Dhruv", "Drove")]);
  await expect(page.getByLabel("Language you speak")).toHaveValue("de");
  await expect(page.getByRole("switch", { name: "Tidy up as I talk" })).toBeChecked();
  await expect(page.getByRole("button", { name: "Remove Myna" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Remove Drove → Dhruv" })).toBeVisible();
});

test("the switch and the language save, and the help text follows the switch", async ({ page }) => {
  await open(page, []);
  await expect(page.getByText("Removes um and uh and repeated words")).toBeVisible();
  await page.getByRole("switch", { name: "Tidy up as I talk" }).click();
  await expect(page.getByText("Pastes exactly what the model heard. Words to remember still apply.")).toBeVisible();
  await page.getByLabel("Language you speak").selectOption("en");
  const saved = await page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "rules_set").map((c: any) => c[1].rules));
  expect(saved).toEqual([{ language: "de", tidy: false, words: [] }, { language: "en", tidy: false, words: [] }]);
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
