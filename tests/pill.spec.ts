import { expect, test, type Page } from "@playwright/test";
import { fakeTauri } from "./tauri";

const STYLES = ["capsule", "orb", "wave", "card"];
const PLACES = ["bottom", "top", "bottom-right", "top-right"];

async function open(page: Page, look = { style: "capsule", place: "bottom" }) {
  await fakeTauri(page, `{ pill_look_get: () => (${JSON.stringify(look)}) }`);
  await page.clock.install();
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(5); // the layout's and the page's "look" too
}
const emit = (page: Page, event: string, payload: unknown) => page.evaluate(([e, p]) => (window as any).__emit(e, p), [event, payload] as const);
const say = (page: Page, state: string, hold = true) => emit(page, "dictation", state === "listening" ? { state, hold, key: "Alt+Space", limit: 300 } : { state });
const fallback = (page: Page, reason: string) => emit(page, "dictation", { state: "fallback", reason });
const fail = (page: Page, title: string, hint: string) => emit(page, "dictation", { state: "error", title, hint });

test("hidden until dictation starts", async ({ page }) => {
  await open(page);
  await expect(page.getByRole("status")).toBeHidden();
});

test("Listening: level bars follow the mic, with Release to paste", async ({ page }) => {
  await open(page);
  await say(page, "listening");
  const pill = page.getByRole("status");
  await expect(pill).toHaveText("ListeningRelease ⌥Space to paste");
  const middle = pill.locator(".lvl i").nth(2);
  await expect(middle).toHaveCSS("transform", "matrix(1, 0, 0, 0.2, 0, 0)");
  await emit(page, "dictation-level", 1);
  await expect(middle).toHaveAttribute("style", /scaleY\(1\)/);
});

test("Writing, then Added where you were typing, then it goes away", async ({ page }) => {
  await open(page);
  const pill = page.getByRole("status");
  await say(page, "listening");
  await say(page, "writing");
  await expect(pill).toHaveText("Writing…");
  await say(page, "added");
  await expect(pill).toHaveText("Added where you were typing");
  await page.clock.runFor(1500);
  await expect(pill).toBeVisible();
  await page.clock.runFor(500);
  await expect(pill).toBeHidden();
});

for (const [title, hint] of [["No speech model", "Download Parakeet v3 in Settings › Speech models"], ["Needs the Microphone", "Allow it in Settings › Voice typing"], ["Didn’t hear anything", "Hold the keys while you talk"]]) {
  test(`error: ${title}`, async ({ page }) => {
    await open(page);
    await fail(page, title, hint);
    await expect(page.getByRole("status")).toHaveText(title + hint);
    await page.clock.runFor(4200);
    await expect(page.getByRole("status")).toBeHidden();
  });
}

test("Cleaning up…, then Added; a fallback says why and stays up longer than Added", async ({ page }) => {
  await open(page);
  const pill = page.getByRole("status");
  await say(page, "cleaning");
  await expect(pill).toHaveText("Cleaning up…");
  await say(page, "added");
  await expect(pill).toHaveText("Added where you were typing");
  await fallback(page, "AI took too long");
  await expect(pill).toHaveText("Added without cleanupAI took too long");
  await page.clock.runFor(2500);
  await expect(pill).toBeVisible();
  await page.clock.runFor(200);
  await expect(pill).toBeHidden();
});

test("a new dictation keeps the pill up past an earlier result's timer", async ({ page }) => {
  await open(page);
  await say(page, "added");
  await page.clock.runFor(1000);
  await say(page, "listening");
  await page.clock.runFor(1000);
  await expect(page.getByRole("status")).toHaveText("ListeningRelease ⌥Space to paste");
});

test("a tap hides the pill at once", async ({ page }) => {
  await open(page);
  await say(page, "listening");
  await say(page, "idle");
  await page.clock.runFor(300);
  await expect(page.getByRole("status")).toBeHidden();
});

// Every style says every state in words (Orb only to screen readers while things go well), at one height per style.
for (const style of STYLES) {
  test(`${style}: words for every state, one height throughout`, async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" }); // no enter transition, so boxes are final
    await open(page, { style, place: "bottom" });
    const pill = page.getByRole("status");
    const heights = new Set<number>();
    for (const [state, words] of [["listening", /Listening/], ["writing", /Writing…/], ["cleaning", /Cleaning up…/], ["added", /Added/]] as const) {
      await say(page, state);
      await expect(pill).toContainText(words);
      heights.add((await pill.locator(".hud").boundingBox())!.height);
    }
    await fail(page, "Needs Accessibility", "Allow it in Settings › Voice typing");
    await expect(pill).toContainText("Needs AccessibilityAllow it in Settings › Voice typing");
    if (style !== "orb") heights.add((await pill.locator(".hud").boundingBox())!.height); // Orb grows into a capsule to say it
    await fallback(page, "AI took too long");
    await expect(pill).toContainText("Added without cleanupAI took too long");
    if (style !== "orb") heights.add((await pill.locator(".hud").boundingBox())!.height);
    expect([...heights]).toEqual([{ capsule: 40, orb: 32, wave: 44, card: 56 }[style]]);
  });
}

