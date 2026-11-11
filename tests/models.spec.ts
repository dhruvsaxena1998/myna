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

// Understand › All models: the speech-to-text list's drill-in. Speaker diarization has its own drill-in on Meeting notes (toVoices).
async function toModels(page: import("@playwright/test").Page) {
  await page.getByRole("option", { name: "Understand" }).click();
  await page.getByRole("button", { name: "All models" }).click();
}

async function toVoices(page: import("@playwright/test").Page) {
  await page.getByRole("option", { name: "Meeting notes" }).click();
  await page.getByRole("listitem").filter({ hasText: "Speaker diarization" }).getByRole("button", { name: "All models" }).click();
}

async function open(page: import("@playwright/test").Page, select = "() => null", extra = "") {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${tasks}), model_select: ${select}, meeting_get: () => ({ on: true, key: "Ctrl+Alt+KeyM", keep_teams: false }), ${extra} }`);
  await page.goto("/settings");
  await toModels(page);
}

test("lists the catalog with size and state, and takes Recommended from the default flag", async ({ page }) => {
  await open(page);
  const list = stt(page);
  await expect(list.getByRole("radio")).toHaveCount(3);
  await expect(list.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  await expect(list.getByRole("listitem").nth(0)).toContainText("Vendor · Recommended · 670 MB");
  await expect(list.getByRole("button", { name: "Download parakeet" })).toBeVisible();
  await expect(list.getByRole("listitem").nth(2)).toContainText("Vendor · 1.6 GB");
  await expect(list.getByRole("listitem").nth(2)).not.toContainText("Recommended");
  await expect(list.getByRole("listitem").nth(2)).toContainText("Installed");
  await expect(page.getByText("Recommended")).toHaveCount(1);
  // The design has no accuracy or speed dots, no Sort by and no vendor headings.
  await expect(page.getByRole("img", { name: /Accuracy/ })).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Sort by" })).toHaveCount(0);
  await expect(list.getByRole("heading")).toHaveCount(0);
  await expect(page.getByText("Used for voice typing. Downloads are shared with meeting notes.")).toBeVisible();
  await expect(page.getByText("Sizes are download sizes. A model downloads only when you ask.")).toBeVisible();
});

test("every model is one 60px row, and the radio is off until the model is installed", async ({ page }) => {
  await open(page);
  const heights = await stt(page).getByRole("listitem").evaluateAll((els) => els.map((e) => e.getBoundingClientRect().height));
  expect(heights).toEqual([60, 60, 60]);
  await expect(page.getByRole("radio", { name: /parakeet/ })).toBeDisabled(); // not installed yet
  await expect(page.getByRole("radio", { name: /whisper/ })).toBeEnabled();
});

test("models that need macOS 15 can't be picked", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("radio", { name: /voz/ })).toBeDisabled();
  await expect(page.getByText("Needs macOS 15")).toHaveCount(1);
  await toVoices(page);
  await expect(page.getByRole("radio", { name: /who/ })).toBeDisabled();
  await expect(page.getByRole("radiogroup", { name: "Speaker diarization model" }).getByRole("radio", { name: /sortformer/ })).toBeChecked();
  await expect(page.getByText("Needs macOS 15")).toHaveCount(1);
});

test("picking a model saves it", async ({ page }) => {
  await open(page);
  await page.getByRole("radio", { name: /whisper/ }).check();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "model_select")))
    .toEqual([["model_select", { task: "stt", id: "whisper" }]]);
});

test("a failed save reverts the pick and says why, without moving the rows", async ({ page }) => {
  await open(page, `() => { throw "Couldn't save the chosen model: disk full" }`);
  const voz = page.getByRole("radio", { name: /voz/ });
  const before = await voz.boundingBox();
  await page.getByRole("radio", { name: /whisper/ }).click(); // .check() would insist it stays checked
  await expect(page.getByText("Couldn't save the chosen model: disk full")).toBeVisible();
  await expect(page.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  expect(await voz.boundingBox()).toEqual(before);
});

test("a failed model list shows the error, not a blank page", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => { throw "Couldn't read the chosen models: locked" } }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Understand" }).click();
  await expect(page.getByRole("alert")).toContainText("Couldn't read the chosen models: locked");
});

const emit = (page: import("@playwright/test").Page, p: object) => page.evaluate((p) => (window as any).__emit("model-state", p), p);
const calls = (page: import("@playwright/test").Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c[0] === cmd), cmd);
const stt = (page: import("@playwright/test").Page) => page.getByRole("radiogroup", { name: "Speech-to-text model" });
const row = (page: import("@playwright/test").Page) => stt(page).getByRole("listitem").nth(0);
// Every row's top and height, and where each status column starts: none may move when a download changes state.
const layout = (page: import("@playwright/test").Page) =>
  stt(page).getByRole("listitem").evaluateAll((els) => els.map((e) => { const r = e.getBoundingClientRect(), s = e.querySelector(".mstate")!.getBoundingClientRect(); return [r.top, r.height, s.left, s.width]; }));

test("download shows progress, then Installed, without moving the row", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  const before = await layout(page);
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  expect(await calls(page, "model_download")).toEqual([["model_download", { id: "parakeet" }]]);
  await emit(page, { id: "parakeet", state: "progress", pct: 42 });
  await expect(row(page).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
  await expect(row(page)).toContainText("42%");
  expect(await layout(page)).toEqual(before);
  await emit(page, { id: "parakeet", state: "done" });
  await expect(row(page)).toContainText("Installed");
  expect(await layout(page)).toEqual(before);
});

test("stopping a download offers Retry", async ({ page }) => {
  await open(page, undefined, "model_download: () => null, model_cancel: () => null");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await row(page).getByRole("button", { name: "Stop downloading parakeet" }).click();
  expect(await calls(page, "model_cancel")).toEqual([["model_cancel", { id: "parakeet" }]]);
  await emit(page, { id: "parakeet", state: "canceled" });
  await expect(row(page).getByRole("button", { name: "Retry parakeet" })).toContainText("Retry");
});

test("a failed download says why in its own row, on one line, and can be retried", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  const before = await layout(page);
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  const msg = "parakeet didn't download correctly (the checksum didn't match). Try again.";
  await emit(page, { id: "parakeet", state: "failed", error: msg });
  await expect(row(page).getByText(msg)).toBeVisible();
  const color = (t: import("@playwright/test").Locator) => t.evaluate((e) => getComputedStyle(e).color);
  expect(await color(row(page).getByText(msg))).not.toBe(await color(stt(page).getByRole("listitem").nth(1).getByText("voz description"))); // --danger-text
  await expect(stt(page).getByText(msg)).toHaveCount(1); // no other row, no line under the list
  await expect(page.getByText("parakeet description")).toHaveCount(0); // the message replaces this row's description
  await expect(row(page).getByRole("button", { name: "Retry parakeet" })).toContainText("Retry");
  expect(await layout(page)).toEqual(before);
});

test("a download that can't start says why and stays a Download button", async ({ page }) => {
  await open(page, undefined, `model_download: () => { throw "parakeet is already downloading." }`);
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await expect(page.getByText("already downloading")).toBeVisible();
  await expect(row(page).getByRole("progressbar")).toHaveCount(0);
});

test("Remove uninstalls a model", async ({ page }) => {
  await open(page, undefined, "model_delete: () => null");
  const whisper = stt(page).getByRole("listitem").nth(2);
  await whisper.getByRole("button", { name: "Remove whisper" }).click();
  expect(await calls(page, "model_delete")).toEqual([["model_delete", { id: "whisper", task: "stt" }]]);
  await expect(whisper.getByRole("button", { name: "Download whisper" })).toBeVisible();
});

test("removing the chosen model selects the one that takes over", async ({ page }) => {
  await open(page, undefined, `model_delete: () => "parakeet"`);
  await page.getByRole("radio", { name: /whisper/ }).check();
  await page.getByRole("button", { name: "Remove whisper" }).click();
  await expect(page.getByRole("radio", { name: /parakeet/ })).toBeChecked();
  await expect(page.getByRole("button", { name: "Download whisper" })).not.toContainText("Retry");
});

test("an installed model that won't load says so in its row until it loads", async ({ page }) => {
  await open(page);
  await emit(page, { id: "whisper", state: "failed", error: "Couldn't load whisper: the file is damaged." });
  await expect(page.getByText("Couldn't load whisper: the file is damaged.")).toBeVisible();
  await expect(stt(page).getByRole("listitem").nth(2)).toContainText("Installed");
  await emit(page, { id: "whisper", state: "loaded" });
  await expect(page.getByText("Couldn't load whisper")).toHaveCount(0);
});

test("a model with no downloadable files says coming soon (Desert Ant, until ticket 03.4)", async ({ page }) => {
  const t = JSON.stringify([{ id: "stt", active: "voz", models: [model("voz", { min_macos: 15, files: [] })] }, { id: "diar", active: null, models: [] }]);
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${t}) }`);
  await page.goto("/settings");
  await toModels(page);
  await expect(page.getByText("Coming later")).toBeVisible();
  await expect(page.getByRole("button", { name: /Download/ })).toHaveCount(0);
});

