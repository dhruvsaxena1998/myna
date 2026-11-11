import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";

async function open(page: import("@playwright/test").Page, setFails = false) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    pill_look_get: () => ({ style: "orb", place: "top" }),
    pill_look_set: () => { if (${setFails}) throw "Couldn't save the pill's look: disk full"; },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Listen" }).click();
}

test("shows the saved style and position, with a preview", async ({ page }) => {
  await open(page);
  await expect(page.getByLabel("Style")).toHaveValue("orb");
  await expect(page.getByLabel("Position")).toHaveValue("top");
  await expect(page.getByRole("img", { name: "Preview: Orb pill, Top centre" })).toBeVisible();
});

test("saves a new choice and the preview follows, without moving the rows below", async ({ page }) => {
  await open(page);
  const below = page.getByRole("heading", { name: "Permissions" });
  const before = await below.boundingBox();
  await page.getByLabel("Style").selectOption("card");
  await page.getByLabel("Position").selectOption("bottom-right");
  await expect(page.getByRole("img", { name: "Preview: Card pill, Bottom right" })).toBeVisible();
  const sets = await page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "pill_look_set").map((c: any) => c[1]));
  expect(sets).toEqual([{ style: "card", place: "top" }, { style: "card", place: "bottom-right" }]);
  expect(await below.boundingBox()).toEqual(before);
});

test("a failed save puts the old choice back and says why", async ({ page }) => {
  await open(page, true);
  await page.getByLabel("Style").selectOption("wave");
  await expect(page.getByText("Couldn't save the pill's look: disk full")).toBeVisible();
  await expect(page.getByLabel("Style")).toHaveValue("orb");
});
