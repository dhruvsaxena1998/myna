<!-- Meeting notes' switch and its shortcut (prototype: the meetings page, `case 'meetings'`). -->
<script lang="ts" module>
  /** `key` is an accelerator, or "" for no shortcut. */
  export type Notes = { on: boolean; key: string };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { notes = $bindable() }: { notes: Notes } = $props();
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

<section class="group" aria-labelledby="meet-h">
  <h2 id="meet-h" class="sr">Meeting notes</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="meet-on">Use meeting notes</label><span class="desc">Record your calls and get notes, a summary and answers to your questions.</span></span>
      <input id="meet-on" type="checkbox" role="switch" class="switch" checked={notes.on} onchange={(e) => save({ ...notes, on: e.currentTarget.checked })} />
    </li>
  </ul>
  {#if !notes.on}<p class="err" role="status">{error}</p>{/if}
</section>

{#if notes.on}
  <section class="group" aria-labelledby="meet-key-h">
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
{/if}

<style>
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
