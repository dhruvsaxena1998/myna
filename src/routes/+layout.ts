import { invoke } from "@tauri-apps/api/core";
import { applyLook, DEFAULT_LOOK, type Look } from "$lib/look";

// No Node server in Tauri: render on the client, prerender a shell per route (settings.html).
export const ssr = false;
export const prerender = true;

// Every window takes the saved look before its first paint, so nothing flashes from the default.
export async function load() {
  const appearance = await invoke<Look>("look_get").then(
    (look) => ({ ok: true as const, look }),
    (err) => ({ ok: false as const, error: String(err) }),
  );
  applyLook(appearance.ok ? appearance.look : DEFAULT_LOOK);
  return { appearance };
}
