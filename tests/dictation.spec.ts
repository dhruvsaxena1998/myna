import { expect, test } from "@playwright/test";
import { fakeTauri } from "./tauri";

// Fresh install: macOS hasn't asked for the mic, Accessibility is off. `permission_ask` grants what it's asked for.
async function open(page: import("@playwright/test").Page, at = "Listen") {
  await fakeTauri(page, `{
    storage_status: () => "/tmp/Myna", models_list: () => [],
    permissions_state: () => ({ ...(window.__perms ??= { mic: "ask", ax: false }) }), // a copy, like JSON from Rust
    permission_ask: ({ kind }) => { if (kind === "mic") window.__perms.mic = "allowed"; },
  }`);
  await page.goto("/settings");
  await page.getByRole("option", { name: at }).click();
}

test("Allow asks macOS for the mic and shows the answer", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Allow Microphone" }).click();
  await expect(page.getByRole("listitem").filter({ hasText: "Microphone" })).toContainText("Allowed");
  expect(await page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "permission_ask"))).toEqual([["permission_ask", { kind: "mic" }]]);
});

// Microphone is on Listen; Accessibility moved to Write for now.
test("Accessibility refreshes when the user comes back from System Settings, without moving the rows", async ({ page }) => {
  await open(page, "Write");
  const row = page.getByRole("listitem").filter({ hasText: "Accessibility" });
  await row.getByRole("button", { name: "Open System Settings for Accessibility" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "permission_ask" && c[1].kind === "ax"))).toBe(true);
  await expect(row).toContainText("Switch Myna on in System Settings, then come back.");
  const before = [await row.boundingBox(), await row.getByText("Accessibility", { exact: true }).boundingBox()];
  await page.evaluate(() => { (window as any).__perms.ax = true; (window as any).__emit("tauri://focus", null); });
  await expect(row).toContainText("Allowed");
  await expect(row).toContainText("So Myna can put your words into your app.");
  expect([await row.boundingBox(), await row.getByText("Accessibility", { exact: true }).boundingBox()]).toEqual(before);
});

test("the stale-entry hint shows only after a trip to System Settings that didn't turn Accessibility on", async ({ page }) => {
  await open(page, "Write");
  const row = page.getByRole("listitem").filter({ hasText: "Accessibility" });
  await expect(row).not.toContainText("Still asking?");
  await page.evaluate(() => (window as any).__emit("tauri://focus", null)); // focus without going to System Settings
  await expect(row).not.toContainText("Still asking?");
  await page.getByRole("button", { name: "Open System Settings for Accessibility" }).click();
  await page.evaluate(() => (window as any).__emit("tauri://focus", null));
  await expect(row).toContainText("Still asking? Remove Myna from the list with −, then open System Settings again.");
  await page.evaluate(() => { (window as any).__perms.ax = true; (window as any).__emit("tauri://focus", null); });
  await expect(row).not.toContainText("Still asking?");
});

test("Listen shows only Microphone; Write shows only Accessibility", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("listitem").filter({ hasText: "Accessibility" })).toHaveCount(0);
  await page.getByRole("option", { name: "Write" }).click();
  await expect(page.getByRole("listitem").filter({ hasText: "Accessibility" })).toHaveCount(1);
  await expect(page.getByRole("listitem").filter({ hasText: "Microphone" })).toHaveCount(0);
});
