import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Meeting notes' own speech-to-text model (18.7): the same catalog and downloads as voice typing, its own pick.
const m = (id: string, installed: boolean) => ({
  id, name: `Model ${id}`, vendor: "V", desc: `${id} description`, acc: 4, spd: 4, default: id === "a", size: 500_000_000, min_macos: 0,
  repo: "r", revision: "x".repeat(40), files: [{}], system: false, runtime: false, installed, needs: null,
});

async function open(page: Page) {
  // The fake backend keeps the picks; meeting_stt follows stt while it has none of its own, like models.rs.
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna",
    meeting_get: () => ({ on: true, key: "Ctrl+Alt+KeyM", keep_teams: false }),
    models_list: () => {
      const p = (window.__picks ??= { stt: "a", meeting_stt: null }), cat = [${JSON.stringify(m("a", true))}, ${JSON.stringify(m("b", true))}, ${JSON.stringify(m("c", false))}];
      return [{ id: "stt", active: p.stt, models: cat }, { id: "diar", active: null, models: [] }, { id: "meeting_stt", active: p.meeting_stt ?? p.stt, models: cat }];
    },
    model_select: ({ task, id }) => { window.__picks[task] = id; },
    model_delete: ({ id, task }) => { for (const k in window.__picks) if (window.__picks[k] === id) window.__picks[k] = k === "stt" ? "a" : null; return task === "stt" ? window.__picks.stt : (window.__picks.meeting_stt ?? window.__picks.stt); },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Meeting notes" }).click();
}
const row = (page: Page) => page.locator(".row", { hasText: "Speech-to-text model" });
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("Meeting notes opens its own model list, and a pick there leaves voice typing's alone", async ({ page }) => {
  await open(page);
  await expect(row(page)).toContainText("Model a · Installed");
  await row(page).getByRole("button", { name: "All models" }).click();
  await expect(page.locator("main header h1")).toHaveText("Speech-to-text · Meeting notes");
  await expect(page.getByText("Used for meeting notes. Downloads are shared with voice typing.")).toBeVisible();
  const list = page.getByRole("radiogroup", { name: "Meeting notes speech-to-text model" });
  await expect(list.getByRole("radio", { name: "Use Model a" })).toBeChecked();
  await list.getByRole("radio", { name: "Use Model b" }).check();
  expect(await calls(page, "model_select")).toEqual([{ task: "meeting_stt", id: "b" }]);
  await page.getByRole("button", { name: "Back" }).click();
  await expect(row(page)).toContainText("Model b · Installed");
  await page.getByRole("option", { name: "Understand" }).click();
  await expect(row(page)).toContainText("Model a · Installed");
});

test("with no pick of its own, meeting notes follow a new voice typing pick", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "Understand" }).click();
  await row(page).getByRole("button", { name: "All models" }).click();
  await page.getByRole("radio", { name: "Use Model b" }).check();
  await page.getByRole("option", { name: "Meeting notes" }).click();
  await expect(row(page)).toContainText("Model b · Installed");
});

test("removing a model from the meeting list asks for meeting notes' pick back", async ({ page }) => {
  await open(page);
  await row(page).getByRole("button", { name: "All models" }).click();
  await page.getByRole("radio", { name: "Use Model b" }).check();
  await page.getByRole("button", { name: "Remove Model b" }).click();
  expect(await calls(page, "model_delete")).toEqual([{ id: "b", task: "meeting_stt" }]);
  await expect(page.getByRole("radio", { name: "Use Model a" })).toBeChecked(); // back to voice typing's pick
});
