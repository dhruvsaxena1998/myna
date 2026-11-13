import { expect, test, type Locator, type Page } from "@playwright/test";
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
  await page.getByRole("option", { name: "AI providers" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("fresh install: empty state and both models Not set", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null });
  await expect(page.getByText("No AI providers yet", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add a provider" })).toHaveCount(1);
  await expect(page.getByText("Add one to use summaries, chat and AI cleanup. Myna runs no AI itself.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Add a provider" })).toHaveClass(/primary/);
  await expect(page.getByText("Add a provider first, then pick a model for each.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Not set" })).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Not set" }).first()).toBeDisabled();
});

test("adding an OpenAI-compatible server: placeholders, loopback pill, then the row; its list is prefetched", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: (a) => ({ id: 1, kind: a.kind, name: a.name, url: a.url, has_key: false, off_device: false })`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await expect(page.getByLabel("Kind")).toHaveValue("openai");
  await expect(page.getByLabel("Kind").locator("option")).toHaveText(["OpenAI-compatible", "OpenRouter"]);
  await expect(page.getByText("Any server that speaks the OpenAI API.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Add a provider", level: 1 })).toBeVisible();
  await expect(page.getByLabel("Name")).toHaveValue("");
  await expect(page.getByLabel("Name")).toHaveAttribute("placeholder", "Server on this Mac");
  await expect(page.getByLabel("Address")).toHaveAttribute("placeholder", "http://localhost:11434/v1");
  await expect(page.getByLabel("Key (optional)")).toBeVisible();
  await expect(page.locator("#add-url-h")).toHaveText("Where the server listens, ending in /v1."); // no claim about an address not typed yet
  await page.getByRole("button", { name: "Add provider" }).click();
  await expect(page.getByText("Give it a name.")).toBeVisible();
  await page.getByLabel("Name").fill("Ollama on this Mac");
  await page.getByLabel("Address").fill("http://localhost:11434/v1");
  await expect(page.locator("#add-url-h")).toContainText("Stays on this Mac");
  await page.getByRole("button", { name: "Add provider" }).click();
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
  await expect(page.getByText("Find it at openrouter.ai/keys. Kept in your Mac’s Keychain.")).toBeVisible();
  await expect(page.locator("#add-url-h")).toContainText("Sends text off this Mac");
  await expect(page.getByLabel("Key", { exact: true })).toHaveAttribute("type", "password");
  await page.getByRole("button", { name: "Add provider" }).click();
  await expect(page.getByText("Paste your OpenRouter key.")).toBeVisible();
  await page.getByLabel("Key", { exact: true }).fill("nope");
  await page.getByRole("button", { name: "Add provider" }).click();
  await expect(page.getByText("OpenRouter keys start with")).toBeVisible();
  expect(await calls(page, "ai_add")).toEqual([]);
  await page.getByLabel("Key", { exact: true }).fill("sk-or-v1-abc");
  await page.getByRole("button", { name: "Add provider" }).click();
  const row = page.getByRole("listitem").filter({ hasText: "https://openrouter.ai/api/v1" });
  await expect(row).toContainText("Key in Keychain");
  await expect(row).toContainText("Sends text off this Mac");
});

test("a rejected add shows the backend's words", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: () => { throw "Couldn't save the key to your Keychain: locked" }`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.getByLabel("Name").fill("Ollama");
  await page.getByLabel("Address").fill("http://localhost:11434/v1");
  await page.getByRole("button", { name: "Add provider" }).click();
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
  // Like a native menu: the focused item fills with the accent (blue by default).
  await expect(page.getByRole("menuitemradio").nth(1)).toHaveCSS("background-color", "rgb(10, 100, 216)");
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

test("Test shows the model count or the key's usage; a failure shows the error and no Retry", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR, LAN], main: null, fast: null },
    `ai_test: (() => { let failed = false; return ({ id }) => { if (id === 1 && !failed) { failed = true; throw "Couldn't reach http://localhost:11434/v1: connection refused" } return id === 1 ? { ms: 400, models: 12, used: null, limit: null } : id === 2 ? { ms: 300, models: null, used: 0.1234, limit: 5 } : { ms: 200, models: null, used: 0, limit: null } } })()`);
  const or = page.getByRole("listitem").filter({ hasText: "https://openrouter.ai" });
  await expect(or).toContainText("Not tested yet");
  await or.getByRole("button", { name: "Test" }).click();
  await expect(or).toContainText("Key works · $0.12 used of $5.00 · 0.3 s");
  const ol = page.getByRole("listitem").filter({ hasText: "localhost:11434" });
  await ol.getByRole("button", { name: "Test" }).click();
  await expect(ol).toContainText("Couldn't reach http://localhost:11434/v1: connection refused");
  await expect(ol.getByRole("button", { name: "Retry" })).toHaveCount(0);
  await ol.getByRole("button", { name: "Test" }).click();
  await expect(ol).toContainText("Works · 12 models · 0.4 s");
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
  await expect(page.getByText("Remove OpenRouter and its key?")).toBeVisible();
  await expect(page.getByText("Models that use it become Not set.")).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("button", { name: "Remove OpenRouter" })).toBeFocused();
  await page.getByRole("button", { name: "Remove OpenRouter" }).click();
  await page.getByRole("button", { name: "Remove", exact: true }).click();
  await expect.poll(() => calls(page, "ai_remove")).toEqual([{ id: 2 }]);
  await expect(page.getByRole("status").filter({ hasText: "Removed OpenRouter and its key. Main model and Fast model are now Not set." })).toBeVisible();
  await expect(page.getByRole("button", { name: "Not set" })).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Dismiss" })).toHaveCount(0);
});

