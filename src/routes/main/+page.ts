import { invoke } from "@tauri-apps/api/core";
import type { Row, Trace } from "$lib/trace";
import type { Rules } from "$lib/Words.svelte";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const storage = await invoke<string>("storage_status").then(
    () => ({ ok: true as const }),
    (err) => ({ ok: false as const, error: String(err) }),
  );
  const [traces, all, words] = storage.ok
    ? await Promise.all([
        invoke<Row[]>("traces_list").then(
          (rows) => ({ ok: true as const, rows }),
          (err) => ({ ok: false as const, error: String(err) }),
        ),
        // Stats for nerds: every trace with its steps, and the Words to remember list its Overview checks.
        invoke<Trace[]>("traces_all").then(
          (traces) => ({ ok: true as const, traces }),
          (err) => ({ ok: false as const, error: String(err) }),
        ),
        invoke<Rules>("rules_get").then((r) => r.settings.words, () => []),
      ])
    : [null, null, []];
  return { storage, traces, all, words };
}
