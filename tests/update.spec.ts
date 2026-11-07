import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const NOTES = "## Dictation\n\n- Words appear **faster**.\n- Fixed `the pill`.\n\n[Full changelog](https://github.com/dhruvsaxena1998/myna/releases)";
const found = { version: "0.0.2-beta.2", notes: NOTES };

/** The pop-up showing `status`. `window.__state` replaces what `update_state` answers from then on. */
async function open(page: Page, status: object, extra = "") {
  await fakeTauri(page, `{
    update_state: () => window.__state ?? ${JSON.stringify(status)},
    update_check: () => null, update_cancel: () => null, open_link: () => null,
    // Never answers: the app restarts.
    update_restart: () => new Promise(() => {}),
    ${extra}
  }`);
  await page.goto("/update");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.some((c: any) => c[0] === "plugin:event|listen" && c[1].event === "update-open"))).toBe(true);
  await page.evaluate(() => (window as any).__emit("update-open", null));
}
const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as any).__calls.filter((c: any) => c[0] === cmd), cmd);
const hidden = async (page: Page) => (await calls(page, "plugin:window|hide")).length;
const buttons = (page: Page) => page.locator("footer button");

test("an update shows its version, the Channel and What's new, with focus on Download and restart", async ({ page }) => {
  await open(page, { state: "available", ...found });
  await expect(page.getByRole("heading", { name: "Myna 0.0.2-beta.2 is available" })).toBeVisible();
  await expect(page.getByText("You have 0.1.0 · Beta")).toBeVisible();
  const notes = page.getByLabel("What’s new");
  await expect(notes.getByRole("heading", { name: "Dictation" })).toBeVisible();
  await expect(notes.getByRole("listitem")).toHaveText(["Words appear faster.", "Fixed the pill."]);
  await expect(buttons(page)).toHaveText(["Later", "Download and restart"]);
  await expect(page.getByRole("button", { name: "Download and restart" })).toBeFocused();

  await emit(page, "update-channel", "stable");
  await expect(page.getByText("You have 0.1.0 · Stable")).toBeVisible();
});

test("a link opens in the browser, never in the window", async ({ page }) => {
  await open(page, { state: "available", ...found });
  await page.getByRole("link", { name: "Full changelog" }).click();
  expect(await calls(page, "open_link")).toEqual([["open_link", { url: "https://github.com/dhruvsaxena1998/myna/releases" }]]);
  expect(new URL(page.url()).pathname).toBe("/update");
});

test("notes from the feed can't run HTML", async ({ page }) => {
  const notes = '<script>window.__pwned = 1</script>\n\n<img src="x" onerror="window.__pwned = 1">\n\n[click](javascript:window.__pwned=1)';
  await open(page, { state: "available", version: "0.0.2-beta.2", notes });
  const box = page.getByLabel("What’s new");
  await expect(box).toContainText("<script>window.__pwned = 1</script>");
  await expect(box).toContainText('<img src="x" onerror="window.__pwned = 1">');
  await expect(box.locator("script, img")).toHaveCount(0);
  await expect(box.getByRole("link")).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__pwned)).toBeUndefined();
});

test("Later and Escape close the window", async ({ page }) => {
  await open(page, { state: "available", ...found });
  await page.getByRole("button", { name: "Later" }).click();
  await page.keyboard.press("Escape");
  expect(await hidden(page)).toBe(2);
});

test("Download and restart shows progress and Cancel in place of the buttons, then restarts", async ({ page }) => {
  await open(page, { state: "available", ...found }, `update_download: () => new Promise((r) => (window.__finish = r))`);
  const foot = page.locator("footer");
  const before = await foot.boundingBox();
  await page.getByRole("button", { name: "Download and restart" }).click();
  await emit(page, "update-state", { state: "downloading", ...found, pct: 42 });
  await expect(page.getByRole("progressbar", { name: "Downloading update" })).toHaveAttribute("aria-valuenow", "42");
  await expect(foot).toContainText("42%");
  await expect(buttons(page)).toHaveText(["Cancel"]);
  expect(await foot.boundingBox()).toEqual(before);
  await expect(page.getByRole("heading", { name: "Myna 0.0.2-beta.2 is available" })).toBeVisible();

  await page.evaluate((f) => ((window as any).__state = { state: "downloaded", ...f }), found);
  await emit(page, "update-state", { state: "downloaded", ...found });
  await page.evaluate(() => (window as any).__finish(null));
  await expect(page.getByRole("status")).toHaveText("Restarting…");
  expect(await calls(page, "update_restart")).toHaveLength(1);
});

