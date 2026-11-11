import { expect, test, type Page } from "@playwright/test";
import { fakeTauri, MODELS } from "./tauri";

const BASE = "Fix punctuation, capitals and grammar slips.";
const SHIPPED: Record<string, string> = { default: BASE, code: BASE + " Keep code names.", message: BASE + " Keep it casual.", email: BASE + " Use full sentences." };
const style = (id: string, name: string, apps: string[], text = SHIPPED[id] ?? BASE, extra = {}) => ({ id, name, apps, text, shipped: SHIPPED[id] ?? null, newer: false, ...extra });
const STYLES = [style("default", "Default", []), style("code", "Code", ["Xcode", "Cursor"]), style("message", "Message", ["Slack"], "Keep it casual and short.", { newer: true }), style("email", "Email", ["Mail"])];

// `voice_set` fails for any shortcut using Ctrl+Alt, like a shortcut another app holds.
// `__globe` stands for the 🌐 key's setting being Do Nothing.
async function open(page: Page, key = "Alt+Space", globe = false, extra: { clean?: string | null; ai?: object; at?: string; on?: boolean } = {}) {
  await page.addInitScript((g) => ((window as any).__globe = g), globe);
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => ${MODELS},
    voice_get: () => ({ on: ${extra.on ?? true}, key: "${key}", clean: ${JSON.stringify(extra.clean ?? null)}, mode: "hold", paste: "clipboard" }),
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
  await page.getByRole("option", { name: extra.at ?? "Listen" }).click();
}
const dictateKey = (page: Page) => page.getByRole("button", { name: /^Dictate shortcut: / });
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);

test("shows the shortcut", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("button", { name: "Dictate shortcut: Option Space" })).toHaveText("⌥Space");
});

test("the recorder shows the keycaps; clicking it records a new shortcut and saves it; the old one is paused meanwhile", async ({ page }) => {
  await open(page);
  await dictateKey(page).click();
  await expect(dictateKey(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Press keys…" })).toBeFocused();
  await expect(page.getByText("Press the key or keys you want to use. Esc cancels.")).toBeVisible();
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("KeyD");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await expect(page.getByRole("button", { name: "Dictate shortcut: Control Shift D" })).toHaveText("⌃⇧D");
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }]);
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Ctrl+Shift+KeyD", clean: null, mode: "hold", paste: "clipboard" } }]);
});
test("Esc cancels and gives the old shortcut back", async ({ page }) => {
  await open(page);
  await dictateKey(page).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Dictate shortcut: Option Space" })).toBeVisible();
  await expect(dictateKey(page)).toBeFocused();
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }, { paused: false }]);
  expect(await calls(page, "voice_set")).toEqual([]);
});

test("a key that types on its own is refused, and recording goes on", async ({ page }) => {
  await open(page);
  await dictateKey(page).click();
  await page.keyboard.press("KeyD");
  await expect(page.getByText("Add ⌘, ⌥ or ⌃, so the key still types normally.")).toBeVisible();
  await page.keyboard.press("F5");
  await expect(page.getByRole("button", { name: "Dictate shortcut: F5" })).toBeVisible();
  await expect(page.getByText("Add ⌘")).toBeHidden();
});

test("a shortcut another app holds says so and keeps the old one", async ({ page }) => {
  await open(page);
  await dictateKey(page).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Dictate shortcut: Option Space" })).toBeVisible();
});

test("Tap saves, says how it stops, and the preview follows", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Release ⌥Space to paste");
  await page.getByRole("radio", { name: "Tap" }).check();
  await expect(page.getByText("Press to start, again to stop. Stops after 5 minutes. Esc cancels.")).toBeVisible();
  await expect(page.getByRole("img", { name: /Preview/ })).toContainText("Press ⌥Space again to paste");
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: null, mode: "tap", paste: "clipboard" } }]);
});

test("turning voice typing off leaves only the Use voice typing row; on brings the rest back", async ({ page }) => {
  await open(page);
  const sw = page.getByRole("switch", { name: "Use voice typing" });
  await expect(page.getByRole("listitem").filter({ hasText: "Microphone" })).toContainText("Allowed");
  await sw.click();
  await expect(sw).not.toBeChecked();
  for (const h of ["Shortcuts", "Pill"]) await expect(page.getByRole("heading", { name: h })).toBeHidden();
  await expect(page.getByText("Microphone", { exact: true })).toBeHidden();
  await expect(page.locator(".page .row")).toHaveCount(1);
  await sw.click();
  await expect(page.getByRole("heading", { name: "Shortcuts" })).toBeVisible();
  await expect(page.getByText("Microphone", { exact: true })).toBeVisible();
  expect((await calls(page, "voice_set")).map((c: any) => c.voice.on)).toEqual([false, true]);
});

