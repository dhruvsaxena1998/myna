<!-- How long the speech-to-text model stays in memory after its last use: one row of Understand's Speech-to-text list. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { initial, error = $bindable("") }: { initial: number; error?: string } = $props();
  let secs = $derived(initial); // assignable: a failed save reverts it

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

<li class="row">
  <span><label class="name" for="unload">Unload model when idle</label>
    <span class="desc" id="unload-h">Loading it again takes a few seconds.</span></span>
  <select id="unload" class="select" aria-describedby="unload-h" value={secs} onchange={save}>
    {#each CHOICES as [v, label]}<option value={v}>{label}</option>{/each}
  </select>
</li>