test("a failed load shows the error, not a blank page", async ({ page }) => {
  await fakeTauri(page, `{ storage_status: () => "/tmp/Myna", models_list: () => [], ai_get: () => { throw "Couldn't read the AI providers: locked" } }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "AI providers" }).click();
  await expect(page.getByText("Couldn't read the AI providers: locked")).toBeVisible();
});

test("a chosen model missing from the provider's list shows as no longer offered", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: { provider: 2, model: "openai/retired", gone: false }, fast: { provider: 1, model: "old:1b", gone: true } });
  await expect(page.getByText("No longer offered")).toHaveCount(1);
  await expect(page.getByRole("listitem").filter({ hasText: "Fast model" })).toContainText("No longer offered");
  // Opening a picker brings OpenRouter's list, which no longer has the Main model.
  await page.getByRole("button", { name: /OpenRouter · openai\/retired/ }).click();
  await expect(page.getByText("No longer offered")).toHaveCount(2);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: /Ollama on this Mac · old:1b/ })).toBeVisible(); // the choice is kept, never switched
});

const h = async (l: Locator) => (await l.boundingBox())!.height;
const prow = (page: Page, text: string) => page.getByRole("listitem").filter({ hasText: text });

test("provider rows are 80px in every state, and the confirm moves nothing", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: null, fast: null },
    `ai_test: ({ id }) => id === 1 ? new Promise(() => {}) : (() => { throw "Couldn't connect: nothing answered at this address" })()`);
  const a = prow(page, "localhost:11434"), b = prow(page, "openrouter.ai");
  const list = page.locator("section").first().locator("ul");
  expect([await h(a), await h(b)]).toEqual([80, 80]); // untested, with and without a key
  await a.getByRole("button", { name: "Test" }).click();
  await expect(a).toContainText("Checking…");
  expect(await h(a)).toBe(80);
  await b.getByRole("button", { name: "Test" }).click();
  await expect(b).toContainText("Couldn't connect");
  expect(await h(b)).toBe(80);
  const box = await list.boundingBox();
  await page.getByRole("button", { name: "Remove OpenRouter" }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeFocused();
  expect(await h(prow(page, "Remove OpenRouter and its key?"))).toBe(80);
  expect(await list.boundingBox()).toEqual(box);
  // Remove stays red under the pointer.
  const remove = page.getByRole("button", { name: "Remove", exact: true });
  const bg = () => remove.evaluate((n) => getComputedStyle(n).backgroundColor);
  const before = await bg();
  await remove.hover();
  await page.waitForTimeout(250);
  const after = await bg();
  const red = (c: string) => { const m = c.match(/[\d.]+/g)!.map(Number); return m[0] > m[1] * 1.5 && m[0] > m[2] * 1.5; };
  expect(red(before) && red(after)).toBe(true);
  await page.getByRole("button", { name: "Cancel" }).click();
  expect(await list.boundingBox()).toEqual(box);
});

test("the removal line is neutral, not a banner", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: { provider: 2, model: "m", gone: false }, fast: null }, `ai_remove: () => ["main"]`);
  await page.getByRole("button", { name: "Remove OpenRouter" }).click();
  await page.getByRole("button", { name: "Remove", exact: true }).click();
  const note = page.getByText("Removed OpenRouter and its key. Main model is now Not set.");
  await expect(note).toBeVisible();
  expect(await note.evaluate((n) => getComputedStyle(n).color)).toBe(await page.locator("main .desc").first().evaluate((n) => getComputedStyle(n).color));
});

test("Add a provider opens the drill-in; Cancel and Esc go back", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null });
  await page.getByText("Add a provider", { exact: true }).click(); // the whole row opens it
  await expect(page.getByRole("heading", { name: "Add a provider", level: 1 })).toBeVisible();
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("heading", { name: "AI providers", level: 1 })).toBeVisible();
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { name: "AI providers", level: 1 })).toBeVisible();
});

test("a field's one line holds its help or its error without moving the form", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null });
  await page.getByRole("button", { name: "Add a provider" }).click();
  const lines = ["#add-kind-h", "#add-name-h", "#add-url-h", "#add-key-h"];
  const boxes = async () => Promise.all([...lines, "form"].map((l) => page.locator(l).boundingBox()));
  const before = await boxes();
  await page.getByLabel("Name").fill("Ollama on this Mac"); // taken
  await page.getByRole("button", { name: "Add provider" }).click();
  await expect(page.locator("#add-name-h")).toHaveText("You already have one with this name.");
  await expect(page.locator("#add-url-h")).toHaveText("Enter the full address, starting with http:// or https://.");
  expect(await boxes()).toEqual(before);
  await page.getByLabel("Address").fill("http://192.168.1.20:1/v1");
  await expect(page.locator("#add-url-h")).toContainText("Sends text off this Mac");
  expect(await boxes()).toEqual(before);
  await page.getByLabel("Kind").selectOption("openrouter");
  expect(await boxes()).toEqual(before);
});

test("a rejected add keeps the form and its height", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_add: () => { throw "Couldn't save the key to your Keychain: locked" }`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  const box = await page.locator("form").boundingBox();
  await page.getByLabel("Name").fill("X");
  await page.getByLabel("Address").fill("http://localhost:1/v1");
  await page.getByRole("button", { name: "Add provider" }).click();
  await expect(page.getByText("Couldn't save the key to your Keychain: locked")).toBeVisible();
  expect(await page.locator("form").boundingBox()).toEqual(box);
});

test("model rows are 60px, with the host pill of the chosen provider", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR], main: { provider: 2, model: "m", gone: false }, fast: { provider: 1, model: "old", gone: true } });
  const main = prow(page, "Main model"), fast = prow(page, "Fast model");
  expect([await h(main), await h(fast)]).toEqual([60, 60]);
  await expect(main).toContainText("Sends text off this Mac");
  await expect(fast).toContainText("No longer offered");
  await expect(page.getByText("Features without a model stay off.")).toBeVisible();
  await expect(page.getByText("Decision model")).toHaveCount(0);
});