test("Waveform scrolls the mic level and counts the seconds", async ({ page }) => {
  await open(page, { style: "wave", place: "bottom" });
  await say(page, "listening");
  const bars = page.locator(".trace i");
  await expect(bars).toHaveCount(48);
  await emit(page, "dictation-level", 1);
  await expect(bars.last()).toHaveAttribute("style", "height: 24px;");
  await expect(bars.first()).toHaveAttribute("style", "height: 3px;");
  await page.clock.runFor(4100);
  await expect(page.locator("time")).toHaveText("0:04");
});

test("sits against the edges of its position, and moves when the setting changes", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await open(page);
  await say(page, "listening");
  const vp = page.viewportSize()!;
  const at = async () => (await page.locator(".hud").boundingBox())!;
  for (const place of PLACES) {
    await emit(page, "pill-look", { style: "capsule", place });
    const b = await at();
    if (place.startsWith("top")) expect(b.y).toBe(12); else expect(vp.height - b.y - b.height).toBe(24);
    if (place.endsWith("right")) expect(vp.width - b.x - b.width).toBe(24); else expect(Math.abs(b.x + b.width / 2 - vp.width / 2)).toBeLessThan(1);
  }
  await emit(page, "pill-look", { style: "card", place: "bottom" });
  await expect(page.locator(".hud.card")).toBeVisible();
});

test("tap mode: Press again to paste, and the shortcut on the card", async ({ page }) => {
  await open(page, { style: "card", place: "bottom" });
  await say(page, "listening", false);
  await expect(page.getByRole("status")).toHaveText("Listening Press ⌥Space again to paste");
  await say(page, "listening", true);
  await expect(page.getByRole("status")).toHaveText("Listening Release ⌥Space to paste");
});

// Every style says how long is left in tap mode's last 30 s.
for (const [style, words] of [["capsule", "ListeningAdding in 30 s"], ["orb", "Listening30"], ["wave", "Listening4:30 left"], ["card", "Listening Adding in 30 s"]]) {
  test(`${style}: tap mode counts down its last 30 s`, async ({ page }) => {
    await open(page, { style, place: "bottom" });
    await say(page, "listening", false);
    await page.clock.runFor(269_000);
    await expect(page.getByRole("status")).not.toContainText(/Adding in|left|30$/);
    await page.clock.runFor(1_000);
    await expect(page.getByRole("status")).toHaveText(words.replace("4:30", "0:30"));
  });
}

test("hold mode never counts down", async ({ page }) => {
  await open(page);
  await say(page, "listening", true);
  await page.clock.runFor(290_000);
  await expect(page.getByRole("status")).toHaveText("ListeningRelease ⌥Space to paste");
});

// Liquid Glass: the backend puts macOS's glass where the page says the pill is.
async function openGlass(page: Page, look: object) {
  await fakeTauri(page, `{ look_get: () => (${JSON.stringify({ theme: "auto", accent: "green", glass: "clear", liquid: true, experimental: true, ...look })}) }`);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/pill");
  await expect.poll(() => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "plugin:event|listen").length)).toBe(5);
}
const lastGlass = (page: Page) => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "glass_set").at(-1)?.[1].glass?.[0] ?? null);

test("Liquid Glass: the glass follows the drawn pill, its corners, and whether it shows", async ({ page }) => {
  await openGlass(page, {});
  await expect.poll(() => lastGlass(page)).toMatchObject({ shown: false });
  await say(page, "listening");
  const b = (await page.locator(".hud").boundingBox())!;
  await expect.poll(() => lastGlass(page)).toEqual({ x: b.x, y: b.y, w: b.width, h: b.height, radius: 20, shown: true });
  await expect(page.locator(".hud")).toHaveCSS("background-color", "rgba(28, 28, 30, 0)"); // only the native glass shows
  await emit(page, "pill-look", { style: "card", place: "top-right" });
  const c = (await page.locator(".hud").boundingBox())!;
  await expect.poll(() => lastGlass(page)).toMatchObject({ x: c.x, y: c.y, w: c.width, h: c.height, radius: 18 });
});

test("Liquid Glass: a new tint re-dresses the glass, and turning it off takes the glass away", async ({ page }) => {
  await openGlass(page, { glass: "tinted" });
  await expect.poll(() => lastGlass(page)).toBeTruthy();
  const sends = () => page.evaluate(() => (window as any).__calls.filter((c: any) => c[0] === "glass_set").length);
  const before = await sends();
  await emit(page, "look", { theme: "auto", accent: "pink", glass: "tinted", liquid: true, experimental: true });
  await expect.poll(sends).toBeGreaterThan(before);
  await emit(page, "look", { theme: "auto", accent: "pink", glass: "tinted", liquid: true, experimental: false });
  await expect.poll(() => lastGlass(page)).toBeNull(); // the master switch alone takes it away
  await say(page, "listening");
  await expect(page.locator(".hud")).not.toHaveCSS("background-color", "rgba(0, 0, 0, 0)"); // today's pill again
});
