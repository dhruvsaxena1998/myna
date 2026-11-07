import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const BASE = "Fix punctuation, capitals and grammar slips.";
const SHIPPED: Record<string, string> = { default: BASE, code: BASE + " Keep code names.", message: BASE + " Keep it casual.", email: BASE + " Use full sentences." };
const style = (id: string, name: string, apps: string[], text = SHIPPED[id] ?? BASE, extra = {}) => ({ id, name, apps, text, shipped: SHIPPED[id] ?? null, newer: false, ...extra });
const STYLES = [style("default", "Default", []), style("code", "Code", ["Xcode", "Cursor"]), style("message", "Message", ["Slack"], "Keep it casual and short.", { newer: true }), style("email", "Email", ["Mail"])];

// `voice_set` fails for any shortcut using Ctrl+Alt, like a shortcut another app holds.
// `__globe` stands for the 🌐 key's setting being Do Nothing.
async function open(page: Page, key = "Alt+Space", globe = false, extra: { clean?: string | null; ai?: object } = {}) {
  await page.addInitScript((g) => ((window as any).__globe = g), globe);
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    voice_get: () => ({ on: true, key: "${key}", clean: ${JSON.stringify(extra.clean ?? null)}, mode: "hold", paste: "clipboard" }),
    cleanup_get: () => ({ styles: window.__styles ??= ${JSON.stringify(STYLES)}, apps: ["Cursor", "Mail", "Notes", "Slack", "Xcode"] }),
    cleanup_spell: () => "Postgres, Myna",
    cleanup_set: ({ styles }) => {
      if (styles.some((s) => s.text === "boom")) throw "Couldn't save the cleanup styles: disk full";
      return window.__styles = styles.map((s) => { const sh = ${JSON.stringify(SHIPPED)}[s.id] ?? null; return { ...s, text: s.text.trim() || sh || ${JSON.stringify(SHIPPED.default)}, shipped: sh, newer: !!window.__newer?.[s.id] && s.text !== sh }; });
    },
    ai_get: () => (${JSON.stringify(extra.ai ?? { providers: [], main: null, fast: null })}),
    globe_ok: () => window.__globe, globe_open: () => {},
    voice_set: ({ voice }) => { if (voice.key.startsWith("Ctrl+Alt")) throw "Another app is using that shortcut. Pick a different one."; },
    voice_pause: () => {},
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Voice typing" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("shows the shortcut", async ({ page }) => {
  await open(page);
  await expect(page.getByLabel("Current shortcut: Option Space")).toHaveText("⌥Space");
});

test("Change records a new shortcut and saves it; the old one is paused meanwhile", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await expect(page.getByRole("button", { name: "Press a key…" })).toBeFocused();
  await expect(page.getByText("Press the key or keys you want to use. Esc cancels.")).toBeVisible();
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("KeyD");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await expect(page.getByLabel("Current shortcut: Control Shift D")).toHaveText("⌃⇧D");
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }]);
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Ctrl+Shift+KeyD", clean: null, mode: "hold", paste: "clipboard" } }]);
});

test("Esc cancels and gives the old shortcut back", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByLabel("Current shortcut: Option Space")).toBeVisible();
  await expect(page.getByRole("button", { name: "Change" })).toBeFocused();
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }, { paused: false }]);
  expect(await calls(page, "voice_set")).toEqual([]);
});

test("a key that types on its own is refused, and recording goes on", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("KeyD");
  await expect(page.getByText("Add ⌘, ⌥ or ⌃, so the key still types normally.")).toBeVisible();
  await page.keyboard.press("F5");
  await expect(page.getByLabel("Current shortcut: F5")).toBeVisible();
  await expect(page.getByText("Add ⌘")).toBeHidden();
});

test("a shortcut another app holds says so and keeps the old one", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(page.getByLabel("Current shortcut: Option Space")).toBeVisible();
});

test("Tap to start and stop saves, says how it stops, and the preview follows", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Release to paste");
  await page.getByText("Tap to start and stop").click();
  await expect(page.getByText("Press once to start and again to stop. Stops by itself after 5 minutes.")).toBeVisible();
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Press again to paste");
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: null, mode: "tap", paste: "clipboard" } }]);
});

