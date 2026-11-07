import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";

const model = (id: string, extra = {}) => ({
  id, name: id, vendor: "Vendor", desc: `${id} description`, acc: 4, spd: 3, default: false,
  size: 670_479_942, min_macos: 0, repo: "r", revision: "x".repeat(40), files: [{}], system: false, runtime: false, installed: false, needs: null, ...extra,
});

// Fresh install on macOS 14: nothing installed, Desert Ant models blocked.
const tasks = JSON.stringify([
  { id: "stt", active: "parakeet", models: [model("parakeet", { default: true }), model("voz", { min_macos: 15, needs: "Needs macOS 15", files: [] }), model("whisper", { size: 1_624_555_275, installed: true })] },
  { id: "diar", active: "sortformer", models: [model("who", { default: true, min_macos: 15, needs: "Needs macOS 15" }), model("sortformer")] },
]);

async function open(page: import("@playwright/test").Page, select = "() => null", extra = "") {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${tasks}), model_select: ${select}, ${extra} }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Speech models" }).click();
}

test("lists the catalog per task with size and state", async ({ page }) => {
  await open(page);
  const stt = page.getByRole("radiogroup", { name: "Turning speech into text model" });
  await expect(stt.getByRole("radio")).toHaveCount(3);
  await expect(stt.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  await expect(stt.getByRole("listitem").nth(0)).toContainText("670 MB");
  await expect(stt.getByRole("button", { name: "Download parakeet" })).toBeVisible();
  await expect(stt.getByRole("listitem").nth(2)).toContainText("Installed");
  await expect(stt.getByRole("listitem").nth(2)).toContainText("1.6 GB");
  await expect(page.getByRole("img", { name: "Accuracy: Great, 4 out of 5" }).first()).toBeVisible();
  await expect(page.getByRole("radiogroup", { name: "Telling voices apart model" }).getByRole("radio", { name: /sortformer/ })).toBeChecked();
});

test("models that need macOS 15 can't be picked", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("radio", { name: /voz/ })).toBeDisabled();
  await expect(page.getByRole("radio", { name: /who/ })).toBeDisabled();
  await expect(page.getByText("Needs macOS 15")).toHaveCount(2);
});

test("picking a model saves it", async ({ page }) => {
  await open(page);
  await page.getByRole("radio", { name: /whisper/ }).check();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "model_select")))
    .toEqual([["model_select", { task: "stt", id: "whisper" }]]);
});

test("a failed save reverts the pick and says why, without moving the rows", async ({ page }) => {
  await open(page, `() => { throw "Couldn't save the chosen model: disk full" }`);
  const sortformer = page.getByRole("radio", { name: /sortformer/ });
  const before = await sortformer.boundingBox();
  await page.getByRole("radio", { name: /whisper/ }).click(); // .check() would insist it stays checked
  await expect(page.getByText("Couldn't save the chosen model: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  expect(await sortformer.boundingBox()).toEqual(before);
});

test("a failed model list shows the error, not a blank page", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => { throw "Couldn't read the chosen models: locked" } }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Speech models" }).click();
  await expect(page.getByRole("alert")).toContainText("Couldn't read the chosen models: locked");
});

const emit = (page: import("@playwright/test").Page, p: object) => page.evaluate((p) => (window as any).__emit("model-state", p), p);
const calls = (page: import("@playwright/test").Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c[0] === cmd), cmd);
const row = (page: import("@playwright/test").Page) => page.getByRole("radiogroup", { name: "Turning speech into text model" }).getByRole("listitem").nth(0);

test("download shows progress, then Installed, without moving the row", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  const before = await row(page).boundingBox();
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  expect(await calls(page, "model_download")).toEqual([["model_download", { id: "parakeet" }]]);
  await emit(page, { id: "parakeet", state: "progress", pct: 42 });
  await expect(row(page).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
  await expect(row(page)).toContainText("42%");
  expect(await row(page).boundingBox()).toEqual(before);
  await emit(page, { id: "parakeet", state: "done" });
  await expect(row(page)).toContainText("Installed");
  expect(await row(page).boundingBox()).toEqual(before);
});

test("cancel offers Try again", async ({ page }) => {
  await open(page, undefined, "model_download: () => null, model_cancel: () => null");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await row(page).getByRole("button", { name: "Cancel" }).click();
  expect(await calls(page, "model_cancel")).toEqual([["model_cancel", { id: "parakeet" }]]);
  await emit(page, { id: "parakeet", state: "canceled" });
  await expect(row(page).getByRole("button", { name: "Download parakeet" })).toContainText("Try again");
});

test("a checksum failure is shown and can be retried", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await emit(page, { id: "parakeet", state: "failed", error: "parakeet didn't download correctly (the checksum didn't match). Try again." });
  await expect(page.getByText("the checksum didn't match")).toBeVisible();
  await expect(row(page).getByRole("button", { name: "Download parakeet" })).toContainText("Try again");
});

