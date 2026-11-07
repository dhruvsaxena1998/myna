<!-- How long the speech-to-text model stays in memory after its last use. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { initial }: { initial: number } = $props();
  let secs = $derived(initial); // assignable: a failed save reverts it
  let error = $state("");

  const CHOICES: [number, string][] = [
    [0, "Right after each use"], [60, "After 1 minute"], [300, "After 5 minutes"],
    [900, "After 15 minutes"], [3600, "After 1 hour"], [-1, "Never"],
  ];

  async function save(e: Event) {
    const was = secs;
    secs = Number((e.target as HTMLSelectElement).value);
    error = "";
    try {
      await invoke("stt_unload_set", { secs });
    } catch (err) {
      secs = was;
      error = String(err);
    }
  }
</script>

<section aria-labelledby="unload-sec">
  <h2 id="unload-sec">Memory</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="unload">Free up memory when idle</label>
        <span class="desc" id="unload-h">The speech model uses 1–2 GB while loaded. Loading it again takes a few seconds.</span></span>
      <select id="unload" class="select" aria-describedby="unload-h" value={secs} onchange={save}>
        {#each CHOICES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

