import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import type { Task } from "$lib/models";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const [storage, version] = await Promise.all([
    invoke<string>("storage_status").then(
      (path) => ({ ok: true as const, path }),
      (err) => ({ ok: false as const, error: String(err) }),
    ),
    getVersion(),
  ]);
  const models = storage.ok
    ? await invoke<Task[]>("models_list").then((tasks) => ({ ok: true as const, tasks }), (err) => ({ ok: false as const, error: String(err) }))
    : null;
  return { storage, version, models };
}