test("Cancel stops the download", async ({ page }) => {
  await open(page, { state: "downloading", ...found, pct: 10 });
  await page.getByRole("button", { name: "Cancel" }).click();
  expect(await calls(page, "update_cancel")).toHaveLength(1);
  await emit(page, "update-state", { state: "available", ...found });
  await expect(page.getByRole("button", { name: "Download and restart" })).toBeFocused();
});

test("a download that finishes while dictating stops on Restart now until dictation ends", async ({ page }) => {
  await open(page, { state: "available", ...found }, `update_download: () => new Promise((r) => (window.__finish = r))`);
  await page.getByRole("button", { name: "Download and restart" }).click();
  await emit(page, "dictation", { state: "listening", hold: true, key: "Alt+Space" });
  await page.evaluate((f) => ((window as any).__state = { state: "downloaded", ...f }), found);
  await emit(page, "update-state", { state: "downloaded", ...found });
  await page.evaluate(() => (window as any).__finish(null));

  const restart = page.getByRole("button", { name: "Restart now" });
  await expect(buttons(page)).toHaveText(["Later", "Restart now"]);
  await expect(restart).toBeDisabled();
  await expect(page.getByText("Finish recording first")).toBeVisible();
  expect(await calls(page, "update_restart")).toHaveLength(0);

  await emit(page, "dictation", { state: "writing" });
  await expect(restart).toBeDisabled();
  await emit(page, "dictation", { state: "added" });
  await expect(restart).toBeEnabled();
  await restart.click();
  await expect(page.getByRole("status")).toHaveText("Restarting…");
});

test("downloaded: Later, and Restart now says why if the backend refuses", async ({ page }) => {
  await open(page, { state: "downloaded", ...found }, `update_restart: () => { throw "Finish recording first"; }`);
  await expect(page.getByRole("heading", { name: "Myna 0.0.2-beta.2 is available" })).toBeVisible();
  await expect(page.getByLabel("What’s new")).toContainText("Words appear faster.");
  await expect(buttons(page)).toHaveText(["Later", "Restart now"]);
  await expect(page.getByRole("button", { name: "Restart now" })).toBeFocused();
  await page.getByRole("button", { name: "Restart now" }).click();
  await expect(page.getByText("Finish recording first")).toBeVisible();
  await page.getByRole("button", { name: "Later" }).click();
  expect(await hidden(page)).toBe(1);
});

test("up to date: the version and OK", async ({ page }) => {
  await open(page, { state: "uptodate" });
  await expect(page.getByRole("heading", { name: "Myna is up to date" })).toBeVisible();
  await expect(page.getByText("You’re on the latest version (0.1.0)")).toBeVisible();
  await expect(page.getByLabel("What’s new")).toHaveCount(0);
  await expect(buttons(page)).toHaveText(["OK"]);
  await expect(page.getByRole("button", { name: "OK" })).toBeFocused();
  await page.getByRole("button", { name: "OK" }).click();
  expect(await hidden(page)).toBe(1);
});

test("an error: Close, and Try again checks once more", async ({ page }) => {
  await open(page, { state: "error", kind: "check", message: "Couldn’t reach GitHub. Check your connection." });
  await expect(page.getByRole("heading", { name: "Couldn’t check for updates" })).toBeVisible();
  await expect(page.getByText("Couldn’t reach GitHub. Check your connection.")).toBeVisible();
  await expect(buttons(page)).toHaveText(["Close", "Try again"]);
  await expect(page.getByRole("button", { name: "Try again" })).toBeFocused();
  await page.getByRole("button", { name: "Try again" }).click();
  await expect.poll(async () => (await calls(page, "update_check")).length).toBe(1);
  await page.getByRole("button", { name: "Close" }).click();
  expect(await hidden(page)).toBe(2);
});

test("a bad signature: Close only", async ({ page }) => {
  await open(page, { state: "error", kind: "signature", message: "This update failed its security check and wasn’t installed." });
  await expect(page.getByRole("heading", { name: "Update not installed" })).toBeVisible();
  await expect(page.getByText("This update failed its security check and wasn’t installed.")).toBeVisible();
  await expect(buttons(page)).toHaveText(["Close"]);
  await expect(page.getByRole("button", { name: "Close" })).toBeFocused();
});

test("the default button has no focus ring until a key is pressed, then it does", async ({ page }) => {
  await open(page, { state: "available", ...found });
  const main = page.getByRole("button", { name: "Download and restart" });
  await expect(main).toBeFocused();
  await expect(main).toHaveCSS("outline-style", "none");
  await page.keyboard.press("Shift");
  await expect(main).not.toHaveCSS("outline-style", "none");
});
