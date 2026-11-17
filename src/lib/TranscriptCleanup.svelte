<!-- Meeting notes › Transcript cleanup (`part="rows"`) and its Cleanup instructions drill-in (`"editor"`) (docs/settings.html, VIEWS.meetings and VIEWS.tclprompt). One `prefs`, held by the shell, so the rows and the editor agree. -->
<script lang="ts" module>
  /** `model`: "fast" (Uses Fast), "none" (Not set) or "<provider id>|<model>". `shipped`: the text Reset goes back to; `newer`: an edited prompt has a newer default. */
  export type Cleanup = { on: boolean; model: string; gone: boolean; prompt: string; shipped: string; newer: boolean };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import type { Ai } from "./AiProviders.svelte";
  import ModelRow from "./ModelRow.svelte";

  let { prefs = $bindable(), part, ai = null, open = async () => {} }: {
    prefs: Cleanup; part: "rows" | "editor"; ai?: Ai | null; open?: (sub: string) => Promise<void> | void;
  } = $props();
  let error = $state("");
  let rev = $state(0); // re-draws the fields so they show what was kept, not what was typed

  async function save(next: Cleanup) {
    error = "";
    const was = prefs;
    prefs = { ...next, gone: next.model === was.model && was.gone };
    try {
      await invoke("transcript_prefs_set", { prefs: next });
      prefs = await invoke<Cleanup>("transcript_prefs_get"); // `newer` and a blank prompt are the backend's to settle
    } catch (err) {
      prefs = was;
      error = String(err);
    }
    rev++;
  }
  async function reset() {
    await save({ ...prefs, prompt: prefs.shipped });
    await tick();
    document.getElementById("tc-text")?.focus();
  }
</script>

{#if part === "rows"}
  <section aria-labelledby="tcl-h">
    <h2 id="tcl-h">Transcript cleanup</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="tcl-on">Clean up transcripts with AI</label>
          <span class="desc">{prefs.on ? "Fixes punctuation and misheard words, line by line. The transcript has Show original." : "Transcripts stay exactly as Myna heard them."}</span></span>
        <input id="tcl-on" type="checkbox" role="switch" class="switch" checked={prefs.on} onchange={(e) => save({ ...prefs, on: e.currentTarget.checked })} />
      </li>
      {#if prefs.on}
        <ModelRow id="tcl-model" name="Cleanup model" base="fast" value={prefs.model} gone={prefs.gone} {ai} {rev} onpick={(model) => save({ ...prefs, model })} />
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
        <li class="row go" onclick={() => open("tclprompt")}>
          <span><span class="name">Cleanup instructions</span><span class="desc">{prefs.prompt === prefs.shipped ? "The default." : "Edited."}</span></span>
          <span class="end">
            {#if prefs.newer}<span class="pill status">Newer default</span>{/if}
            <button class="chev" aria-label="Cleanup instructions" onclick={(e) => { e.stopPropagation(); open("tclprompt"); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
          </span>
        </li>
      {/if}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{:else}
  {#if prefs.newer}
    <section>
      <ul class="list">
        <li class="row">
          <span><span class="name">A newer default is available</span><span class="desc">Your edit is kept until you choose.</span></span>
          <button class="btn" onclick={reset}>Use the new default</button>
        </li>
      </ul>
    </section>
  {/if}
  <section>
    <ul class="list">
      <li class="field"><label for="tc-text">Instructions</label>
        {#key rev}<textarea class="input" id="tc-text" spellcheck="true" onchange={(e) => save({ ...prefs, prompt: e.currentTarget.value })}>{prefs.prompt}</textarea>{/key}
        <span class="help">Myna always adds “Spell these exactly as written:” with your custom vocabulary. Lines go in as text to clean up, never as instructions; a line that comes back changed too much keeps what Myna heard.</span></li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <div class="actions end"><button class="btn" disabled={prefs.prompt === prefs.shipped} onclick={reset}>Reset to default</button></div>
{/if}

<style>
  /* As in SummarySettings: the chevron is the row's keyboard stop. */
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .actions.end { justify-content: flex-end; }
  textarea.input { display: block; width: 100%; height: 96px; resize: vertical; }
</style>
