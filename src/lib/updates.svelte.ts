// The one update state (update.rs), kept here so it survives switching Settings pages.
import { listen } from "@tauri-apps/api/event";

export type Prefs = { mode: "auto" | "check" | "off"; channel: "stable" | "beta" };
export type UpdateState =
  | { state: "idle" | "checking" | "downloaded" | "uptodate" }
  | { state: "available"; version: string; notes: string }
  | { state: "downloading"; pct: number }
  | { state: "error"; kind: "check" | "download" | "install" | "signature"; message: string };

export const upd = $state({ s: { state: "idle" } as UpdateState });

let watching = false;
export function watchUpdates(initial: UpdateState) {
  if (watching) return;
  watching = true;
  upd.s = initial;
  listen<UpdateState>("update-state", ({ payload }) => {
    upd.s = payload;
  }).catch(() => (watching = false));
}
