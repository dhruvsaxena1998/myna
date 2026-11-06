import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const OLLAMA = { id: 1, kind: "openai", name: "Ollama on this Mac", url: "http://localhost:11434/v1", has_key: false, off_device: false };
const OR = { id: 2, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true };
const LAN = { id: 3, kind: "openai", name: "Studio Mac", url: "http://192.168.1.20:11434/v1", has_key: false, off_device: true };

// `state` is the fake backend's get result; `extra` adds or overrides handlers.
async function open(page: Page, state: object, extra = "") {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], ai_get: () => (${JSON.stringify(state)}),
    ai_models: ({ id }) => id === 2
      ? [{ id: "anthropic/claude-sonnet-4.5", input_per_m: 3, output_per_m: 15, context: 200000, images: true }, { id: "google/gemini-2.5-flash", input_per_m: 0.3, output_per_m: 2.5, context: 1000000, images: false }]
      : [{ id: "qwen2.5:7b", input_per_m: null, output_per_m: null, context: null, images: null }, { id: "llama3.2:3b", input_per_m: null, output_per_m: null, context: null, images: null }],
    ai_set: () => null, ${extra} }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "AI provider" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("fresh install: empty state and both models Not set", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null });
  await expect(page.getByRole("heading", { name: "No AI providers yet" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add a provider" })).toHaveCount(1);
  await expect(page.getByText("Not set. Add a provider first.")).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Not set" })).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Not set" }).first()).toBeDisabled();
});

test("adding an OpenAI-compatible server: placeholders, loopback pill, then the row; its list is prefetched", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: (a) => ({ id: 1, kind: a.kind, name: a.name, url: a.url, has_key: false, off_device: false })`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await expect(page.getByLabel("Kind")).toHaveValue("openai");
  await expect(page.getByLabel("Kind").locator("option")).toHaveText(["OpenAI-compatible", "OpenRouter"]);
  await expect(page.getByText("Ollama, LM Studio, oMLX or any server that speaks the OpenAI API.")).toBeVisible();
  await expect(page.getByLabel("Name")).toHaveValue("");
  await expect(page.getByLabel("Name")).toHaveAttribute("placeholder", "Ollama on this Mac");
  await expect(page.getByLabel("Address")).toHaveAttribute("placeholder", "http://localhost:11434/v1");
  await expect(page.getByLabel("Key (optional)")).toBeVisible();
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText("Give it a name.")).toBeVisible();
  await page.getByLabel("Name").fill("Ollama on this Mac");
  await page.getByLabel("Address").fill("http://localhost:11434/v1");
  await expect(page.locator("#add-url-h")).toContainText("Stays on this Mac");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect.poll(() => calls(page, "ai_add")).toEqual([{ kind: "openai", name: "Ollama on this Mac", url: "http://localhost:11434/v1", key: "" }]);
  await expect.poll(() => calls(page, "ai_models")).toEqual([{ id: 1, fresh: true }]);
  const row = page.getByRole("listitem").filter({ hasText: "http://localhost:11434/v1" });
  await expect(row).toContainText("Stays on this Mac");
  await expect(row).toContainText("Not tested yet");
  await expect(page.getByRole("button", { name: "Not set" }).first()).toBeEnabled();
});

test("adding OpenRouter needs a key that looks right; it sends text off this Mac", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: (a) => ({ id: 2, kind: a.kind, name: a.name, url: a.url, has_key: !!a.key, off_device: true })`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.getByLabel("Kind").selectOption("openrouter");
  await expect(page.getByLabel("Name")).toHaveValue("OpenRouter");
  await expect(page.getByLabel("Address")).toHaveValue("https://openrouter.ai/api/v1");
  await expect(page.getByText("Find it at openrouter.ai/keys. Myna keeps it in your Mac’s Keychain.")).toBeVisible();
  await expect(page.locator("#add-url-h")).toContainText("Sends text off this Mac");
  await expect(page.getByLabel("Key", { exact: true })).toHaveAttribute("type", "password");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText("Paste your OpenRouter key.")).toBeVisible();
  await page.getByLabel("Key", { exact: true }).fill("nope");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText("OpenRouter keys start with")).toBeVisible();
  expect(await calls(page, "ai_add")).toEqual([]);
  await page.getByLabel("Key", { exact: true }).fill("sk-or-v1-abc");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  const row = page.getByRole("listitem").filter({ hasText: "https://openrouter.ai/api/v1" });
  await expect(row).toContainText("key in Keychain");
  await expect(row).toContainText("Sends text off this Mac");
});