test("turning voice typing off hides its settings; on brings them back", async ({ page }) => {
  await open(page);
  const sw = page.getByRole("switch", { name: "Use voice typing" });
  await sw.click();
  await expect(sw).not.toBeChecked();
  for (const h of ["Shortcut", "Pill", "Permissions"]) await expect(page.getByRole("heading", { name: h })).toBeHidden();
  await sw.click();
  await expect(page.getByRole("heading", { name: "Shortcut" })).toBeVisible();
  expect((await calls(page, "voice_set")).map((c: any) => c.voice.on)).toEqual([false, true]);
});

test("fn can be picked while recording; the browser never sees it, so the backend says so", async ({ page }) => {
  await open(page);
  await page.evaluate(() => (window as any).__emit("voice-fn", null)); // not recording: ignored
  await expect(page.getByLabel("Current shortcut: Option Space")).toBeVisible();
  await page.getByRole("button", { name: "Change" }).click();
  await page.evaluate(() => (window as any).__emit("voice-fn", null));
  await expect(page.getByLabel("Current shortcut: fn")).toHaveText("fn");
  await expect(page.getByRole("button", { name: "Change" })).toBeFocused();
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Fn", clean: null, mode: "hold", paste: "clipboard" } }]);
});

test("with fn, the 🌐 key row shows the steps until the key is set to Do Nothing", async ({ page }) => {
  await open(page, "Fn");
  const row = page.getByRole("listitem").filter({ hasText: "Set the 🌐 key to Do Nothing" });
  await expect(row).toContainText("“Press 🌐 key to”");
  await row.getByRole("button", { name: "Open Keyboard Settings" }).click();
  expect(await calls(page, "globe_open")).toHaveLength(1);
  // Changed in System Settings, then back to Myna.
  await page.evaluate(() => { (window as any).__globe = true; (window as any).__emit("tauri://focus", null); });
  await expect(row).toBeHidden();
});

test("the 🌐 key row is only for fn", async ({ page }) => {
  await open(page);
  await expect(page.getByText("Set the 🌐 key to Do Nothing")).toBeHidden();
});

test("How words are added: Paste by default, Type saves and changes the help", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("radio", { name: "Paste" })).toBeChecked();
  await expect(page.getByText("Fastest. Myna puts back whatever you had copied afterwards.")).toBeVisible();
  await page.getByRole("radio", { name: "Type" }).check();
  await expect(page.getByText("Types each letter. Slower, but works in apps that block pasting.")).toBeVisible();
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: null, mode: "hold", paste: "type" } }]);
});

test("only the page scrolls, never the whole window", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 420 });
  await open(page);
  await expect(page.getByRole("heading", { name: "Permissions" })).toBeAttached();
  const doc = await page.evaluate(() => [document.documentElement.scrollHeight, innerHeight]);
  expect(doc[0]).toBeLessThanOrEqual(doc[1]);
});

const OLLAMA = { id: 1, kind: "openai", name: "Ollama on this Mac", url: "http://localhost:11434/v1", has_key: false, off_device: false };
const OR = { id: 2, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true };
const fastOn = (p: typeof OLLAMA, model: string) => ({ providers: [p], main: null, fast: { provider: p.id, model, gone: false } });
const cleanRow = (page: Page) => page.getByRole("listitem").filter({ hasText: "Dictate with cleanup" });

test("Shortcuts: Dictate and Dictate with cleanup, and one How it starts for both", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("heading", { name: "Shortcuts" })).toBeVisible();
  await expect(page.getByText("Adds what you said. Works in any app.")).toBeVisible();
  await expect(page.getByText("Hold the key while you talk. Let go to add your words. Same for both shortcuts.")).toBeVisible();
});

test("cleanup without a Fast model: Needs an Endpoint, and Set up opens AI provider", async ({ page }) => {
  await open(page);
  const row = cleanRow(page);
  await expect(row).toContainText("Needs an Endpoint");
  await expect(row).toContainText("Set a Fast model on AI provider first.");
  await expect(row.getByRole("button", { name: "Set shortcut", exact: true })).toHaveCount(0); // no recorder yet
  await row.getByRole("button", { name: "Set up" }).click();
  await expect(page.getByRole("heading", { name: "AI provider", exact: true })).toBeVisible();
});

test("a Fast model that's gone counts as none", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: { providers: [OLLAMA], main: null, fast: { provider: 1, model: "qwen2.5:7b", gone: true } } });
  await expect(cleanRow(page)).toContainText("Needs an Endpoint");
});

