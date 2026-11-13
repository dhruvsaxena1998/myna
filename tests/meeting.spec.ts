import { expect, test, type Locator, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// `meeting_set` fails for Ctrl+Alt+N, like a shortcut another app holds, and for a key a voice typing shortcut has (the backend's message). Asking for System Audio allows it.
// `extra` adds or replaces handlers; `addon_get` can answer late, to show the page waits for it.
async function open(page: Page, notes: { on: boolean; key: string; keep_teams?: boolean } = { on: false, key: "Ctrl+Alt+KeyM" }, audio = "allowed", addon = { connected: false, code: "MYNA-4F7Q-K2" }, extra = "", late = 0) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    meeting_get: () => (${JSON.stringify(notes)}),
    addon_get: () => new Promise((ok) => setTimeout(() => ok(${JSON.stringify(addon)}), ${late})),
    addon_disconnect: () => "MYNA-NEW1-23", addon_folder: () => {}, text_copy: () => {}, voice_pause: () => {},
    meeting_set: ({ notes }) => {
      if (notes.key === "Ctrl+Alt+KeyN") throw "Another app is using that shortcut. Pick a different one.";
      if (notes.key === "Alt+Space") throw "Option Space is already the Dictate shortcut. Pick another key.";
      if (notes.key === "Ctrl+Shift+KeyC") throw "Control Shift C is already the Dictate with cleanup shortcut. Pick another key.";
    },
    permissions_state: () => ({ ...(window.__perms ??= { mic: "allowed", ax: true, audio: ${JSON.stringify(audio)} }) }),
    permission_ask: ({ kind }) => { if (kind === "audio" && window.__perms.audio === "ask") window.__perms.audio = "allowed"; },
    ${extra}
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: "Meeting notes" }).click();
}
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).map((x: any) => x[1]), cmd);
const shortcut = (page: Page) => page.getByRole("button", { name: /^Recording shortcut: / });
const heights = (loc: Locator) => loc.evaluateAll((els) => els.map((e) => e.getBoundingClientRect().height));

test("off by default: just the switch, and turning it on saves and shows the shortcut", async ({ page }) => {
  await open(page);
  const sw = page.getByRole("switch", { name: "Use meeting notes" });
  await expect(sw).not.toBeChecked();
  await expect(page.getByText("Recording shortcut")).toBeHidden();
  await expect(page.getByRole("heading", { level: 2 }).filter({ hasNotText: "Meeting notes" })).toHaveCount(0);
  await sw.click();
  await expect(shortcut(page)).toHaveText("⌃⌥M");
  expect(await calls(page, "meeting_set")).toEqual([{ notes: { on: true, key: "Ctrl+Alt+KeyM" } }]);
  await expect(page.getByText("Works from any app, even when the window is closed.")).toBeVisible();
});

test("Keep my voice when muted: the Teams switch is off and disabled until the add-on is connected, then it saves with the other settings", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM", keep_teams: true });
  await expect(page.getByText("What you say while muted is left out of the notes, unless you turn on the app here.")).toBeVisible();
  const keep = page.getByRole("switch", { name: "Keep my voice when muted in Microsoft Teams on the web" });
  await expect(keep).toBeDisabled();
  await expect(keep).not.toBeChecked(); // saved on, but it can't work yet
  await expect(page.getByText("Needs the browser add-on. Connect it below.")).toBeVisible();
  await emit(page, "addon-status", true);
  await expect(keep).toBeEnabled();
  await expect(keep).toBeChecked();
  await expect(page.getByText("Myna sees your mute button through the browser add-on.")).toBeVisible();
  await keep.click();
  await expect(keep).not.toBeChecked();
  expect((await calls(page, "meeting_set")).at(-1)).toEqual({ notes: { on: true, key: "Ctrl+Alt+KeyM", keep_teams: false } });
});

