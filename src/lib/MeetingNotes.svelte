<!-- Meeting notes' switch, its shortcut and permissions (prototype: the meetings page, `case 'meetings'`). -->
<script lang="ts" module>
  /** `key` is an accelerator, or "" for no shortcut. */
  export type Notes = { on: boolean; key: string; keep_teams: boolean };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import BrowserAddon from "$lib/BrowserAddon.svelte";
  import Permissions, { type Perms } from "$lib/Permissions.svelte";

  let { notes = $bindable(), perms }: { notes: Notes; perms: Perms } = $props();
  let error = $state("");
  const KEYS: [string, string][] = [["Ctrl+Alt+KeyM", "⌃⌥M"], ["Ctrl+Alt+KeyN", "⌃⌥N"], ["", "No shortcut"]];

  async function save(next: Notes) {
    const was = notes;
    notes = next;
    error = "";
    try {
      await invoke("meeting_set", { notes: next });
    } catch (err) {
      notes = was;
      error = String(err);
    }
  }
</script>

<section aria-labelledby="meet-h">
  <h2 id="meet-h" class="sr-only">Meeting notes</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="meet-on">Use meeting notes</label><span class="desc">Record your calls and get notes, a summary and answers to your questions.</span></span>
      <input id="meet-on" type="checkbox" role="switch" class="switch" checked={notes.on} onchange={(e) => save({ ...notes, on: e.currentTarget.checked })} />
    </li>
  </ul>
  {#if !notes.on}<p class="err" role="status">{error}</p>{/if}
</section>

{#if notes.on}
  <section aria-labelledby="meet-key-h">
    <h2 id="meet-key-h">Shortcut</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="meet-key">Start or stop notes</label><span class="desc">Works from any app, even when the Myna window is closed.</span></span>
        <select id="meet-key" class="select" value={notes.key} onchange={(e) => save({ ...notes, key: e.currentTarget.value })}>
          {#each KEYS as [v, label]}<option value={v}>{label}</option>{/each}
        </select>
      </li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <section aria-labelledby="keep-h">
    <h2 id="keep-h">Keep my voice when I’m muted</h2>
    <p class="help">Normally, anything you say while muted in a call is left out of the notes. Turn on an app below to keep it in.</p>
    <h3>Myna sees your mute button through the browser add-on</h3>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="keep-teams">Microsoft Teams on the web</label><span class="desc">Keeps your voice in the notes even while you’re muted.</span></span>
        <input id="keep-teams" type="checkbox" role="switch" class="switch" aria-label="Keep my voice when muted in Microsoft Teams on the web" checked={notes.keep_teams} onchange={(e) => save({ ...notes, keep_teams: e.currentTarget.checked })} />
      </li>
    </ul>
  </section>
  <BrowserAddon />
  <Permissions initial={perms} kinds={["mic", "audio"]} />
{/if}