test("a failed switch-on keeps the error line under the one row", async ({ page }) => {
  await open(page, "Ctrl+Alt+KeyK", false, { on: false }); // voice_set refuses this key
  const sw = page.getByRole("switch", { name: "Use voice typing" });
  await sw.click();
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(sw).not.toBeChecked();
  await expect(page.locator(".page .row")).toHaveCount(1);
});

test("fn can be picked while recording; the browser never sees it, so the backend says so", async ({ page }) => {
  await open(page);
  await page.evaluate(() => (window as any).__emit("voice-fn", null)); // not recording: ignored
  await expect(page.getByRole("button", { name: "Dictate shortcut: Option Space" })).toBeVisible();
  await dictateKey(page).click();
  await page.evaluate(() => (window as any).__emit("voice-fn", null));
  await expect(page.getByRole("button", { name: "Dictate shortcut: fn" })).toHaveText("fn");
  await expect(dictateKey(page)).toBeFocused();
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Fn", clean: null, mode: "hold", paste: "clipboard" } }]);
});

test("with fn, the 🌐 key row shows the steps until the key is set to Do Nothing", async ({ page }) => {
  await open(page, "Fn");
  const row = page.getByRole("listitem").filter({ hasText: "Set fn to Do Nothing" });
  await expect(row).toContainText("In Keyboard settings, set Press 🌐 key to Do Nothing.");
  await expect(row.locator("kbd")).toHaveText("fn");
  await row.getByRole("button", { name: "Open Keyboard Settings" }).click();
  expect(await calls(page, "globe_open")).toHaveLength(1);
  // Changed in System Settings, then back to Myna.
  await page.evaluate(() => { (window as any).__globe = true; (window as any).__emit("tauri://focus", null); });
  await expect(row).toBeHidden();
});

test("the 🌐 key row is only for fn", async ({ page }) => {
  await open(page);
  await expect(page.getByText("Do Nothing")).toBeHidden();
});

test("How words are added: Paste by default, Type saves and changes the help", async ({ page }) => {
  await open(page, "Alt+Space", false, { at: "Write" });
  await expect(page.getByRole("radio", { name: "Paste" })).toBeChecked();
  await expect(page.getByText("Fastest. Myna puts back whatever you had copied afterwards.")).toBeVisible();
  await page.getByRole("radio", { name: "Type" }).check();
  await expect(page.getByText("Types each letter. Slower, but works in apps that block pasting.")).toBeVisible();
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: null, mode: "hold", paste: "type" } }]);
});

test("only the page scrolls, never the whole window", async ({ page }) => {
  await page.setViewportSize({ width: 880, height: 420 });
  await open(page);
  await expect(page.getByRole("heading", { name: "Pill" })).toBeAttached();
  const doc = await page.evaluate(() => [document.documentElement.scrollHeight, innerHeight]);
  expect(doc[0]).toBeLessThanOrEqual(doc[1]);
});

const OLLAMA = { id: 1, kind: "openai", name: "Ollama on this Mac", url: "http://localhost:11434/v1", has_key: false, off_device: false };
const OR = { id: 2, kind: "openrouter", name: "OpenRouter", url: "https://openrouter.ai/api/v1", has_key: true, off_device: true };
const fastOn = (p: typeof OLLAMA, model: string) => ({ providers: [p], main: null, fast: { provider: p.id, model, gone: false } });
const cleanRow = (page: Page) => page.getByRole("listitem").filter({ hasText: "Dictate with cleanup" });

test("Shortcuts: Dictate, Dictate with cleanup and Activation", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("heading", { name: "Shortcuts" })).toBeVisible();
  await expect(page.getByText("Adds what you said. Works in any app.")).toBeVisible();
  await expect(page.getByText("Hold while you talk, let go to add. Esc cancels.")).toBeVisible();
  await expect(page.getByRole("radio", { name: "Hold" })).toBeChecked();
});

test("cleanup without a Fast model: needs a Fast model, and Set up opens AI providers", async ({ page }) => {
  await open(page);
  const row = cleanRow(page);
  await expect(row).toContainText("Needs a Fast model.");
  await expect(row.getByRole("button", { name: /^Set the / })).toHaveCount(0); // no recorder yet
  await row.getByRole("button", { name: "Set up" }).click();
  await expect(page.getByRole("heading", { name: "AI providers", exact: true, level: 1 })).toBeVisible();
});

test("a Fast model that's gone counts as none", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: { providers: [OLLAMA], main: null, fast: { provider: 1, model: "qwen2.5:7b", gone: true } } });
  await expect(cleanRow(page)).toContainText("Needs a Fast model.");
});