test("a rejected add shows the backend's words", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: () => { throw "Couldn't save the key to your Keychain: locked" }`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.getByLabel("Name").fill("Ollama");
  await page.getByLabel("Address").fill("http://localhost:11434/v1");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.getByText("Couldn't save the key to your Keychain: locked")).toBeVisible();
});

test("a LAN address shows the off-device pill as you type", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null });
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.getByLabel("Address").fill("http://192.168.1.20:11434/v1");
  await expect(page.locator("#add-url-h")).toContainText("Sends text off this Mac");
  await page.getByLabel("Address").fill("http://[::1]:1234/v1");
  await expect(page.locator("#add-url-h")).toContainText("Stays on this Mac");
});

test("existing LAN provider row shows the off-device pill", async ({ page }) => {
  await open(page, { providers: [OLLAMA, LAN], main: null, fast: null });
  await expect(page.getByRole("listitem").filter({ hasText: "192.168.1.20" })).toContainText("Sends text off this Mac");
});

test("picking Main by search and Fast by keyboard", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: null, fast: null });
  const main = page.locator(".pk-btn").nth(0), fast = page.locator(".pk-btn").nth(1);
  await main.click();
  await expect(page.getByRole("dialog", { name: "Choose Main model" })).toBeVisible();
  await expect(page.getByText("$3.00 in · $15.00 out per 1M tokens · 200K context · reads images")).toBeVisible();
  await expect(page.getByText("Free · runs on your server").first()).toBeVisible();
  await page.getByPlaceholder("Search models").fill("gemini");
  await expect(page.getByRole("menuitemradio")).toHaveCount(1);
  await expect(page.getByText("$0.30 in · $2.50 out per 1M tokens · 1M context")).toBeVisible();
  await page.getByRole("menuitemradio").click();
  await expect.poll(() => calls(page, "ai_set")).toEqual([{ slot: "main", provider: 2, model: "google/gemini-2.5-flash" }]);
  await expect(page.getByRole("button", { name: "OpenRouter · google/gemini-2.5-flash" })).toBeVisible();

  await fast.click();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("menuitemradio").nth(1)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Ollama on this Mac · llama3.2:3b" })).toBeVisible();

  await main.click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(main).toBeFocused();
});

test("a provider whose model list fails says so inline", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: null, fast: null }, `ai_models: ({ id }) => { if (id === 1) throw "nothing is answering at localhost"; return [{ id: "m", input_per_m: null, output_per_m: null, context: null, images: null }] }`);
  await page.getByRole("button", { name: "Not set" }).first().click();
  await expect(page.getByText("Couldn’t list models: nothing is answering at localhost")).toBeVisible();
  await expect(page.getByRole("menuitemradio")).toHaveCount(1);
});

const M = (id: string) => `{ id: "${id}", input_per_m: null, output_per_m: null, context: null, images: null }`;

test("the saved list shows at once, then the live one swaps in without moving the picker", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null },
    `ai_models: ({ fresh }) => fresh ? new Promise((r) => { window.__live = () => r([${M("new-model")}]) }) : [${M("saved-model")}]`);
  await page.getByRole("button", { name: "Not set" }).first().click();
  await expect(page.getByRole("menuitemradio")).toHaveText([/saved-model/]);
  await expect(page.locator(".sk")).toHaveCount(0);
  const box = await page.locator(".pk-list").boundingBox();
  await page.evaluate(() => (window as any).__live());
  await expect(page.getByRole("menuitemradio")).toHaveText([/new-model/]);
  expect(await page.locator(".pk-list").boundingBox()).toEqual(box);
  expect((await calls(page, "ai_models")).map((c: any) => c.fresh)).toEqual([false, true]);
});

test("with no saved list the picker shows skeletons until it arrives", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null },
    `ai_models: ({ fresh }) => fresh ? [${M("m")}] : new Promise((r) => { window.__first = () => r([${M("m")}]) })`);
  await page.getByRole("button", { name: "Not set" }).first().click();
  await expect(page.locator(".sk")).toHaveCount(2);
  await page.evaluate(() => (window as any).__first());
  await expect(page.getByRole("menuitemradio")).toHaveCount(1);
  await expect(page.locator(".sk")).toHaveCount(0);
});

test("a failed refresh keeps the saved list and says so under it", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null },
    `ai_models: ({ fresh }) => { if (fresh) throw "nothing is answering at localhost"; return [${M("saved-model")}] }`);
  await page.getByRole("button", { name: "Not set" }).first().click();
  await expect(page.getByText("Couldn’t refresh: nothing is answering at localhost")).toBeVisible();
  await expect(page.getByRole("menuitemradio")).toHaveText([/saved-model/]);
});

test("Test shows the model count or the key's usage; a failure offers Retry", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR, LAN], main: null, fast: null },
    `ai_test: (() => { let failed = false; return ({ id }) => { if (id === 1 && !failed) { failed = true; throw "Couldn't reach http://localhost:11434/v1: connection refused" } return id === 1 ? { ms: 400, models: 12, used: null, limit: null } : id === 2 ? { ms: 300, models: null, used: 0.1234, limit: 5 } : { ms: 200, models: null, used: 0, limit: null } } })()`);
  const or = page.getByRole("listitem").filter({ hasText: "https://openrouter.ai" });
  await expect(or).toContainText("Not tested yet");
  await or.getByRole("button", { name: "Test" }).click();
  await expect(or).toContainText("Key works · $0.12 used of $5.00 · 0.3 s");
  const ol = page.getByRole("listitem").filter({ hasText: "localhost:11434" });
  await ol.getByRole("button", { name: "Test" }).click();
  await expect(ol).toContainText("Couldn't reach http://localhost:11434/v1: connection refused");
  await ol.getByRole("button", { name: "Retry" }).click();
  await expect(ol).toContainText("Connected · 12 models · 0.4 s");
  const lan = page.getByRole("listitem").filter({ hasText: "192.168.1.20" });
  await lan.getByRole("button", { name: "Test" }).click();
  await expect(lan).toContainText("Key works · $0.00 used · 0.2 s");
});

test("Test says Checking… while it runs", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null }, `ai_test: () => new Promise(() => {})`);
  await page.getByRole("button", { name: "Test" }).click();
  await expect(page.getByText("Checking…")).toBeVisible();
  await expect(page.getByRole("button", { name: "Test" })).toBeDisabled();
});

test("removing a provider confirms, then names the slots now Not set", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: { provider: 2, model: "m" }, fast: { provider: 2, model: "n" } }, `ai_remove: () => ["main", "fast"]`);
  await page.getByRole("button", { name: "Remove OpenRouter" }).click();
  await expect(page.getByText("Remove OpenRouter? Its key is deleted from your Keychain. Models that use it become Not set.")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("button", { name: "Remove OpenRouter" })).toBeFocused();
  await page.getByRole("button", { name: "Remove OpenRouter" }).click();
  await page.getByRole("button", { name: "Remove", exact: true }).click();
  await expect.poll(() => calls(page, "ai_remove")).toEqual([{ id: 2 }]);
  await expect(page.getByRole("status").filter({ hasText: "Removed OpenRouter and its key. Main model and Fast model are now Not set." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Not set" })).toHaveCount(2);
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByText("Removed OpenRouter")).toHaveCount(0);
});

test("a failed load shows the error, not a blank page", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], ai_get: () => { throw "Couldn't read the AI providers: locked" } }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "AI provider" }).click();
  await expect(page.getByText("Couldn't read the AI providers: locked")).toBeVisible();
});