test("download progress survives switching to another page and back", async ({ page }) => {
  await open(page, undefined, "model_download: () => null");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await emit(page, { id: "parakeet", state: "progress", pct: 30 });
  await page.getByRole("option", { name: "General" }).click();
  await emit(page, { id: "parakeet", state: "progress", pct: 55 }); // keeps arriving while away
  await toModels(page);
  await expect(row(page).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "55");
  await page.getByRole("option", { name: "General" }).click();
  await emit(page, { id: "parakeet", state: "done" });
  await toModels(page);
  await expect(row(page)).toContainText("Installed");
});

test("Stop and Retry respond at once, before the backend answers", async ({ page }) => {
  await open(page, undefined, "model_download: () => new Promise(() => {}), model_cancel: () => new Promise(() => {})");
  await row(page).getByRole("button", { name: "Download parakeet" }).click();
  await expect(row(page).getByRole("button", { name: "Stop downloading parakeet" })).toBeVisible();
  await row(page).getByRole("button", { name: "Stop downloading parakeet" }).click();
  await expect(row(page).getByRole("button", { name: "Retry parakeet" })).toContainText("Retry");
  await row(page).getByRole("button", { name: "Retry parakeet" }).click();
  await expect(row(page).getByRole("progressbar")).toBeVisible();
});

