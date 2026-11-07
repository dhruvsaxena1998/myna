<!-- Voice typing › Your feedback: export the ratings and corrections (docs/index.html). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let done = $state(false);
  let error = $state("");

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

<section class="group" aria-labelledby="fb-h">
  <h2 id="fb-h">Feedback</h2>
  <ul class="list">
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
