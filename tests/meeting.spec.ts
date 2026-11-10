import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// `meeting_set` fails for Ctrl+Alt+N, like a shortcut another app holds.
async function open(page: Page, notes = { on: false, key: "Ctrl+Alt+KeyM" }) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    meeting_get: () => (${JSON.stringify(notes)}),
    meeting_set: ({ notes }) => { if (notes.key === "Ctrl+Alt+KeyN") throw "Another app is using that shortcut. Pick a different one."; },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Meeting notes" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("off by default: just the switch, and turning it on saves and shows the shortcut", async ({ page }) => {
  await open(page);
  const sw = page.getByRole("switch", { name: "Use meeting notes" });
  await expect(sw).not.toBeChecked();
  await expect(page.getByLabel("Start or stop notes")).toBeHidden();
  await sw.click();
  await expect(page.getByLabel("Start or stop notes")).toHaveValue("Ctrl+Alt+KeyM");
  expect(await calls(page, "meeting_set")).toEqual([{ notes: { on: true, key: "Ctrl+Alt+KeyM" } }]);
});

test("the shortcut can be changed, or removed", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const key = page.getByLabel("Start or stop notes");
  await expect(key.locator("option")).toHaveText(["⌃⌥M", "⌃⌥N", "No shortcut"]);
  await key.selectOption("");
  expect((await calls(page, "meeting_set")).at(-1)).toEqual({ notes: { on: true, key: "" } });
});

test("a shortcut another app holds is refused and the old one stays", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  await page.getByLabel("Start or stop notes").selectOption("Ctrl+Alt+KeyN");
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(page.getByLabel("Start or stop notes")).toHaveValue("Ctrl+Alt+KeyM");
});

const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);

test("the recording pill shows Recording, the timer and Mic live, and goes when it stops", async ({ page }) => {
  await fakeTauri(page, `{}`);
  await page.clock.install({ time: 100_000 });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(8);
  const pill = page.getByRole("status");
  await expect(pill).toBeHidden();
  await emit(page, "meeting", { started: 100_000 - 65_000 });
  await expect(pill).toHaveText("Recording1:05Mic live");
  await page.clock.runFor(1000);
  await expect(pill).toHaveText("Recording1:06Mic live");
  await emit(page, "meeting", { started: null });
  await expect(pill).toBeHidden();
});
