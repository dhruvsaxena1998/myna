import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { Task } from "$lib/models";
import type { Perms } from "$lib/Permissions.svelte";
import type { Look } from "$lib/Pill.svelte";
import type { Row } from "$lib/trace";
import type { Rules } from "$lib/Words.svelte";
import type { Voice } from "$lib/VoiceShortcut.svelte";
import type { Prefs, UpdateState } from "$lib/updates.svelte";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const [storage, version, perms, globe] = await Promise.all([
    invoke<string>("storage_status").then(
      (path) => ({ ok: true as const, path }),
      (err) => ({ ok: false as const, error: String(err) }),
    ),
    getVersion(),
    invoke<Perms>("permissions_state"),
    invoke<boolean>("globe_ok"),
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
  const rules = storage.ok
    ? await invoke<Rules>("rules_get").then(
        (rules) => ({ ok: true as const, rules }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  const updates = storage.ok
    ? await Promise.all([invoke<Prefs>("updates_get"), invoke<UpdateState>("update_state")]).then(
        ([prefs, status]) => ({ ok: true as const, prefs, status }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  const traces = storage.ok
    ? await invoke<Row[]>("traces_list").then(
        (rows) => ({ ok: true as const, rows }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  return { storage, version, models, perms, globe, look, voice, rules, updates, traces };
}
