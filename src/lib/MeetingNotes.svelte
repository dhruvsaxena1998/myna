<!-- The Meeting notes page (docs/settings.html, VIEWS.meetings): the switch and its shortcut, the muted-voice switch, the browser add-on, models and permissions. -->
<script lang="ts" module>
  /** `key` is an accelerator, or "" for no shortcut. */
  export type Notes = { on: boolean; key: string; keep_teams: boolean };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import BrowserAddon from "$lib/BrowserAddon.svelte";
  import Permissions, { type Perms } from "$lib/Permissions.svelte";
  import ShortcutButton from "$lib/ShortcutButton.svelte";
  import { PRESS, Recorder } from "$lib/recorder.svelte";
  import { clash } from "$lib/shortcut";
  import type { Voice } from "$lib/VoiceShortcut.svelte";

  // `voice`: its keys can't be this one. `connected` and `code` are the add-on's, kept by the page so they stay current while away.
  // `models`: the Models group, which needs the page's model state.
  let { notes = $bindable(), voice, perms, connected = $bindable(), code = $bindable(), loadError, models }:
    { notes: Notes; voice: Voice | null; perms: Perms; connected: boolean; code: string; loadError: string; models: Snippet } = $props();
  let keepError = $state("");

  const rec = new Recorder<"notes">({
    fn: false,
    save: (_, key) => save({ ...notes, key }),
    check: (_, key) =>
      clash(key, voice ? [["Dictate", voice.key], ["Dictate", voice.key2], ["Dictate with cleanup", voice.clean], ["Dictate with cleanup", voice.clean2]] : []),
  });

  /** `keep`: the muted-voice switch, whose error shows under its own group. */
  async function save(next: Notes, keep = false) {
    const was = notes;
    notes = next;
    if (keep) keepError = "";
    else rec.error = "";
    try {
      await invoke("meeting_set", { notes: next });
    } catch (err) {
      notes = was;
      if (keep) keepError = String(err);
      else rec.error = String(err);
    }
  }
  const recording = $derived(rec.recording === "notes");
  const keysDesc = $derived(recording ? PRESS : "Works from any app, even when the window is closed.");
  const teamsDesc = $derived(connected ? "Myna sees your mute button through the browser add-on." : "Needs the browser add-on. Connect it below.");
</script>

<svelte:window onkeydown={rec.onkeydown} onblur={() => rec.record(null)} />

<section aria-labelledby="meet-h">
  <h2 id="meet-h" class="sr-only">Meeting notes</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="meet-on">Use meeting notes</label><span class="desc">Record your calls and get notes, a summary and answers to your questions.</span></span>
      <input id="meet-on" type="checkbox" role="switch" class="switch" checked={notes.on} onchange={(e) => save({ ...notes, on: e.currentTarget.checked })} />
    </li>
    {#if notes.on}
      <li class="row wide">
        <span><span class="name">Recording shortcut</span><span class="desc" title={keysDesc}>{keysDesc}</span></span>
        <span class="end">
          <ShortcutButton id="rec-notes" name="Recording shortcut" key={notes.key} {recording} onclick={() => rec.record(recording ? null : "notes")} />
          <!-- Its place stays when there's no key, so removing it moves nothing. -->
          <button class="btn quiet icon" title="Remove" aria-label="Remove the Recording shortcut" style:visibility={notes.key ? null : "hidden"} onclick={() => save({ ...notes, key: "" })}>
            <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg>
          </button>
        </span>
      </li>
    {/if}
  </ul>
  <p class="err" role="status">{rec.error}</p>
</section>

{#if notes.on}
  <section aria-labelledby="keep-h">
    <h2 id="keep-h">Keep my voice when muted</h2>
    <p class="help">What you say while muted is left out of the notes, unless you turn on the app here.</p>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="keep-teams">Microsoft Teams on the web</label><span class="desc" title={teamsDesc}>{teamsDesc}</span></span>
        <input id="keep-teams" type="checkbox" role="switch" class="switch" aria-label="Keep my voice when muted in Microsoft Teams on the web" disabled={!connected}
          checked={notes.keep_teams && connected} onchange={(e) => save({ ...notes, keep_teams: e.currentTarget.checked }, true)} />
      </li>
    </ul>
    <p class="err" role="status">{keepError}</p>
  </section>
  <BrowserAddon bind:connected bind:code {loadError} />
  {@render models()}
  <section aria-labelledby="perm-h">
    <h2 id="perm-h">Permissions</h2>
    <ul class="list"><Permissions initial={perms} kinds={["mic", "audio"]} rows /></ul>
  </section>
{/if}