test("cleanup with a Fast model on this Mac: names it, says it stays here, and starts unassigned", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  const row = cleanRow(page);
  await expect(row).toContainText("before adding your words. If that takes over 3 s, they go in as they are.");
  await expect(row).toContainText("Ollama on this Mac · qwen2.5:7b");
  await expect(row).toContainText("Stays on this Mac");
  await expect(row.getByLabel("No shortcut")).toBeVisible();
  await expect(row).not.toContainText("Needs an Endpoint");
  await expect(row.getByRole("button", { name: "Remove" })).toHaveCount(0);
});

test("cleanup with an online Fast model says it sends text off this Mac", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OR as typeof OLLAMA, "openai/gpt-5-mini") });
  await expect(cleanRow(page)).toContainText("Sends text off this Mac");
});

test("Set records the cleanup shortcut and saves it beside Dictate's; Change and Remove follow", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  const row = cleanRow(page);
  await row.getByRole("button", { name: "Set shortcut", exact: true }).click();
  await expect(page.getByRole("button", { name: "Press a key…" })).toBeFocused();
  await page.keyboard.press("Control+Shift+KeyC");
  await expect(page.getByLabel("Current shortcut: Control Shift C")).toHaveText("⌃⇧C");
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: "Ctrl+Shift+KeyC", mode: "hold", paste: "clipboard" } }]);
  await expect(row.getByRole("button", { name: "Change" })).toBeVisible();
  await row.getByRole("button", { name: "Remove the Dictate with cleanup shortcut" }).click();
  await expect(row.getByLabel("No shortcut")).toBeVisible();
  expect((await calls(page, "voice_set")).at(-1)).toEqual({ voice: { on: true, key: "Alt+Space", clean: null, mode: "hold", paste: "clipboard" } });
});

test("a key already used by the other shortcut is refused with the reason", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  await cleanRow(page).getByRole("button", { name: "Set shortcut", exact: true }).click();
  await page.keyboard.press("Alt+Space");
  await expect(page.getByText("Option Space is already the Dictate shortcut. Pick another key.")).toBeVisible();
  expect(await calls(page, "voice_set")).toEqual([]);
  expect((await calls(page, "voice_pause")).at(-1)).toEqual({ paused: false });
  // And the other way round.
  await page.getByRole("button", { name: "Change" }).click();
  await page.keyboard.press("Control+Shift+KeyC");
  await cleanRow(page).getByRole("button", { name: "Set shortcut", exact: true }).click();
  await page.keyboard.press("Control+Shift+KeyC");
  await expect(page.getByText("Control Shift C is already the Dictate shortcut. Pick another key.")).toBeVisible();
});

test("a Fast model set on AI provider shows on Voice typing without reloading", async ({ page }) => {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [], voice_get: () => ({ on: true, key: "Alt+Space", clean: null, mode: "hold", paste: "clipboard" }),
    ai_get: () => (window.__ai ??= ${JSON.stringify({ providers: [OLLAMA], main: null, fast: null })}), globe_ok: () => true,
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "AI provider" }).click();
  await page.evaluate(() => ((window as any).__ai.fast = { provider: 1, model: "qwen2.5:7b", gone: false })); // what picking it there saves
  await page.getByRole("option", { name: "Voice typing" }).click();
  await expect(cleanRow(page)).toContainText("Ollama on this Mac · qwen2.5:7b");
  await expect(cleanRow(page).getByRole("button", { name: "Set shortcut", exact: true })).toBeVisible();
});

// Cleanup styles: only once there's a Fast model to run them. `window.__newer` marks which edited styles have a newer shipped text.
const withFast = (page: Page) => open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
const styleRow = (page: Page, name: string) => page.getByRole("listitem").filter({ hasText: new RegExp(`^${name}`) }).filter({ has: page.getByRole("button", { name: /^(Edit|Done)$/ }) });
const edit = (page: Page, name: string) => styleRow(page, name).getByRole("button", { name: "Edit" }).click();

