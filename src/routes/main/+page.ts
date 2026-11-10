import { invoke } from "@tauri-apps/api/core";
import type { Row, Trace } from "$lib/trace";
import type { Meeting, Running } from "$lib/meetings";
import type { Notes } from "$lib/MeetingNotes.svelte";
import type { Rules } from "$lib/Words.svelte";

// Load before the first paint so nothing pops in after the window shows.
export async function load() {
  const storage = await invoke<string>("storage_status").then(
    () => ({ ok: true as const }),
    (err) => ({ ok: false as const, error: String(err) }),
  );
  const [traces, all, words, meetings, notes, running] = storage.ok
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
        invoke<Meeting[]>("meetings_list").then(
          (rows) => ({ ok: true as const, rows }),
          (err) => ({ ok: false as const, error: String(err) }),
        ),
        invoke<Notes>("meeting_get").then((n) => n, () => ({ on: false, key: "", keep_teams: false })),
        invoke<Running>("meeting_running").then((r) => r, () => null),
      ])
    : [null, null, [], null, { on: false, key: "", keep_teams: false }, null];
  return { storage, traces, all, words, meetings, notes, running };
}
