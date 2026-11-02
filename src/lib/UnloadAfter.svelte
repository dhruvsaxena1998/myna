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

<div class="srow">
  <span><label class="label" for="unload">Free up memory when idle</label>
    <span class="hint" id="unload-h">The speech model uses 1–2 GB while loaded. Loading it again takes a few seconds.</span></span>
  <select id="unload" class="select" aria-describedby="unload-h" value={secs} onchange={save}>
    {#each CHOICES as [v, label]}<option value={v}>{label}</option>{/each}
  </select>
</div>
<p class="err" role="status">{error}</p>

<style>
  .srow { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3); margin-top: var(--sp-3); padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .label { display: block; font-weight: var(--fw-semibold); }
  .hint { display: block; margin-top: 2px; color: var(--text-2); }
  .select { height: var(--h-ctl); padding: 0 var(--sp-2); border: 0; border-radius: var(--r-md); background: var(--fill-control); color: var(--text); font: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); }
  .select:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