test("cleanup with a Fast model: a one-line description, and it starts unassigned", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  const row = cleanRow(page);
  await expect(row).toContainText("Fixes punctuation, grammar slips and self-corrections.");
  await expect(row).not.toContainText("Needs a Fast model");
  await expect(row.getByRole("button", { name: "Set the Dictate with cleanup shortcut" })).toHaveText("Set key");
  await expect(row.getByRole("button", { name: "Remove" })).toHaveCount(0);
});

test("Set records the cleanup shortcut and saves it beside Dictate's; the recorder and Remove follow", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  const row = cleanRow(page);
  await row.getByRole("button", { name: "Set the Dictate with cleanup shortcut" }).click();
  await expect(page.getByRole("button", { name: "Press keys…" })).toBeFocused();
  await page.keyboard.press("Control+Shift+KeyC");
  await expect(page.getByRole("button", { name: "Dictate with cleanup shortcut: Control Shift C" })).toHaveText("⌃⇧C");
  expect(await calls(page, "voice_set")).toEqual([{ voice: { on: true, key: "Alt+Space", clean: "Ctrl+Shift+KeyC", mode: "hold", paste: "clipboard" } }]);
  await expect(row.getByRole("button", { name: "Dictate with cleanup shortcut: Control Shift C" })).toBeVisible();
  await row.getByRole("button", { name: "Remove the Dictate with cleanup shortcut" }).click();
  await expect(row.getByRole("button", { name: "Set the Dictate with cleanup shortcut" })).toHaveText("Set key");
  expect((await calls(page, "voice_set")).at(-1)).toEqual({ voice: { on: true, key: "Alt+Space", clean: null, clean2: null, mode: "hold", paste: "clipboard" } });
});

test("Add another key gives each shortcut a second key; one key can't serve twice; removing the first cleanup key moves the second up", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b"), clean: "Ctrl+Shift+KeyC" });
  const add = page.getByRole("button", { name: "Add another key" });
  await expect(add).toHaveCount(2); // one per shortcut
  await add.first().click();
  await expect(page.getByRole("button", { name: "Press keys…" })).toBeFocused();
  await page.keyboard.press("PageDown");
  await expect(page.getByRole("button", { name: "second Dictate shortcut: Page Down" })).toBeVisible();
  expect((await calls(page, "voice_set")).at(-1)).toEqual({ voice: { on: true, key: "Alt+Space", clean: "Ctrl+Shift+KeyC", key2: "PageDown", mode: "hold", paste: "clipboard" } });
  // Dictate's second key can't also be cleanup's.
  await page.getByRole("button", { name: "Add another key" }).click();
  await page.keyboard.press("PageDown");
  await expect(page.getByText("Page Down is already the Dictate shortcut. Pick another key.")).toBeVisible();
  await page.getByRole("button", { name: "Add another key" }).click();
  await page.keyboard.press("End");
  expect((await calls(page, "voice_set")).at(-1)).toMatchObject({ voice: { clean: "Ctrl+Shift+KeyC", clean2: "End" } });
  await page.getByRole("button", { name: "Remove the Dictate with cleanup shortcut" }).click();
  expect((await calls(page, "voice_set")).at(-1)).toMatchObject({ voice: { key2: "PageDown", clean: "End", clean2: null } });
  await page.getByRole("button", { name: "Remove the second Dictate shortcut" }).click();
  expect((await calls(page, "voice_set")).at(-1)).toMatchObject({ voice: { key2: null } });
  await expect(add).toHaveCount(2);
});

test("a key already used by the other shortcut is refused with the reason", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  await cleanRow(page).getByRole("button", { name: "Set the Dictate with cleanup shortcut" }).click();
  await page.keyboard.press("Alt+Space");
  await expect(page.getByText("Option Space is already the Dictate shortcut. Pick another key.")).toBeVisible();
  expect(await calls(page, "voice_set")).toEqual([]);
  expect((await calls(page, "voice_pause")).at(-1)).toEqual({ paused: false });
  // And the other way round.
  await dictateKey(page).click();
  await page.keyboard.press("Control+Shift+KeyC");
  await cleanRow(page).getByRole("button", { name: "Set the Dictate with cleanup shortcut" }).click();
  await page.keyboard.press("Control+Shift+KeyC");
  await expect(page.getByText("Control Shift C is already the Dictate shortcut. Pick another key.")).toBeVisible();
});