test("unload time shows the saved choice, saves a new one, and reverts a failed save", async ({ page }) => {
  await open(page, "() => null", `stt_unload_get: () => 900, stt_unload_set: ({ secs }) => { if (secs === -1) throw "Couldn't save when to unload the model: locked" }`);
  await page.getByRole("button", { name: "Back" }).click(); // the unload row is on Understand itself
  const unload = page.getByLabel("Unload model when idle");
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
  await toModels(page);
  return row(page);
}

test("Apple Speech, when macOS has its model, says its language with no size and no Remove", async ({ page }) => {
  const r = await openApple(page, { installed: true });
  await expect(r).toContainText("Listens in English (India).");
  await expect(r).toContainText("Built in");
  await expect(r).not.toContainText("MB");
  await expect(r.getByRole("button")).toHaveCount(0);
  await expect(page.getByRole("radio", { name: /Apple Speech/ })).toBeChecked();
});

test("Apple Speech downloads through macOS with progress and no Cancel, without moving the row", async ({ page }) => {
  const r = await openApple(page, {}, "model_download: () => null");
  const before = await layout(page);
  await r.getByRole("button", { name: "Download Apple Speech" }).click();
  await emit(page, { id: "apple", state: "progress", pct: 40 });
  await expect(r.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "40");
  await expect(r.getByRole("button")).toHaveCount(0);
  expect(await layout(page)).toEqual(before);
  await emit(page, { id: "apple", state: "done" });
  await expect(r).toContainText("Built in");
  expect(await layout(page)).toEqual(before);
});

test("Apple Speech in a language Apple doesn't support can't be picked", async ({ page }) => {
  await openApple(page, { needs: "Needs a language Apple supports", desc: "Built into macOS." });
  await expect(page.getByRole("radio", { name: /Apple Speech/ })).toBeDisabled();
  await expect(page.getByText("Needs a language Apple supports")).toBeVisible();
});

// A longer speech-to-text list from three providers; parakeet is chosen.
const many = JSON.stringify([
  { id: "stt", active: "parakeet", models: [
    model("parakeet", { name: "Parakeet TDT v3", vendor: "NVIDIA", default: true, acc: 5, spd: 5, size: 670_000_000 }),
    model("apple", { name: "Apple Speech", vendor: "Apple", acc: 4, spd: 5, size: 0, system: true, files: [] }),
    model("w-small", { name: "Whisper small", vendor: "OpenAI", acc: 3, spd: 4, size: 488_000_000 }),
    model("w-large", { name: "Whisper large-v3", vendor: "OpenAI", acc: 4, spd: 2, size: 1081_000_000 }),
    model("w-turbo", { name: "Whisper large-v3-turbo", vendor: "OpenAI", acc: 4, spd: 3, size: 1624_000_000 }),
  ] },
  { id: "diar", active: "sortformer", models: [model("sortformer", { vendor: "NVIDIA" })] },
]);
async function openMany(page: import("@playwright/test").Page) {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${many}), model_select: () => null }`);
  await page.goto("/settings");
  await toModels(page);
}
const names = (page: import("@playwright/test").Page) => stt(page).getByRole("radio").evaluateAll((rs) => rs.map((r) => (r as HTMLInputElement).value));

test("speech-to-text models stay in catalog order, with no vendor headings or sort", async ({ page }) => {
  await openMany(page);
  expect(await names(page)).toEqual(["parakeet", "apple", "w-small", "w-large", "w-turbo"]);
  await expect(stt(page).getByRole("heading")).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Sort by" })).toHaveCount(0);
});

test("fuzzy search on the name forgives a typo and never hides the chosen model", async ({ page }) => {
  await openMany(page);
  const box = page.getByRole("searchbox", { name: "Search models" });
  await box.fill("wisper turbo");
  expect(await names(page)).toEqual(["parakeet", "w-turbo"]);
  await box.fill("zzz");
  expect(await names(page)).toEqual(["parakeet"]);
  await expect(page.getByText("No models match.")).toBeVisible();
  await box.press("Escape");
  expect(await names(page)).toHaveLength(5);
  await expect(page.locator("p.none")).toHaveText("");
});

test("the no-match line is reserved: it takes its height before any search", async ({ page }) => {
  await openMany(page);
  const none = page.locator("p.none");
  const before = (await none.boundingBox())!;
  expect(before.height).toBeGreaterThan(0);
  await page.getByRole("searchbox", { name: "Search models" }).fill("zzz");
  await expect(none).toHaveText("No models match.");
  expect(await none.boundingBox()).toEqual({ ...before, y: before.y - 4 * 60 }); // only the four hidden rows moved it
});

test("the speaker diarization list is short, so it has no search, and says what it does", async ({ page }) => {
  await open(page);
  await toVoices(page);
  await expect(page.getByRole("heading", { name: "Speaker diarization", level: 1 })).toBeVisible();
  await expect(page.getByRole("searchbox")).toHaveCount(0);
  await expect(page.getByText("Works out who said what. Runs on this Mac. Your own voice is always “Me”.")).toBeVisible();
  await expect(page.getByRole("radio")).toHaveCount(2);
});

test("a task on Meeting notes shows the vendor after the name, with a space", async ({ page }) => {
  await open(page);
  await toVoices(page);
  await expect(page.getByRole("radiogroup", { name: "Speaker diarization model" }).getByText("who Vendor")).toBeVisible();
});

test("Understand says which model is picked, with its state, and opens the list from anywhere on the row", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => (${many}), model_download: () => null }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Understand" }).click();
  const r = page.getByRole("listitem").filter({ hasText: "Speech-to-text model" });
  await expect(r).toContainText("Parakeet TDT v3 · Not downloaded · 670 MB");
  await expect(r.getByRole("button", { name: "Download" })).toBeVisible();
  await emit(page, { id: "parakeet", state: "progress", pct: 30 });
  await emit(page, { id: "w-small", state: "failed", error: "nope" });
  await expect(r).toContainText("Parakeet TDT v3 · Downloading 30%");
  await expect(r).toContainText("1 downloading, 1 failed");
  await expect(r.getByRole("button", { name: "Download" })).toHaveCount(0);
  await r.getByText("Speech-to-text model").click();
  await expect(page.getByRole("heading", { name: "Speech-to-text · Voice typing", level: 1 })).toBeVisible();
});
