// The add-on's Meet rules against tests/extension/meet.html in a real browser engine, so the CSS in selectors.json is
// matched for real (the node tests fake the DOM). Runs with `pnpm test:ui`.
import { expect, test, type Page } from "@playwright/test";
import table from "../../extension/selectors.json" with { type: "json" };

const here = (path: string) => new URL(path, import.meta.url).href;

async function meet(page: Page) {
  await page.goto(here("./meet.html"));
  await page.evaluate(async ([js, data]) => {
    const S = await import(js);
    S.useRules(S.validate(data).meet);
    (window as any).S = S;
  }, [here("../../extension/selectors.js"), table] as const);
}
const mute = (page: Page) => page.evaluate(() => (window as any).S.readPage(document));
const mic = (page: Page, label: string, muted: string) =>
  page.evaluate(([l, m]) => {
    const b = document.querySelector("button[aria-label*='microphone']")!;
    b.setAttribute("aria-label", l);
    b.setAttribute("data-is-muted", m);
  }, [label, muted]);

test("Meet: the mic button is read live and muted, never the camera's", async ({ page }) => {
  await meet(page);
  expect(await mute(page)).toBe("unmuted");
  await mic(page, "Turn on microphone (⌘ + d)", "true");
  expect(await mute(page)).toBe("muted");
  await page.evaluate(() => document.querySelector("button[aria-label*='camera']")!.setAttribute("data-is-muted", "false"));
  expect(await mute(page)).toBe("muted");
});

test("Meet: no mic button is unknown, never a guess", async ({ page }) => {
  await meet(page);
  await page.evaluate(() => document.querySelector("[aria-label='Call controls']")!.remove());
  expect(await mute(page)).toBe("unknown");
});

test("Meet: each caption gives its speaker and words; the region's own button isn't a line", async ({ page }) => {
  await meet(page);
  expect(await page.evaluate(() => (window as any).S.readCaptions(document))).toEqual([
    { name: "Avery Chen", text: "Kickoff is at 10 AM." },
    { name: "You", text: "I'll post notes in the doc." },
    { name: "Riley Park", text: "Sounds good, see you then." },
  ]);
  await page.evaluate(() => document.querySelector("[jsname='dsyhDe']")!.remove());
  expect(await page.evaluate(() => (window as any).S.readCaptions(document))).toBeNull();
});
