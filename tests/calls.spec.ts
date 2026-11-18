import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

// The pill's side of call detection (#214): the backend's watcher sends `call`; the buttons answer with commands.
async function open(page: Page) {
  await fakeTauri(page, `{ pill_look_get: () => ({ style: "capsule", place: "bottom" }) }`);
  await page.clock.install();
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(12);
}
const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);
const called = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__calls.filter((x: any) => x[0] === c).length, cmd);

test("a detected call offers Start notes, which starts it", async ({ page }) => {
  await open(page);
  await emit(page, "call", { state: "offer", title: "Huddle detected", label: "Slack" });
  const pill = page.getByRole("status").last();
  await expect(pill).toContainText("Huddle detected");
  await expect(pill).toContainText("Slack");
  await page.screenshot({ path: "test-results/call-offer.png" });
  await page.getByRole("button", { name: "Start notes" }).click();
  expect(await called(page, "call_start")).toBe(1);
  await expect(page.getByRole("button", { name: "Start notes" })).toBeHidden();
});

test("dismissing the offer tells the backend, and Withdraw takes it away", async ({ page }) => {
  await open(page);
  await emit(page, "call", { state: "offer", title: "Meeting detected", label: "Microsoft Teams" });
  await page.getByRole("button", { name: "Dismiss" }).click();
  expect(await called(page, "call_dismiss")).toBe(1);
  await emit(page, "call", { state: "offer", title: "Meeting detected", label: "Microsoft Teams" });
  await emit(page, "call", { state: "none" });
  await expect(page.getByRole("button", { name: "Start notes" })).toBeHidden();
});

test("the call ending counts down over the recording, with Keep recording and Stop now", async ({ page }) => {
  await open(page);
  const now = await page.evaluate(() => Date.now());
  await emit(page, "meeting", { started: now - 60_000, mic: null, words: "Mic live" });
  await emit(page, "call", { state: "ending", at: now + 30_000 });
  const pill = page.getByRole("status").last();
  await expect(pill).toContainText("Call ended");
  await expect(pill).toContainText("Stopping in 30 s");
  await page.clock.runFor(5_000);
  await expect(pill).toContainText("Stopping in 25 s");
  await page.screenshot({ path: "test-results/call-ending.png" });
  await page.getByRole("button", { name: "Keep recording" }).click();
  expect(await called(page, "call_keep")).toBe(1);
  await expect(pill).toContainText("Recording");
  await emit(page, "call", { state: "ending", at: now + 30_000 });
  await page.getByRole("button", { name: "Stop now" }).click();
  expect(await called(page, "call_stop")).toBe(1);
});