test("a download that can't start says why and stays a Download button", async ({ page }) => {
  await open(page, undefined, `model_download: () => { throw "parakeet is already downloading." }`);
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await expect(page.getByText("already downloading")).toBeVisible();
  await expect(row(page).getByRole("progressbar")).toHaveCount(0);
});

test("Remove uninstalls a model", async ({ page }) => {
  await open(page, undefined, "model_delete: () => null");
  const whisper = page.getByRole("radiogroup", { name: "Turning speech into text model" }).getByRole("listitem").nth(2);
  await whisper.getByRole("button", { name: "Remove whisper" }).click();
  expect(await calls(page, "model_delete")).toEqual([["model_delete", { id: "whisper" }]]);
  await expect(whisper.getByRole("button", { name: "Download whisper" })).toBeVisible();
});

test("removing the chosen model selects the one that takes over", async ({ page }) => {
  await open(page, undefined, `model_delete: () => "parakeet"`);
  await page.getByRole("radio", { name: /whisper/ }).check();
  await page.getByRole("button", { name: "Remove whisper" }).click();
  await expect(page.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  await expect(page.getByRole("button", { name: "Download whisper" })).not.toContainText("Try again");
});

test("an installed model that won't load says so in its row until it loads", async ({ page }) => {
  await open(page);
  await emit(page, { id: "whisper", state: "failed", error: "Couldn't load whisper: the file is damaged." });
  await expect(page.getByText("Couldn't load whisper: the file is damaged.")).toBeVisible();
  await expect(page.getByRole("radiogroup", { name: "Turning speech into text model" }).getByRole("listitem").nth(2)).toContainText("Installed");
  await emit(page, { id: "whisper", state: "loaded" });
  await expect(page.getByText("Couldn't load whisper")).toHaveCount(0);
});

test("a model with no downloadable files says coming soon (Desert Ant, until ticket 03.4)", async ({ page }) => {
  const t = JSON.stringify([{ id: "stt", active: "voz", models: [model("voz", { min_macos: 15, files: [] })] }, { id: "diar", active: null, models: [] }]);
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${t}) }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Speech models" }).click();
  await expect(page.getByText("Coming soon")).toBeVisible();
  await expect(page.getByRole("button", { name: /Download/ })).toHaveCount(0);
});

test("download progress survives switching to another page and back", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await emit(page, { id: "parakeet", state: "progress", pct: 30 });
  await page.getByRole("option", { name: "About" }).click();
  await emit(page, { id: "parakeet", state: "progress", pct: 55 }); // keeps arriving while away
  await page.getByRole("option", { name: "Speech models" }).click();
  await expect(row(page).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "55");
  await page.getByRole("option", { name: "About" }).click();
  await emit(page, { id: "parakeet", state: "done" });
  await page.getByRole("option", { name: "Speech models" }).click();
  await expect(row(page)).toContainText("Installed");
});

test("Cancel and Try again respond at once, before the backend answers", async ({ page }) => {
  await open(page, undefined, "model_download: () => new Promise(() => {}), model_cancel: () => new Promise(() => {})");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await expect(row(page).getByRole("button", { name: "Cancel" })).toBeVisible();
  await row(page).getByRole("button", { name: "Cancel" }).click();
  await expect(row(page).getByRole("button", { name: "Download parakeet" })).toContainText("Try again");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await expect(row(page).getByRole("progressbar")).toBeVisible();
});

test("unload time shows the saved choice, saves a new one, and reverts a failed save", async ({ page }) => {
  await open(page, "() => null", `stt_unload_get: () => 900, stt_unload_set: ({ secs }) => { if (secs === -1) throw "Couldn't save when to unload the model: locked" }`);
  const unload = page.getByLabel("Free up memory when idle");
  await expect(unload).toHaveValue("900");
  await unload.selectOption({ label: "After 1 hour" });
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "stt_unload_set")))
    .toEqual([["stt_unload_set", { secs: 3600 }]]);
  await unload.selectOption({ label: "Never" });
  await expect(page.getByText("Couldn't save when to unload the model: locked")).toBeVisible();
  await expect(unload).toHaveValue("3600");
});

const apple = (extra = {}) => model("apple", { name: "Apple Speech", size: 0, min_macos: 26, system: true, files: [], desc: "Built into macOS. Listens in English (India).", ...extra });
async function openApple(page: import("@playwright/test").Page, a: object, extra = "") {
  const t = JSON.stringify([{ id: "stt", active: "apple", models: [apple(a), model("parakeet", { default: true })] }, { id: "diar", active: null, models: [] }]);
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${t}), ${extra} }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Speech models" }).click();
  return row(page);
}

