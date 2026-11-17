import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Every <select class="select"> opens the app's own menu (src/lib/SelectMenu.svelte), not the native one.
test.beforeEach(async ({ page }) => {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    pill_look_get: () => ({ style: "orb", place: "top" }), pill_look_set: () => {},
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Listen" }).click();
});

test("clicking a dropdown opens the app's menu, with the current choice ticked", async ({ page }) => {
  await page.getByLabel("Style").click();
  const menu = page.getByRole("listbox", { name: "Style" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("option", { selected: true })).toHaveText("Orb");
  await menu.getByRole("option", { name: "Card" }).click();
  await expect(menu).toBeHidden();
  await expect(page.getByLabel("Style")).toHaveValue("card");
  await expect(page.getByRole("img", { name: /^Preview: Card pill/ })).toBeVisible();
});

test("the menu works from the keyboard and Escape keeps the old choice", async ({ page }) => {
  const style = page.getByLabel("Style");
  await style.focus();
  await page.keyboard.press("ArrowDown");
  const menu = page.getByRole("listbox", { name: "Style" });
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await expect(style).toBeFocused();
  await expect(style).toHaveValue("orb");
  await page.keyboard.press("Space");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(style).not.toHaveValue("orb");
});
