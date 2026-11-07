// Download state lives here, not in ModelTask, so it survives switching Settings pages (the component unmounts).
import { listen } from "@tauri-apps/api/event";

// "failed" is also a downloaded model that won't load; "loaded" is it loading fine again.
type Ev = { id: string; state: "progress" | "done" | "canceled" | "failed" | "loaded"; pct?: number; error?: string };

export const dl = $state({
  pct: {} as Record<string, number>, // only while downloading
  retry: {} as Record<string, boolean>, // canceled or failed: offer "Try again"
  installed: {} as Record<string, boolean>, // overrides the list loaded at startup
  errors: {} as Record<string, string>,
});

let watching = false;
export function watchDownloads() {
  if (watching) return;
  watching = true;
  listen<Ev>("model-state", ({ payload: p }) => {
    if (p.state === "progress") return void (dl.pct[p.id] = p.pct ?? 0);
    if (p.state === "loaded") return void delete dl.errors[p.id];
    delete dl.pct[p.id];
    dl.retry[p.id] = p.state !== "done";
    if (p.state === "done") dl.installed[p.id] = true;
    if (p.state === "failed") dl.errors[p.id] = p.error ?? "The download failed.";
    if (p.state === "done") delete dl.errors[p.id];
  }).catch(() => (watching = false));
}
