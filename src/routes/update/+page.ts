import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { Prefs, UpdateState } from "$lib/updates.svelte";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const [version, channel, status] = await Promise.all([
    getVersion(),
    invoke<Prefs>("updates_get").then((p) => p.channel, () => null),
    invoke<UpdateState>("update_state"),
  ]);
  return { version, channel, status };
}
