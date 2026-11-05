import { invoke } from "@tauri-apps/api/core";
import type { Row } from "$lib/trace";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const storage = await invoke<string>("storage_status").then(
    () => ({ ok: true as const }),
    (err) => ({ ok: false as const, error: String(err) }),
  );
  const traces = storage.ok
    ? await invoke<Row[]>("traces_list").then(
        (rows) => ({ ok: true as const, rows }),
        (err) => ({ ok: false as const, error: String(err) }),
      )
    : null;
  return { storage, traces };
}
