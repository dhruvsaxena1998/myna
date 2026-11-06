<!-- Clear all, in the headers of Voice typing history and Stats for nerds (docs/index.html `clip-clear`). Both read the same stored dictations. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";

  // `count`: how many dictations there are, so the button waits until there is something to clear. `onclear`: tells the page the list is empty now.
  let { count, onclear }: { count: number; onclear: () => void } = $props();
  let asking = $state(false);
  let error = $state("");
  let button: HTMLButtonElement;
  let no: HTMLButtonElement | undefined = $state();

  async function ask(on: boolean) {
    asking = on;
    error = "";
    await tick();
    (on ? no : button)?.focus();
  }
  async function yes() {
    try {
      await invoke("traces_clear");
      onclear();
      await ask(false);
    } catch (err) {
      error = String(err);
    }
  }
</script>

<svelte:window onkeydown={(e) => asking && e.key === "Escape" && ask(false)} />

<span class="wrap">
  <button class="btn" bind:this={button} disabled={!count} aria-haspopup="dialog" aria-expanded={asking} onclick={() => ask(true)}>Clear all</button>
  {#if asking}
    <div class="confirm" role="alertdialog" aria-labelledby="clr-t">
      <span id="clr-t">Delete all dictations, with their stats? This can’t be undone.</span>
      {#if error}<span class="err" role="status">{error}</span>{/if}
      <span class="acts"><button class="btn" bind:this={no} onclick={() => ask(false)}>Cancel</button><button class="btn danger" onclick={yes}>Delete all</button></span>
    </div>
  {/if}
</span>

<style>
  .wrap { position: relative; flex: none; }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; white-space: nowrap; }
  .btn:hover:not(:disabled) { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:disabled { opacity: .4; }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .btn.danger { background: var(--danger); color: var(--on-solid); box-shadow: var(--shadow-solid); }
  .confirm { position: absolute; z-index: 90; top: calc(100% + 6px); right: 0; width: 280px; display: grid; gap: var(--sp-2); padding: var(--sp-3) var(--sp-4); border-radius: var(--r-lg); background: var(--bg-raised); box-shadow: var(--shadow-menu); font-size: var(--fs-footnote); line-height: var(--lh-footnote); white-space: normal; }
  .acts { display: flex; justify-content: flex-end; gap: var(--sp-2); }
  .err { color: var(--danger-text); }
</style>
