/** `liquid`: Liquid Glass's own switch; null below macOS 26, where there's none.
 *  `experimental`: Experimental features' master switch (travels with the look for now, see look.rs). */
export type Look = { theme: "auto" | "light" | "dark"; accent: string; glass: "clear" | "tinted"; liquid: boolean | null; experimental: boolean };
export const DEFAULT_LOOK: Look = { theme: "auto", accent: "blue", glass: "clear", liquid: null, experimental: false };

/** Liquid Glass shows only while both it and experimental features are on (look.rs `glass_on`). */
export const glassOn = (l: Look) => l.experimental && l.liquid === true;

/** design/tokens.css restyles everything from these attributes; data-liquid drops the pill's CSS glass. */
export function applyLook(l: Look) {
  const { theme, accent, glass } = l;
  Object.assign(document.documentElement.dataset, { theme, accent, glass });
  document.documentElement.toggleAttribute("data-liquid", glassOn(l));
}