test("a failed save of the Teams switch shows under its own group", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM", keep_teams: false }, "allowed", { connected: true, code: "C" }, `meeting_set: () => { throw "Couldn’t save meeting notes: database is locked"; }`);
  const keep = page.getByRole("switch", { name: "Keep my voice when muted in Microsoft Teams on the web" });
  await keep.click();
  const group = page.getByRole("region", { name: "Keep my voice when muted" });
  await expect(group.getByText("Couldn’t save meeting notes: database is locked")).toBeVisible();
  await expect(keep).not.toBeChecked();
  await expect(page.getByRole("region", { name: "Meeting notes" }).getByText("database is locked")).toHaveCount(0);
});

test("the recorder: click, press keys, and the shortcut is saved; Esc cancels; the shortcuts are paused meanwhile", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  await shortcut(page).click();
  await expect(page.getByRole("button", { name: "Press keys…" })).toBeFocused();
  await expect(page.getByText("Press the key or keys you want to use. Esc cancels.")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(shortcut(page)).toHaveText("⌃⌥M");
  expect(await calls(page, "voice_pause")).toEqual([{ paused: true }, { paused: false }]);
  expect(await calls(page, "meeting_set")).toEqual([]);
  await shortcut(page).click();
  await page.keyboard.press("Control+Alt+KeyK");
  await expect(page.getByRole("button", { name: "Recording shortcut: Control Option K" })).toHaveText("⌃⌥K");
  expect((await calls(page, "meeting_set")).at(-1)).toEqual({ notes: { on: true, key: "Ctrl+Alt+KeyK" } });
});

test("the shortcut can be removed, and No shortcut means an empty key", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const x = page.getByRole("button", { name: "Remove the Recording shortcut" });
  const row = page.getByRole("listitem").filter({ hasText: "Recording shortcut" });
  const before = (await heights(row))[0];
  await x.click();
  expect((await calls(page, "meeting_set")).at(-1)).toEqual({ notes: { on: true, key: "" } });
  await expect(page.getByRole("button", { name: "Set the Recording shortcut" })).toHaveText("Set key");
  await expect(x).toBeHidden();
  expect((await heights(row))[0]).toBe(before);
});

test("a shortcut another app holds is refused and the old one stays", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  await shortcut(page).click();
  await page.keyboard.press("Control+Alt+KeyN");
  await expect(page.getByText("Another app is using that shortcut. Pick a different one.")).toBeVisible();
  await expect(shortcut(page)).toHaveText("⌃⌥M");
});

test("a key a voice typing shortcut uses is refused with the backend’s message, and the old key stays", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  await shortcut(page).click();
  await page.keyboard.press("Alt+Space");
  await expect(page.getByText("Option Space is already the Dictate shortcut. Pick another key.")).toBeVisible();
  await expect(shortcut(page)).toHaveText("⌃⌥M");
  await expect(shortcut(page)).toBeFocused();
  await shortcut(page).click();
  await page.keyboard.press("Control+Shift+KeyC");
  await expect(page.getByText("Control Shift C is already the Dictate with cleanup shortcut. Pick another key.")).toBeVisible();
  await expect(shortcut(page)).toHaveText("⌃⌥M");
});

test("fn alone is refused with a clear message", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  await shortcut(page).click();
  await emit(page, "voice-fn", null);
  await expect(page.getByText("fn alone can’t start meeting notes. Add another key with it.")).toBeVisible();
  await expect(shortcut(page)).toHaveText("⌃⌥M");
  expect(await calls(page, "meeting_set")).toEqual([]);
});

test("System Audio shows on the page once notes are on, and Allow asks macOS", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "ask");
  const row = page.getByRole("listitem").filter({ hasText: "Screen & System Audio Recording" });
  await expect(row).toContainText("So Myna can hear the other people on a call.");
  await expect(page.getByRole("listitem").filter({ hasText: "Accessibility" })).toHaveCount(0);
  await row.getByRole("button", { name: "Allow Screen & System Audio Recording" }).click();
  await expect(row).toContainText("Allowed");
  expect(await calls(page, "permission_ask")).toEqual([{ kind: "audio" }]);
});

