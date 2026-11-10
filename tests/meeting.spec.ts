import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// `meeting_set` fails for Ctrl+Alt+N, like a shortcut another app holds. Asking for System Audio allows it.
async function open(page: Page, notes: { on: boolean; key: string; keep_teams?: boolean } = { on: false, key: "Ctrl+Alt+KeyM" }, audio = "allowed", addon = { connected: false, code: "MYNA-4F7Q-K2", selectors: null as string | null }) {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    meeting_get: () => (${JSON.stringify(notes)}),
    addon_get: () => (${JSON.stringify(addon)}),
    addon_disconnect: () => "MYNA-NEW1-23", addon_folder: () => {}, text_copy: () => {},
    meeting_set: ({ notes }) => { if (notes.key === "Ctrl+Alt+KeyN") throw "Another app is using that shortcut. Pick a different one."; },
    permissions_state: () => ({ ...(window.__perms ??= { mic: "allowed", ax: true, audio: ${JSON.stringify(audio)} }) }),
    permission_ask: ({ kind }) => { if (kind === "audio" && window.__perms.audio === "ask") window.__perms.audio = "allowed"; },
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

test("Keep my voice when I'm muted has a Teams switch that saves with the other settings", async ({ page }) => {
  await open(page, { on: false, key: "Ctrl+Alt+KeyM", keep_teams: false });
  await expect(page.getByText("Keep my voice when I’m muted")).toBeHidden();
  await page.getByRole("switch", { name: "Use meeting notes" }).click();
  await expect(page.getByRole("heading", { name: "Keep my voice when I’m muted" })).toBeVisible();
  const keep = page.getByRole("switch", { name: "Keep my voice when muted in Microsoft Teams on the web" });
  await expect(keep).not.toBeChecked();
  await keep.click();
  await expect(keep).toBeChecked();
  expect((await calls(page, "meeting_set")).at(-1)).toEqual({ notes: { on: true, key: "Ctrl+Alt+KeyM", keep_teams: true } });
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

test("System Audio shows on the page once notes are on, and Allow asks macOS", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "ask");
  const row = page.getByRole("listitem").filter({ hasText: "Screen & System Audio Recording" });
  await expect(row).toContainText("So Myna can hear the other people on a call.");
  await expect(page.getByRole("listitem").filter({ hasText: "Accessibility" })).toHaveCount(0);
  await row.getByRole("button", { name: "Allow Screen & System Audio Recording" }).click();
  await expect(row).toContainText("Allowed");
  expect(await calls(page, "permission_ask")).toEqual([{ kind: "audio" }]);
});

test("System Audio says plainly when Myna can't check it, and offers System Settings", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "unknown");
  const row = page.getByRole("listitem").filter({ hasText: "Screen & System Audio Recording" });
  await expect(row).toContainText("Myna can’t check this one. Your Mac asks the first time you record a meeting.");
  await expect(row).not.toContainText("Allowed");
  await expect(row.getByRole("button", { name: "Open System Settings for Screen & System Audio Recording" })).toBeVisible();
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
  await emit(page, "meeting", { started: Date.now(), mic: "live" });
  await expect(pill).toContainText("Mic live · mirrored from Teams");
  await emit(page, "meeting", { started: Date.now(), mic: "muted" });
  await expect(pill).toContainText("Muted · mirrored from Teams");
  await emit(page, "meeting", { started: Date.now(), mic: null });
  await expect(pill).toHaveText(/Recording\d+:\d+Mic live$/);
});

test("the recording pill shows Recording, the timer and Mic live, and goes when it stops", async ({ page }) => {
  await fakeTauri(page, `{}`);
  await page.clock.install({ time: 100_000 });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(9);
  const pill = page.getByRole("status");
  await expect(pill).toBeHidden();
  await emit(page, "meeting", { started: 100_000 - 65_000 });
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
  await expect(group).toContainText("Not connected.");
  await expect(group).toContainText("MYNA-4F7Q-K2");
  await group.getByRole("button", { name: "Connect" }).click();
  await expect(group).toContainText("Load unpacked");
  await group.getByRole("button", { name: "Show folder" }).click();
  expect(await calls(page, "addon_folder")).toHaveLength(1);

  await emit(page, "addon-status", true);
  await expect(group).toContainText("Connected.");
  await expect(group).not.toContainText("Load unpacked");
  await group.getByRole("button", { name: "Disconnect" }).click();
  await expect(group).toContainText("Not connected.");
  await expect(group).toContainText("MYNA-NEW1-23");
  expect(await calls(page, "addon_disconnect")).toHaveLength(1);
});

test("Browser add-on: Connected shows the selector version the add-on reports", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" }, "allowed", { connected: true, code: "MYNA-4F7Q-K2", selectors: "2026-11-10" });
  const group = page.getByRole("region", { name: "Browser add-on" });
  await expect(group).toContainText("Connected · selectors 2026-11-10.");
});

test("Browser add-on: Copy puts the code on the clipboard", async ({ page }) => {
  await open(page, { on: true, key: "Ctrl+Alt+KeyM" });
  const group = page.getByRole("region", { name: "Browser add-on" });
  await group.getByRole("button", { name: "Copy" }).click();
  await expect(group.getByRole("button", { name: "Copied" })).toBeVisible();
  expect(await calls(page, "text_copy")).toEqual([{ text: "MYNA-4F7Q-K2" }]);
});

test("Browser add-on: hidden while meeting notes are off", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("region", { name: "Browser add-on" })).toBeHidden();
});