test("a failed pick shows under Models, not Providers", async ({ page }) => {
  await open(page, { providers: [OLLAMA], main: null, fast: null }, `ai_set: () => { throw "Couldn't save the model: database is locked" }`);
  await page.getByRole("button", { name: "Not set" }).first().click();
  await page.getByRole("menuitemradio").first().click();
  const msg = page.getByText("Couldn't save the model: database is locked");
  await expect(msg).toBeVisible();
  await expect(page.locator("section").nth(1).getByText("Couldn't save the model")).toBeVisible();
  await expect(page.getByRole("button", { name: "Not set" }).first()).toBeVisible(); // rolled back
});

test("the host pill is the only amber on the page", async ({ page }) => {
  await open(page, { providers: [OLLAMA, OR, LAN], main: null, fast: { provider: 1, model: "old", gone: true } });
  await page.getByRole("button", { name: "Test" }).first().click();
  const amber = await page.evaluate(() => {
    const probe = document.createElement("i");
    probe.style.color = "var(--caution-text)";
    document.body.append(probe);
    const c = getComputedStyle(probe).color;
    return [...document.querySelectorAll("main .page *")].filter((n) => getComputedStyle(n).color === c).map((n) => n.textContent?.trim());
  });
  expect(new Set(amber)).toEqual(new Set(["Sends text off this Mac"]));
});

test("Test on Add a provider checks the address and key as typed, before saving; typing clears the result", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null }, `ai_test_draft: () => ({ ms: 400, models: 7, used: null, limit: null })`);
  await page.getByRole("button", { name: "Add a provider" }).click();
  await page.getByRole("button", { name: "Test" }).click();
  await expect(page.getByText("Enter the full address, starting with http:// or https://.")).toBeVisible();
  await expect(page.getByText("Give it a name.")).toHaveCount(0); // a name isn't needed to test
  await page.getByLabel("Address").fill("http://localhost:11434/v1");
  await page.getByRole("button", { name: "Test" }).click();
  await expect(page.getByText("Works · 7 models · 0.4 s")).toBeVisible();
  expect(await calls(page, "ai_test_draft")).toEqual([{ kind: "openai", url: "http://localhost:11434/v1", key: "" }]);
  expect(await calls(page, "ai_add")).toEqual([]);
  await page.getByLabel("Address").fill("http://localhost:1/v1");
  await expect(page.getByText("Works · 7 models · 0.4 s")).toHaveCount(0);
});

test("a drill-in's title follows a breadcrumb back to its page", async ({ page }) => {
  await open(page, { providers: [], main: null, fast: null });
  await page.getByRole("button", { name: "Add a provider" }).click();
  const crumbs = page.getByRole("navigation", { name: "Breadcrumbs" });
  await expect(crumbs).toHaveText(/AI providers\s*Add a provider/);
  await crumbs.getByRole("button", { name: "AI providers" }).click();
  await expect(page.getByRole("heading", { name: "AI providers", level: 1 })).toBeVisible();
});