test("a Fast model set on AI provider shows on Listen without reloading", async ({ page }) => {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => ${MODELS}, voice_get: () => ({ on: true, key: "Alt+Space", clean: null, mode: "hold", paste: "clipboard" }),
    ai_get: () => (window.__ai ??= ${JSON.stringify({ providers: [OLLAMA], main: null, fast: null })}), globe_ok: () => true,
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "AI providers" }).click();
  await page.evaluate(() => ((window as any).__ai.fast = { provider: 1, model: "qwen2.5:7b", gone: false })); // what picking it there saves
  await page.getByRole("option", { name: "Listen" }).click();
  await expect(cleanRow(page)).not.toContainText("Needs a Fast model");
  await expect(cleanRow(page).getByRole("button", { name: "Set the Dictate with cleanup shortcut" })).toBeVisible();
});

// Cleanup styles: only once there's a Fast model to run them. `window.__newer` marks which edited styles have a newer shipped text.
const withFast = (page: Page) => open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b"), at: "Write" });
const styleRow = (page: Page, name: string) => page.getByRole("listitem").filter({ hasText: new RegExp(`^${name}`) }).filter({ has: page.getByRole("button", { name: /^(Edit|Done)$/ }) });
const edit = (page: Page, name: string) => styleRow(page, name).getByRole("button", { name: "Edit" }).click();

test("Cleanup styles show only with a Fast model", async ({ page }) => {
  await open(page, "Alt+Space", false, { at: "Write" });
  await expect(page.getByRole("heading", { name: "Adding your words" })).toBeVisible();
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

test("Sounds is on unless switched off, and saves with the other voice settings", async ({ page }) => {
  await open(page);
  const sounds = page.getByRole("switch", { name: "Start and stop sounds" });
  await expect(sounds).toBeChecked();
  await sounds.click();
  await expect(sounds).not.toBeChecked();
  expect((await calls(page, "voice_set")).at(-1)).toMatchObject({ voice: { key: "Alt+Space", sounds: false } });
});

test("Write's empty state while voice typing is off sends you to Listen; Understand says why", async ({ page }) => {
  await open(page, "Alt+Space", false, { on: false, at: "Write" });
  await expect(page.getByText("Voice typing is off", { exact: true })).toBeVisible();
  await expect(page.getByText("Turn it on in Listen to choose how your words are added.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Adding your words" })).toHaveCount(0);
  await page.getByRole("option", { name: "Understand" }).click();
  await expect(page.getByText("Voice typing is off. Turn it on in Listen. Meeting notes use their own model.")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Speech-to-text" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Your words" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Suggested fixes" })).toHaveCount(0);
  await page.getByRole("option", { name: "Write" }).click();
  await page.getByRole("button", { name: "Go to Listen" }).click();
  await expect(page.getByRole("heading", { name: "Listen", level: 1 })).toBeVisible();
  await expect(page.getByRole("switch", { name: "Use voice typing" })).not.toBeChecked();
});

test("Understand and Write show their rows while voice typing is on", async ({ page }) => {
  await open(page, "Alt+Space", false, { at: "Understand" });
  await expect(page.getByRole("heading", { name: "Your words" })).toBeVisible();
  await expect(page.getByText("Voice typing is off")).toHaveCount(0);
});

test("the recorders' right edges line up across Dictate and Dictate with cleanup", async ({ page }) => {
  await open(page, "Alt+Space", false, { ai: fastOn(OLLAMA, "qwen2.5:7b") });
  const right = async (loc: import("@playwright/test").Locator) => { const b = (await loc.boundingBox())!; return Math.round(b.x + b.width); };
  const rows = [page.getByRole("listitem").filter({ hasText: "Adds what you said" }), cleanRow(page)];
  const plus = rows.map((r) => r.getByRole("button", { name: "Add another key" }));
  const keys = rows.map((r) => r.getByRole("button", { name: /^(Dictate|Set the)/ }));
  expect(await right(plus[0])).toBe(await right(plus[1]));
  expect(await right(keys[0])).toBe(await right(keys[1]));
  // Recording doesn't move the edge.
  const edge = await right(keys[0]);
  await keys[0].click();
  expect(await right(page.getByRole("button", { name: "Press keys…" }))).toBe(edge);
  await page.keyboard.press("Escape");
  expect(await right(keys[0])).toBe(edge);
});

test("Set up and the other end buttons share the shortcut buttons' right edge", async ({ page }) => {
  await open(page, "Fn");
  const right = async (loc: import("@playwright/test").Locator) => { const b = (await loc.boundingBox())!; return Math.round(b.x + b.width); };
  const edge = await right(page.getByRole("button", { name: "Add another key" }));
  expect(await right(page.getByRole("button", { name: "Set up" }))).toBe(edge);
  expect(await right(page.getByRole("button", { name: "Open Keyboard Settings" }))).toBe(edge);
});
