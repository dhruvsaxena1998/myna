<!-- Meeting notes › Meeting names: an AI names each meeting once it's written up, with the Title model (Fast unless picked). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Ai } from "./AiProviders.svelte";
  import ModelRow from "./ModelRow.svelte";

  type Prefs = { on: boolean; model: string; gone: boolean };
  let { ai }: { ai: Ai | null } = $props();
  let prefs = $state<Prefs | null>(null);
  let error = $state("");
  let rev = $state(0);

  // Read again whenever the providers change: removing one makes it Not set.
  $effect(() => {
    void ai;
    invoke<Prefs>("title_prefs_get").then((p) => (prefs = p), (e) => (error = String(e)));
  });
  async function save(next: Prefs) {
    const was = prefs;
    error = "";
    prefs = { ...next, gone: next.model === was?.model && !!was?.gone };
    try {
      await invoke("title_prefs_set", { prefs: next });
    } catch (err) {
      prefs = was;
      error = String(err);
    }
    rev++;
  }
</script>

<section aria-labelledby="mt-h">
  <h2 id="mt-h">Meeting names</h2>
  {#if prefs}
    <ul class="list">
      <li class="row">
        <span><label class="name" for="mt-on">Name meetings with AI</label>
          <span class="desc">{prefs.on ? "After the notes are written, the start of the transcript is sent to the Title model for a short name. A name you typed is never changed." : "New meetings stay “Untitled meeting” until you rename them."}</span></span>
        <input id="mt-on" type="checkbox" role="switch" class="switch" checked={prefs.on} onchange={(e) => prefs && save({ ...prefs, on: e.currentTarget.checked })} />
      </li>
      {#if prefs.on}
        <ModelRow id="mt-model" name="Title model" base="fast" value={prefs.model} gone={prefs.gone} {ai} {rev} onpick={(model) => prefs && save({ ...prefs, model })} />
      {/if}
    </ul>
  {/if}
  <p class="err" role="status">{error}</p>
</section>
