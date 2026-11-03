import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { Task } from "$lib/models";
import type { Perms } from "$lib/Permissions.svelte";
import type { Look } from "$lib/Pill.svelte";
import type { Voice } from "$lib/VoiceShortcut.svelte";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const [storage, version, perms] = await Promise.all([
    invoke<string>("storage_status").then(
      (path) => ({ ok: true as const, path }),
      (err) => ({ ok: false as const, error: String(err) }),
    ),
    getVersion(),
    invoke<Perms>("permissions_state"),
  ]);
  const models = storage.ok
    ? await Promise.all([invoke<Task[]>("models_list"), invoke<number>("stt_unload_get")]).then(
        ([tasks, unload]) => ({ ok: true as const, tasks, unload }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  const look = storage.ok
    ? await invoke<Look>("pill_look_get").then(
        (look) => ({ ok: true as const, look }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  const voice = storage.ok
    ? await invoke<Voice>("voice_get").then(
        (voice) => ({ ok: true as const, voice }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  return { storage, version, models, perms, look, voice };
}
