import { invoke } from "@tauri-apps/api/core";

/** `liquid`: Liquid Glass's own switch; null below macOS 26, where there's none.
 *  `experimental`: Experimental features' master switch (travels with the look for now, see look.rs).
 *  `nerds`: Stats for nerds' own switch. */
export type Look = { theme: "auto" | "light" | "dark"; accent: string; glass: "clear" | "tinted"; liquid: boolean | null; experimental: boolean; nerds: boolean };
export const DEFAULT_LOOK: Look = { theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: false, nerds: false };

/** Liquid Glass shows only while both it and experimental features are on (look.rs `glass_on`). */
export const glassOn = (l: Look) => l.experimental && l.liquid === true;

/** design/tokens.css restyles everything from these attributes; data-liquid drops the pill's CSS glass. */
export function applyLook(l: Look) {
  const { theme, accent, glass } = l;
  Object.assign(document.documentElement.dataset, { theme, accent, glass });
  document.documentElement.toggleAttribute("data-liquid", glassOn(l));
}

/** Liquid Glass: one sheet of macOS's glass for the whole window in place of its vibrancy; the panels
 *  turn into light frosted fills on it (tokens.css). The backend pins it to the window, so a live resize follows. */
export function syncGlass(l: Look | null) {
  invoke("glass_set", { glass: l && glassOn(l) ? [{ x: 0, y: 0, w: innerWidth, h: innerHeight, radius: 0, shown: true, grow: true }] : [] });
}