test("Apple Speech, when macOS has its model, says its language with no size and no Remove", async ({ page }) => {
  const r = await openApple(page, { installed: true });
  await expect(r).toContainText("Listens in English (India).");
  await expect(r).toContainText("Installed");
  await expect(r).not.toContainText("MB");
  await expect(r.getByRole("button")).toHaveCount(0);
  await expect(page.getByRole("radio", { name: /Apple Speech/ })).toBeChecked();
});

test("Apple Speech downloads through macOS with progress and no Cancel, without moving the row", async ({ page }) => {
  const r = await openApple(page, {}, "model_download: () => null");
  const before = await r.boundingBox();
  await r.getByRole("button", { name: "Download Apple Speech" }).click();
  await emit(page, { id: "apple", state: "progress", pct: 40 });
  await expect(r.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "40");
  await expect(r.getByRole("button")).toHaveCount(0);
  expect(await r.boundingBox()).toEqual(before);
  await emit(page, { id: "apple", state: "done" });
  await expect(r).toContainText("Installed");
  expect(await r.boundingBox()).toEqual(before);
});

test("Apple Speech in a language Apple doesn't support can't be picked", async ({ page }) => {
  await openApple(page, { needs: "Needs a language Apple supports", desc: "Built into macOS." });
  await expect(page.getByRole("radio", { name: /Apple Speech/ })).toBeDisabled();
  await expect(page.getByText("Needs a language Apple supports")).toBeVisible();
});

// A longer speech-to-text list from three providers; parakeet is chosen.
const many = JSON.stringify([
  { id: "stt", active: "parakeet", models: [
    model("parakeet", { name: "Parakeet TDT v3", vendor: "NVIDIA", default: true, acc: 5, spd: 5, size: 670 }),
    model("apple", { name: "Apple Speech", vendor: "Apple", acc: 4, spd: 5, size: 0, system: true, files: [] }),
    model("w-small", { name: "Whisper small", vendor: "OpenAI", acc: 3, spd: 4, size: 488 }),
    model("w-large", { name: "Whisper large-v3", vendor: "OpenAI", acc: 4, spd: 2, size: 1081 }),
    model("w-turbo", { name: "Whisper large-v3-turbo", vendor: "OpenAI", acc: 4, spd: 3, size: 1624 }),
  ] },
  { id: "diar", active: "sortformer", models: [model("sortformer", { vendor: "NVIDIA" })] },
]);
async function openMany(page: import("@playwright/test").Page) {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${many}), model_select: () => null }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Speech models" }).click();
}
const stt = (page: import("@playwright/test").Page) => page.getByRole("radiogroup", { name: "Turning speech into text model" });
const names = (page: import("@playwright/test").Page) => stt(page).getByRole("radio").evaluateAll((rs) => rs.map((r) => (r as HTMLInputElement).value));
const headings = (page: import("@playwright/test").Page) => stt(page).getByRole("heading").allTextContents();

test("speech-to-text models are grouped by provider in catalog order", async ({ page }) => {
  await openMany(page);
  expect(await headings(page)).toEqual(["NVIDIA", "Apple", "OpenAI"]);
  await expect(stt(page).getByRole("list", { name: "OpenAI" }).getByRole("listitem")).toHaveCount(3);
  // The voices list stays plain: no search, no sort, no headings.
  const diar = page.locator("section", { has: page.getByRole("heading", { name: "Telling voices apart" }) });
  await expect(diar.getByRole("searchbox")).toHaveCount(0);
  await expect(diar.getByRole("heading", { level: 3 })).toHaveCount(0);
});

test("fuzzy search on the name forgives a typo and never hides the chosen model", async ({ page }) => {
  await openMany(page);
  const box = page.getByRole("searchbox", { name: "Search models" });
  await box.fill("wisper turbo");
  expect(await names(page)).toEqual(["parakeet", "w-turbo"]);
  expect(await headings(page)).toEqual(["NVIDIA", "OpenAI"]);
  await box.fill("zzz");
  expect(await names(page)).toEqual(["parakeet"]);
  await expect(page.getByText("No model matches “zzz”.")).toBeVisible();
  await box.press("Escape");
  expect(await names(page)).toHaveLength(5);
  await expect(page.getByText("No model matches")).toHaveCount(0);
});

test("sort works inside each provider and keeps the providers in place", async ({ page }) => {
  await openMany(page);
  const sort = page.getByRole("combobox", { name: "Sort by" });
  await expect(sort).toHaveValue("rec");
  await sort.selectOption("spd");
  expect(await names(page)).toEqual(["parakeet", "apple", "w-small", "w-turbo", "w-large"]);
  await sort.selectOption("size");
  expect(await names(page)).toEqual(["parakeet", "apple", "w-small", "w-large", "w-turbo"]);
  await sort.selectOption("acc");
  expect(await names(page)).toEqual(["parakeet", "apple", "w-large", "w-turbo", "w-small"]);
  expect(await headings(page)).toEqual(["NVIDIA", "Apple", "OpenAI"]);
});
