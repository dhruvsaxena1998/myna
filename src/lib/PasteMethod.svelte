<!-- How words are added: pasted through the clipboard, or typed out (docs/index.html, "Adding your words"). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Voice } from "./VoiceShortcut.svelte";

  let { voice = $bindable() }: { voice: Voice } = $props();
  let error = $state("");

  async function save(paste: Voice["paste"]) {
    const was = voice;
    voice = { ...voice, paste };
    error = "";
    try {
      await invoke("voice_set", { voice });
    } catch (err) {
      voice = was;
      error = String(err);
    }
  }
</script>

<section class="group" aria-labelledby="paste-h">
  <h2 id="paste-h">Adding your words</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name" id="paste-l">How words are added</span><span class="desc">{voice.paste === "clipboard" ? "Fastest. Myna puts back whatever you had copied afterwards." : "Types each letter. Slower, but works in apps that block pasting."}</span></span>
      <fieldset class="seg" aria-labelledby="paste-l">
        {#each [["clipboard", "Paste"], ["type", "Type"]] as [v, label]}
          <label><input type="radio" name="paste" value={v} checked={voice.paste === v} onchange={() => save(v as Voice["paste"])} /><span>{label}</span></label>
        {/each}
      </fieldset>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .group { margin-bottom: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-3); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  .seg { display: inline-flex; gap: 2px; margin: 0; padding: 3px; border: 0; border-radius: var(--r-full); background: var(--fill-field); }
  .seg label { position: relative; }
  .seg input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .seg span { display: flex; align-items: center; height: 26px; padding: 0 14px; border-radius: var(--r-full); color: var(--text-2); white-space: nowrap; }
  .seg input:checked + span { background: var(--fill-control-on); color: var(--text); font-weight: var(--fw-medium); box-shadow: 0 0 0 .5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, .1); }
  .seg input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
