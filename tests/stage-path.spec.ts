import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// The stage path heading Listen, Understand and Write: Listen, Understand, Write, each with what it does now.
const model = (id: string, name: string, installed: boolean) => ({ id, name, vendor: "V", desc: "d", acc: 4, spd: 4, default: id === "p", size: 1e6, min_macos: 0, repo: "r", revision: "x", files: [{}], system: false, runtime: false, installed, needs: null });
const models = (active: string) => JSON.stringify([{ id: "stt", active, models: [model("p", "Model P", true), model("s", "Model S", true), model("q", "Model Q", false)] }, { id: "diar", active: null, models: [] }]);
const providers = [
  { id: 1, kind: "openai", name: "Local host", url: "http://localhost:1/v1", has_key: false, off_device: false },
  { id: 2, kind: "openrouter", name: "Remote host", url: "https://example.com/v1", has_key: true, off_device: true },
];
const LONG = "vendor-with-a-very-long-name/an-extremely-long-model-identifier-that-cannot-possibly-fit:32b-instruct-q4_K_M-and-more";

async function open(page: Page, o: { active?: string; on?: boolean; key?: string; fast?: { provider: number; model: string } | null } = {}) {
  const fast = o.fast === undefined ? { provider: 1, model: "fast-model" } : o.fast;
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => ${models(o.active ?? "p")},
    voice_get: () => ({ on: ${o.on ?? true}, key: ${JSON.stringify(o.key ?? "Alt+Space")}, clean: null, mode: "hold", paste: "clipboard" }),
    voice_set: () => {}, model_select: () => {}, model_download: () => null,
    ai_get: () => ({ providers: ${JSON.stringify(providers)}, main: null, fast: ${JSON.stringify(fast && { ...fast, gone: false })} }),
  }`);
  await page.goto("/settings");
  await expect(page.getByRole("option", { name: "Listen" })).toBeVisible();
}
const strip = (page: Page) => page.getByRole("navigation", { name: "Where your voice goes" });
const stage = (page: Page, name: string | RegExp) => strip(page).getByRole("button", { name });
const header = async (page: Page) => (await page.locator("main header").boundingBox())!.height;
const emit = (page: Page, p: object) => page.evaluate((p) => (window as any).__emit("model-state", p), p);

test("names the three stages and marks the page you are on", async ({ page }) => {
  await open(page);
  await expect(strip(page).getByRole("button")).toHaveText([/^Listen/, /^Understand/, /^Write/]);
  await expect(stage(page, /^Listen/)).toHaveAttribute("aria-current", "step");
  await expect(stage(page, /^Write/)).not.toHaveAttribute("aria-current");
  await stage(page, /^Write/).click();
  await expect(stage(page, /^Write/)).toHaveAttribute("aria-current", "step");
  await expect(stage(page, /^Listen/)).not.toHaveAttribute("aria-current");
  await expect(page.locator("main header h1")).toHaveText("Write");
  await expect(page.locator("main header h1")).toHaveClass(/sr-only/);
  await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
});

test("Listen's value follows the shortcut and Hold or Tap", async ({ page }) => {
  await open(page);
  await expect(stage(page, /^Listen/)).toContainText("Hold ⌥Space");
  await expect(stage(page, /^Listen/).getByRole("img", { name: "Option Space" })).toBeVisible();
  await page.getByRole("radio", { name: "Tap" }).check({ force: true });
  await expect(stage(page, /^Listen/)).toContainText("Tap ⌥Space");
  await page.getByRole("button", { name: "Dictate shortcut: Option Space" }).click();
  await page.keyboard.press("Control+Shift+KeyD");
  await expect(stage(page, /^Listen/)).toContainText("Tap ⌃⇧D");
});

test("no key set reads as words", async ({ page }) => {
  await open(page, { key: "" });
  await expect(stage(page, /^Listen/)).toContainText("Hold no key set");
});

test("Understand's value follows a model pick and a download", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "Understand" }).click();
  await expect(stage(page, /^Understand/)).toContainText("Model P");
  await page.getByRole("button", { name: "All models" }).click();
  await page.getByRole("radio", { name: "Use Model S" }).check({ force: true });
  await page.getByRole("button", { name: "Back" }).click();
  await expect(stage(page, /^Understand/)).toContainText("Model S");
});

test("Understand's value follows a download", async ({ page }) => {
  await open(page, { active: "q" });
  await page.getByRole("option", { name: "Understand" }).click();
  await expect(page.getByText("Model not downloaded", { exact: true })).toBeVisible();
  await emit(page, { id: "q", state: "progress", pct: 42 });
  await expect(stage(page, /^Understand/)).toContainText("Downloading 42%");
  await emit(page, { id: "q", state: "progress", pct: 77 });
  await expect(stage(page, /^Understand/)).toContainText("Downloading 77%");
  await emit(page, { id: "q", state: "done" });
  await expect(stage(page, /^Understand/)).toContainText("Model Q");
  await expect(stage(page, /^Understand/).locator(".ok")).toBeVisible();
});

test("Write's value follows Paste or Type, and shows the Fast model", async ({ page }) => {
  await open(page);
  await stage(page, /^Write/).click();
  await expect(stage(page, /^Write/)).toContainText(/Paste\s*·\s*fast-model/);
  await page.getByRole("radio", { name: "Type" }).check({ force: true });
  await expect(stage(page, /^Write/)).toContainText(/Type\s*·\s*fast-model/);
});

test("the caution pill shows for an off-device Fast model, not for a local one", async ({ page, context }) => {
  await open(page, { fast: { provider: 2, model: "remote-model" } });
  await stage(page, /^Write/).click();
  await expect(stage(page, /^Write/).getByText("Sends text off this Mac")).toBeVisible();
  const local = await context.newPage();
  await open(local);
  await stage(local, /^Write/).click();
  await expect(stage(local, /^Write/)).toContainText("fast-model");
  await expect(local.getByText("Sends text off this Mac")).toHaveCount(0);
  const none = await context.newPage();
  await open(none, { fast: null });
  await stage(none, /^Write/).click();
  await expect(stage(none, /^Write/)).toHaveText(/^Write\s*Paste$/);
});

test("voice typing off: Listen reads Off, the others are dimmed and read Off", async ({ page }) => {
  await open(page, { on: false });
  for (const n of [/^Listen/, /^Understand/, /^Write/]) await expect(stage(page, n)).toContainText("Off");
  await expect(stage(page, /^Listen/)).not.toHaveClass(/off/);
  await expect(stage(page, /^Understand/)).toHaveClass(/off/);
  await expect(stage(page, /^Write/)).toHaveClass(/off/);
  await page.getByRole("switch", { name: "Use voice typing" }).check().catch(() => page.getByLabel("Use voice typing").check({ force: true }));
  await expect(stage(page, /^Listen/)).toContainText("Hold");
  await expect(stage(page, /^Understand/)).toContainText("Model P");
});

test("clicking a stage goes there and pushes history", async ({ page }) => {
  await open(page);
  await stage(page, /^Understand/).click();
  await expect(stage(page, /^Understand/)).toHaveAttribute("aria-current", "step");
  await expect(page.getByRole("option", { name: "Understand" })).toHaveAttribute("aria-selected", "true");
  await page.getByRole("button", { name: "Back" }).click();
  await expect(stage(page, /^Listen/)).toHaveAttribute("aria-current", "step");
});

test("drill-ins and the other pages keep the plain title", async ({ page }) => {
  await open(page);
  await page.getByRole("option", { name: "Understand" }).click();
  await page.getByRole("button", { name: "All models" }).click();
  await expect(strip(page)).toHaveCount(0);
  await expect(page.locator("main header h1")).toBeVisible();
  await expect(page.locator("main header h1")).toHaveText("Speech-to-text · Voice typing");
  await page.getByRole("option", { name: "General" }).click();
  await expect(strip(page)).toHaveCount(0);
  await expect(page.locator("main header h1")).toBeVisible();
});

test("the header is the same height in every state, and long IDs end in an ellipsis", async ({ page, context }) => {
  await open(page);
  const h = await header(page);
  expect(h).toBe(56); // the same toolbar as every other page, so ‹ › and the content never move between pages
  await stage(page, /^Understand/).click();
  await emit(page, { id: "p", state: "progress", pct: 5 });
  expect(await header(page)).toBe(h);
  await stage(page, /^Write/).click();
  await page.getByRole("radio", { name: "Type" }).check({ force: true });
  expect(await header(page)).toBe(h);
  await page.getByRole("option", { name: "Listen" }).click();
  await page.getByRole("radio", { name: "Tap" }).check({ force: true });
  expect(await header(page)).toBe(h);
  await page.getByLabel("Use voice typing").uncheck({ force: true });
  await expect(stage(page, /^Write/)).toContainText("Off");
  expect(await header(page)).toBe(h);

  for (const o of [{ fast: { provider: 2, model: LONG } }, { fast: { provider: 1, model: LONG } }, { on: false }, { key: "" }]) {
    const p = await context.newPage();
    await open(p, o);
    for (const w of [880, 640]) {
      await p.setViewportSize({ width: w, height: 520 });
      for (const n of [/^Listen/, /^Understand/, /^Write/]) {
        await stage(p, n).click();
        expect(await header(p)).toBe(h);
      }
    }
    if ("fast" in o) {
      const clip = stage(p, /^Write/).locator(".clip");
      expect(await clip.evaluate((e) => e.scrollWidth > e.clientWidth && getComputedStyle(e).textOverflow === "ellipsis")).toBe(true);
    }
  }
});
