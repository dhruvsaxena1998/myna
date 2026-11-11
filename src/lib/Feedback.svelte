<!-- Voice typing › Feedback: Ask how it went on the pill, and export the ratings and corrections (docs/index.html). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Voice } from "./VoiceShortcut.svelte";

  let { voice = $bindable() }: { voice: Voice } = $props();

  let done = $state(false);
  let error = $state("");

  async function ask(rate: boolean) {
    const was = voice;
    voice = { ...voice, rate };
    error = "";
    try {
      await invoke("voice_set", { voice });
    } catch (err) {
      voice = was;
      error = String(err);
    }
  }

  async function save() {
    done = false;
    error = "";
    try {
      await invoke("feedback_export");
      done = true;
    } catch (err) {
      error = String(err);
    }
  }
</script>

<section aria-labelledby="fb-h">
  <h2 id="fb-h">Feedback</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="fb-ask">Ask how it went</label><span class="desc">After each dictation the pill shows 👍 and 👎 for a few seconds. Clicking them doesn’t take you out of the app you’re typing in.</span></span>
      <input id="fb-ask" type="checkbox" role="switch" class="switch" checked={voice.rate ?? false} onchange={(e) => ask(e.currentTarget.checked)} />
    </li>
    <li class="row">
      <span><span class="name">Your feedback</span><span class="desc">Ratings and corrections stay on this Mac. Export saves them to a file you can look over and send us.</span></span>
      <button type="button" class="btn" onclick={save}>Export…</button>
    </li>
  </ul>
  <p class="err" class:ok={done && !error} role="status">{error || (done ? "Saved to Downloads" : "")}</p>
</section>

<style>
  .err.ok { color: var(--text-3); }
</style>