test("System Audio says plainly when Myna can't check it, in its one description, and offers System Settings", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "unknown");
  const row = page.getByRole("listitem").filter({ hasText: "Screen & System Audio Recording" });
  await expect(row).toContainText("Myna can’t check this one. Your Mac asks the first time you record a meeting.");
  await expect(row).not.toContainText("So Myna can hear the other people on a call.");
  await expect(row).not.toContainText("Allowed");
  await expect(row.getByRole("button", { name: "Open System Settings for Screen & System Audio Recording" })).toBeVisible();
});

test("Permissions: Microphone and System Audio, each one description on one 60px row, and no help line", async ({ page }) => {
  for (const audio of ["allowed", "ask", "unknown"]) {
    await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, audio);
    const group = page.getByRole("region", { name: "Permissions" });
    const rows = group.getByRole("listitem");
    await expect(rows).toHaveCount(2);
    await expect(group.locator(".desc")).toHaveCount(2);
    expect(await heights(rows)).toEqual([60, 60]);
    await expect(group.locator(".help")).toHaveCount(0);
    await page.goto("about:blank");
  }
});

test("no permission rows while notes are off", async ({ page }) => {
  await open(page, { on: false, key: "Ctrl+Alt+KeyM" }, "denied");
  await expect(page.getByText("Screen & System Audio Recording")).toBeHidden();
});

const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);

test("the recording pill says Muted or Mic live, mirrored from Teams, only while mirroring", async ({ page }) => {
  await fakeTauri(page, `{}`);
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  const pill = page.getByRole("status");
  await emit(page, "meeting", { started: Date.now(), mic: "live", words: "Mic live · mirrored from Teams" });
  await expect(pill).toContainText("Mic live · mirrored from Teams");
  await emit(page, "meeting", { started: Date.now(), mic: "muted", words: "Muted · mirrored from Teams" });
  await expect(pill).toContainText("Muted · mirrored from Teams");
  await emit(page, "meeting", { started: Date.now(), mic: null, words: "Mic live" });
  await expect(pill).toHaveText(/Recording\d+:\d+Mic live$/);
});

test("the recording pill shows Recording, the timer and Mic live, and goes when it stops", async ({ page }) => {
  await fakeTauri(page, `{}`);
  await page.clock.install({ time: 100_000 });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  const pill = page.getByRole("status");
  await expect(pill).toBeHidden();
  await emit(page, "meeting", { started: 100_000 - 65_000, mic: null, words: "Mic live" });
  await expect(pill).toHaveText("Recording1:05Mic live");
  await page.clock.runFor(1000);
  await expect(pill).toHaveText("Recording1:06Mic live");
  await emit(page, "meeting", { started: null });
  await expect(pill).toBeHidden();
});

test("after Stop the pill shows the notes being written with progress, then Notes saved, which goes away", async ({ page }) => {
  await fakeTauri(page, `{}`);
  await page.clock.install({ time: 100_000 });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  const pill = page.getByRole("status");
  await emit(page, "meeting-notes", { id: 7, state: "transcribing", progress: 0, error: null });
  await expect(pill).toHaveText("Writing up your notes…0%");
  await emit(page, "meeting-notes", { id: 7, state: "transcribing", progress: 0.42, error: null });
  await expect(pill).toHaveText("Writing up your notes…42%");
  await expect(pill.locator(".bar i")).toHaveAttribute("style", /width: 42%/);
  await emit(page, "meeting-notes", { id: 7, state: "done", progress: 1, error: null });
  await expect(pill).toContainText("Notes saved");
  await page.clock.runFor(7500);
  await expect(pill).toBeVisible();
  await page.clock.runFor(1000);
  await expect(pill).toBeHidden();
});

