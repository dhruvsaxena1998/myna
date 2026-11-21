import { invoke } from "@tauri-apps/api/core";
import type { Task } from "$lib/models";
import type { Perms } from "$lib/Permissions.svelte";
import type { Voice } from "$lib/VoiceShortcut.svelte";
import type { Notes } from "$lib/MeetingNotes.svelte";

// Load before the first paint; "onboarding-start" loads it again when Settings opens setup.
export async function load() {
  const [perms, voice, notes, tasks, globe, addon] = await Promise.all([
    invoke<Perms>("permissions_state"),
    invoke<Voice>("voice_get"),
    invoke<Notes>("meeting_get"),
    invoke<Task[]>("models_list"),
    invoke<boolean>("globe_ok"),
    invoke<{ connected: boolean; code: string }>("addon_get").catch(() => ({ connected: false, code: "" })),
  ]);
  return { perms, voice, notes, tasks, globe, addon };
}
