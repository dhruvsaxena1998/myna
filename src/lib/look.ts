export type Look = { theme: "auto" | "light" | "dark"; accent: string; glass: "clear" | "tinted" };
export const DEFAULT_LOOK: Look = { theme: "auto", accent: "blue", glass: "clear" };

/** design/tokens.css restyles everything from these three attributes. */
export function applyLook(l: Look) {
  Object.assign(document.documentElement.dataset, l);
}