test("Open on Notes saved opens the meeting, and the dismiss button just closes it", async ({ page }) => {
  await fakeTauri(page, `{ meeting_open: () => {} }`);
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  await emit(page, "meeting-notes", { id: 7, state: "done", progress: 1, error: null });
  await page.getByRole("button", { name: "Open" }).click();
  expect(await calls(page, "meeting_open")).toEqual([{ id: 7 }]);
  await expect(page.getByRole("status")).toBeHidden();
  await emit(page, "meeting-notes", { id: 8, state: "done", progress: 1, error: null });
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByRole("status")).toBeHidden();
  expect(await calls(page, "meeting_open")).toHaveLength(1);
});

test("a failure says why and stays; Try again asks the backend and the pill follows its progress", async ({ page }) => {
  await fakeTauri(page, `{ meeting_retry: () => {} }`);
  await page.clock.install({ time: 100_000 });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  const pill = page.getByRole("status");
  await emit(page, "meeting-notes", { id: 7, state: "failed", progress: 0, error: "Download Parakeet v3 in Settings › Speech models" });
  await expect(pill).toHaveText("Couldn’t write up your notesDownload Parakeet v3 in Settings › Speech modelsTry again");
  await page.clock.runFor(60_000);
  await expect(pill).toBeVisible();
  await pill.getByRole("button", { name: "Try again" }).click();
  expect(await calls(page, "meeting_retry")).toEqual([{ id: 7 }]);
  await emit(page, "meeting-notes", { id: 7, state: "transcribing", progress: 0, error: null });
  await expect(pill).toHaveText("Writing up your notes…0%");
});

test("a retry the backend refuses shows its reason", async ({ page }) => {
  await fakeTauri(page, `{ meeting_retry: () => { throw "That meeting isn't waiting to be tried again."; } }`);
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  await emit(page, "meeting-notes", { id: 7, state: "failed", progress: 0, error: "x" });
  await page.getByRole("button", { name: "Try again" }).click();
  await expect(page.getByRole("status")).toContainText("That meeting isn't waiting to be tried again.");
});

test("Browser add-on: not connected shows the steps, connected flips live, Disconnect gives a new code", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const group = page.getByRole("region", { name: "Browser add-on" });
  const status = group.getByRole("listitem").first();
  await expect(status).toContainText("Not connected");
  await expect(status).toContainText("Add it to Chrome to keep your voice when muted in Teams.");
  await expect(group).toContainText("MYNA-4F7Q-K2");
  const connect = group.getByRole("button", { name: "Connect" });
  await expect(connect).toHaveAttribute("aria-expanded", "false");
  const before = await status.boundingBox();
  const connectWidth = (await connect.boundingBox())!.width;
  await connect.click();
  await expect(connect).toHaveAttribute("aria-expanded", "true");
  await expect(group).toContainText("Load unpacked");
  await group.getByRole("button", { name: "Show folder" }).click();
  expect(await calls(page, "addon_folder")).toHaveLength(1);

  await emit(page, "addon-status", true);
  await expect(status).toContainText("Connected");
  await expect(status).not.toContainText("Not connected");
  await expect(status).toContainText("Sees when you’re muted in Teams, and who is talking.");
  await expect(group).not.toContainText("Load unpacked");
  // Connect and Disconnect swap words, not the row's height or the button's width.
  expect((await status.boundingBox())!.height).toBe(before!.height);
  expect((await group.getByRole("button", { name: "Disconnect" }).boundingBox())!.width).toBe(connectWidth);
  await group.getByRole("button", { name: "Disconnect" }).click();
  await expect(status).toContainText("Not connected");
  await expect(group).toContainText("MYNA-NEW1-23");
  expect(await calls(page, "addon_disconnect")).toHaveLength(1);
});

