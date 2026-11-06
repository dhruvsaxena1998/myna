import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const OLLAMA = { id: 1, kind: "ollama", name: "Ollama on this Mac", url: "http://localhost:11434/v1", has_key: false, off_device: false };
const OR = { id: 2, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true };
const LAN = { id: 3, kind: "custom", name: "Studio Mac", url: "http://192.168.1.20:11434/v1", has_key: false, off_device: true };

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

test("adding Ollama: loopback pill, then the row", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: (a) => ({ id: 1, kind: a.kind, name: a.name, url: a.url, has_key: false, off_device: false })`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await expect(page.getByLabel("Name")).toHaveValue("Ollama on this Mac");
  await expect(page.getByLabel("Key (optional)")).toBeVisible();
  await expect(page.locator("#add-url-h")).toContainText("Stays on this Mac");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect.poll(() => calls(page, "ai_add")).toEqual([{ kind: "ollama", name: "Ollama on this Mac", url: "http://localhost:11434/v1", key: "" }]);
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

test("Test shows reply, time and cost; a failure offers Retry", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: { provider: 2, model: "anthropic/claude-sonnet-4.5" }, fast: null },
    `ai_test: (() => { let failed = false; return ({ id }) => { if (id === 1 && !failed) { failed = true; throw "nothing is answering at localhost" } return id === 2 ? { model: "m", reply: "Hello!", ms: 800, cost: { kind: "usd", usd: 0.00036 } } : { model: "m", reply: "Hi", ms: 1500, cost: { kind: "free" } } } })()`);
  const or = page.getByRole("listitem").filter({ hasText: "https://openrouter.ai" });
  await or.getByRole("button", { name: "Test" }).click();
  await expect(or).toContainText("“Hello!” · 0.8 s · $0.00036");
  const ol = page.getByRole("listitem").filter({ hasText: "localhost:11434" });
  await ol.getByRole("button", { name: "Test" }).click();
  await expect(ol).toContainText("Couldn’t get a reply: nothing is answering at localhost");
  await ol.getByRole("button", { name: "Retry" }).click();
  await expect(ol).toContainText("“Hi” · 1.5 s · Free · runs on your server");
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