test("Cleanup styles show only with a Fast model", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("heading", { name: "Words to remember" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Cleanup styles" })).toHaveCount(0);
});

test("Cleanup styles list the four shipped ones with their apps, and mark an edited one with a newer default", async ({ page }) => {
  await withFast(page);
  await expect(page.getByText("Dictate with cleanup picks a style by the app you’re in. Only Myna sees the app; its name is never sent.")).toBeVisible();
  await expect(styleRow(page, "Default")).toContainText("Every other app");
  await expect(styleRow(page, "Code")).toContainText("Xcode, Cursor");
  await expect(styleRow(page, "Message")).toContainText("Slack · edited");
  await expect(styleRow(page, "Message")).toContainText("Newer default");
  await expect(styleRow(page, "Email")).not.toContainText("edited");
});

test("Default has no name or apps to edit, shows what Myna adds, and Reset waits for an edit", async ({ page }) => {
  await withFast(page);
  await edit(page, "Default");
  await expect(page.getByLabel("Name")).toHaveCount(0);
  await expect(page.getByText("Every app that doesn’t have its own style.")).toBeVisible();
  await expect(page.getByText("Myna always adds: “Spell these exactly as written: Postgres, Myna.” What you said goes in as text to clean up, never as instructions.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Reset to default" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Edit", exact: true }).first()).toBeVisible();
});

test("editing a style's instructions saves on leaving the box, marks it edited, and Reset puts the shipped text back", async ({ page }) => {
  await withFast(page);
  await edit(page, "Email");
  const box = page.getByRole("textbox", { name: "Instructions" });
  await expect(box).toHaveValue(SHIPPED.email);
  await box.fill("Make it formal.");
  expect(await calls(page, "cleanup_set")).toEqual([]); // not on every key
  await box.blur();
  await expect(styleRow(page, "Email")).toContainText("Mail · edited");
  expect((await calls(page, "cleanup_set")).at(-1).styles.find((x: any) => x.id === "email")).toMatchObject({ text: "Make it formal." });
  await page.getByRole("button", { name: "Reset to default" }).click();
  await expect(box).toHaveValue(SHIPPED.email);
  await expect(styleRow(page, "Email")).not.toContainText("edited");
  await expect(page.getByRole("button", { name: "Reset to default" })).toBeDisabled();
});

test("an emptied box goes back to the shipped text", async ({ page }) => {
  await withFast(page);
  await edit(page, "Code");
  const box = page.getByRole("textbox", { name: "Instructions" });
  await box.fill("  ");
  await box.blur();
  await expect(box).toHaveValue(SHIPPED.code);
});

test("a newer default keeps the edit until Use the new default", async ({ page }) => {
  await withFast(page);
  await page.evaluate(() => ((window as any).__newer = { message: true }));
  await edit(page, "Message");
  await expect(page.getByRole("textbox", { name: "Instructions" })).toHaveValue("Keep it casual and short.");
  await expect(page.getByRole("status").filter({ hasText: "A newer default is available. Your edit is kept until you choose." })).toBeVisible();
  await page.getByRole("button", { name: "Use the new default" }).click();
  await expect(page.getByRole("textbox", { name: "Instructions" })).toHaveValue(SHIPPED.message);
  await expect(page.getByText("A newer default is available")).toHaveCount(0);
  await expect(styleRow(page, "Message")).not.toContainText("Newer default");
});

test("adding an app moves it from the style that had it; removing one leaves it for Default", async ({ page }) => {
  await withFast(page);
  await edit(page, "Code");
  const add = page.getByRole("combobox", { name: "Add an app to Code" });
  await expect(add.getByRole("option", { name: "Slack (now Message)" })).toHaveCount(1);
  await add.selectOption("Slack");
  await expect(styleRow(page, "Code")).toContainText("Xcode, Cursor, Slack");
  await expect(styleRow(page, "Message")).toContainText("No apps yet");
  await page.getByRole("button", { name: "Remove Xcode from Code" }).click();
  await expect(styleRow(page, "Code")).toContainText("Cursor, Slack");
  await expect(add).toBeFocused();
});

test("Add style makes a named style with Default's text; Delete style removes it; shipped styles can't be deleted", async ({ page }) => {
  await withFast(page);
  await page.getByRole("button", { name: "Add style" }).click();
  const name = page.getByRole("textbox", { name: "Name" });
  await expect(name).toBeFocused();
  await name.fill("Notes style");
  await name.blur();
  await expect(styleRow(page, "Notes style")).toContainText("No apps yet");
  await expect(page.getByRole("textbox", { name: "Instructions" })).toHaveValue(BASE);
  await page.getByRole("button", { name: "Delete style" }).click();
  await expect(page.getByText("Notes style")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Add style" })).toBeFocused();
  await edit(page, "Code");
  await expect(page.getByRole("button", { name: "Delete style" })).toHaveCount(0);
});

test("a failed save says why and shows what was kept", async ({ page }) => {
  await withFast(page);
  await edit(page, "Email");
  const box = page.getByRole("textbox", { name: "Instructions" });
  await box.fill("boom");
  await box.blur();
  await expect(page.getByText("Couldn't save the cleanup styles: disk full")).toBeVisible();
  await expect(box).toHaveValue(SHIPPED.email);
});