test("Browser add-on: the state is there on the first frame, read before the page paints", async ({ page }) => {
  // The add-on answers 600ms late; the Meeting notes page must not show until it has.
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "allowed", { connected: true, code: "MYNA-4F7Q-K2" }, "", 600);
  const group = page.getByRole("region", { name: "Browser add-on" });
  await expect(group.getByRole("listitem").first()).toContainText("Connected");
  await expect(group.locator(".mono")).toHaveText("MYNA-4F7Q-K2");
  expect(await calls(page, "addon_get")).toHaveLength(1);
  // Going away and back doesn't ask again and shows the state the page kept.
  await emit(page, "addon-status", false);
  await page.getByRole("option", { name: "Listen" }).click();
  await emit(page, "addon-status", true);
  await page.getByRole("option", { name: "Meeting notes" }).click();
  await expect(group.getByRole("listitem").first()).toContainText("Connected");
  expect(await calls(page, "addon_get")).toHaveLength(1);
});

test("Browser add-on: Status is one 60px row, connected or not, and has no selector version", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const group = page.getByRole("region", { name: "Browser add-on" });
  const status = group.getByRole("listitem").first();
  expect(await heights(status)).toEqual([60]);
  await emit(page, "addon-status", true);
  await expect(status).toContainText("Connected");
  expect(await heights(status)).toEqual([60]);
  await expect(group).not.toContainText("selectors");
});

test("Browser add-on: Copy puts the code on the clipboard, and Copied doesn't move the row", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const group = page.getByRole("region", { name: "Browser add-on" });
  const row = group.getByRole("listitem").filter({ hasText: "Pairing code" });
  await expect(row).toContainText("Paste this into the add-on.");
  const box = (l: Locator) => l.evaluate((e) => JSON.stringify(e.getBoundingClientRect()));
  const copy = group.getByRole("button", { name: "Copy" });
  const [rowBefore, btnBefore, codeBefore] = [await box(row), await box(copy), await box(group.locator(".mono"))];
  await copy.click();
  const copied = group.getByRole("button", { name: "Copied" });
  await expect(copied).toBeVisible();
  expect([await box(row), await box(copied), await box(group.locator(".mono"))]).toEqual([rowBefore, btnBefore, codeBefore]);
  expect(await calls(page, "text_copy")).toEqual([{ text: "MYNA-4F7Q-K2" }]);
});

test("Browser add-on: a failed read says why on the group's own line, and Copy has nothing to copy", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "allowed", { connected: false, code: "" }, `addon_get: () => { throw "The add-on's folder is missing."; }`);
  const group = page.getByRole("region", { name: "Browser add-on" });
  await expect(group.getByText("The add-on's folder is missing.")).toBeVisible();
  await expect(group.getByRole("button", { name: "Copy" })).toBeDisabled();
});

test("Browser add-on: hidden while meeting notes are off", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("region", { name: "Browser add-on" })).toBeHidden();
});

test("Models: the speech-to-text model and Speaker diarization, with its model's name beside the title", async ({ page }) => {
  const model = (id: string, name: string, installed: boolean) => ({ id, name, vendor: "V", desc: "d", acc: 4, spd: 4, default: true, size: 500_000_000, min_macos: 0, repo: "r", revision: "x", files: [{}], system: false, runtime: false, installed, needs: null });
  const tasks = JSON.stringify([
    { id: "stt", active: "p", models: [model("p", "Model P", true)] },
    { id: "meeting_stt", active: "p", models: [model("p", "Model P", true)] },
    { id: "diar", active: "d", models: [model("d", "Model D", false)] },
  ]);
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "allowed", { connected: false, code: "C" }, `models_list: () => (${tasks})`);
  const group = page.getByRole("region", { name: "Models" });
  await expect(group.getByRole("listitem").filter({ hasText: "Speech-to-text model" })).toContainText("Model P · Installed · 500 MB");
  const diar = group.getByRole("listitem").filter({ hasText: "Speaker diarization" });
  await expect(diar).toContainText("Model D");
  await expect(diar).toContainText("Not downloaded · 500 MB");
  expect(await heights(group.getByRole("listitem"))).toEqual([60, 60]);
  await diar.getByRole("button", { name: "All models" }).click();
  await expect(page.getByRole("heading", { name: "Speaker diarization", level: 1 })).toBeVisible();
});
