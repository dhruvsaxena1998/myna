// Draws the menu bar icons from one glyph: src-tauri/icons/tray.png (a template image, macOS colours it) and the
// recording frames in src-tauri/icons/tray-rec/: the wave in Myna orange at 5 loudness levels, the rest black for a
// light menu bar or white for a dark one (a coloured image can't be a template). Run: node scripts/tray-icons.mjs
import { webkit } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";

const ORANGE = "#E8572A";
// The wave's height per loudness level; 0 is silence, a flat line.
const LEVELS = [0, 1.1, 2, 2.9, 3.7];
const wave = (a) => `M3 5q1.5-${a} 3 0t3 0 3 0 3 0`;
const svg = (a, waveColour, ink) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 18 18" width="44" height="44" fill="none" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">` +
  `<path d="${wave(a)}" stroke="${waveColour}"/><path d="M3 9.5h12M3 14h5.5M11.5 12.25v3.5" stroke="${ink}"/></svg>`;

const out = new URL("../src-tauri/icons/", import.meta.url);
mkdirSync(new URL("tray-rec/", out), { recursive: true });
const browser = await webkit.launch();
const page = await browser.newPage({ viewport: { width: 44, height: 44 } });
const png = async (markup) => {
  await page.setContent(`<style>html,body{margin:0;background:transparent}svg{display:block}</style>${markup}`);
  return page.locator("svg").screenshot({ omitBackground: true });
};
writeFileSync(new URL("tray.png", out), await png(svg(2.2, "#000", "#000")));
for (const [i, a] of LEVELS.entries()) {
  writeFileSync(new URL(`tray-rec/light-${i}.png`, out), await png(svg(a, ORANGE, "rgba(0,0,0,.85)")));
  writeFileSync(new URL(`tray-rec/dark-${i}.png`, out), await png(svg(a, ORANGE, "#fff")));
}
await browser.close();
