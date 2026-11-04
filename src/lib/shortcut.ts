// Shortcuts are stored as accelerators like "Alt+Space" or "Ctrl+Shift+KeyD": the form the global
// shortcut and the menubar menu both read. These turn them into keycaps and back from a key press.
// "Fn" is fn on its own, which the backend watches for and tells the recorder about ("voice-fn").

const MODS: Record<string, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘", Fn: "fn" };
const PUNCT: Record<string, string> = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]",
  Backslash: "\\", Minus: "-", Equal: "=", Backquote: "`", Enter: "Return", ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
};

/** "Ctrl+Shift+KeyD" → ["⌃", "⇧", "D"]. */
export const keycaps = (accel: string) =>
  accel.split("+").map((k) => MODS[k] ?? PUNCT[k] ?? k.replace(/^(Key|Digit)/, ""));

/** Words for screen readers: "Control Shift D". */
export const spoken = (accel: string) =>
  accel.split("+").map((k) => ({ Ctrl: "Control", Alt: "Option", Shift: "Shift", Super: "Command" } as Record<string, string>)[k] ?? keycaps(k)[0]).join(" ");

/**
 * The accelerator for a key press, `null` while only modifiers are down (wait for the rest),
 * or an error when the key would stop typing its character everywhere.
 */
export function fromKey(e: KeyboardEvent): string | null | { error: string } {
  if (["Shift", "Control", "Alt", "Meta", "Fn"].includes(e.key)) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
  // Shift alone still types: ⇧A is just "A".
  const types = /^(Key|Digit)|^(Space|Tab|Backspace)$/.test(e.code) || e.code in PUNCT;
  if (types && mods.every((m) => m === "Shift")) return { error: "Add ⌘, ⌥ or ⌃, so the key still types normally." };
  return [...mods, e.code].join("+");
}
