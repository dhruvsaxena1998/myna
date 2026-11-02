import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const [storage, version] = await Promise.all([
    invoke<string>("storage_status").then(
      (path) => ({ ok: true as const, path }),
      (err) => ({ ok: false as const, error: String(err) }),
    ),
    getVersion(),
  ]);
  return { storage, version };
}
