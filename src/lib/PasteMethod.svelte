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

<section aria-labelledby="paste-h">
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
</style>
