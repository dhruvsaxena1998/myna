<!-- The page head of Listen, Understand and Write: the path your voice takes, each stage with what it does now (docs/settings.html, strip()). -->
<script lang="ts">
  import { onMount } from "svelte";
  import HostPill from "./HostPill.svelte";
  import { keycaps, spoken } from "./shortcut";

  type Stage = "listen" | "understand" | "write";
  let { page, on, mode, accel, model, installed, pct, paste, fast, go }: {
    page: Stage;
    on: boolean;
    mode: "hold" | "tap";
    accel: string | null;
    model: string | null; // the chosen voice typing model's name
    installed: boolean;
    pct: number | undefined; // download progress, while there is one
    paste: "clipboard" | "type";
    fast: { model: string; off: boolean } | null; // a Fast model that is still offered
    go: (page: Stage) => void;
  } = $props();

  const STAGES: [Stage, string][] = [["listen", "Listen"], ["understand", "Understand"], ["write", "Write"]];
  // What each value says, so a change fades in and a repeat doesn't. The first paint never animates.
  const sig = $derived<Record<Stage, string>>({
    listen: on ? `${mode}|${accel ?? ""}` : "off",
    understand: on ? (installed ? `ok|${model}` : pct != null ? `dl|${pct}` : "no") : "off",
    write: on ? `${paste}|${fast ? `${fast.model}|${fast.off}` : ""}` : "off",
  });
  let settled = $state(false);
  onMount(() => requestAnimationFrame(() => (settled = true)));
</script>

<nav class="stage-path" aria-label="Where your voice goes">
  {#each STAGES as [id, name], i (id)}
    {#if i}<span class="link" class:off={!on} aria-hidden="true"></span>{/if}
    <button class="stage" class:off={!on && i > 0} aria-current={page === id ? "step" : undefined} onclick={() => go(id)}>
      <span class="sn">{name}</span>
      {#key sig[id]}
        <span class="sv" class:fresh={settled}>
          {#if !on}Off
          {:else if id === "listen"}{mode === "hold" ? "Hold" : "Tap"}
            {#if accel}<span class="keys" role="img" aria-label={spoken(accel)}>{#each keycaps(accel) as k}<kbd>{k}</kbd>{/each}</span>{:else}no key set{/if}
          {:else if id === "understand"}
            {#if installed}<span class="ok"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{model}</span>
            {:else if pct != null}Downloading {pct}%
            {:else}Model not downloaded{/if}
          {:else}{paste === "clipboard" ? "Paste" : "Type"}{#if fast}<span aria-hidden="true">·</span><span class="clip">{fast.model}</span>{#if fast.off}<HostPill off />{/if}{/if}{/if}
        </span>
      {/key}
    </button>
  {/each}
</nav>

<style>
  .stage-path { display: flex; align-items: flex-start; min-width: 0; overflow: hidden; }
  .stage { display: flex; flex-direction: column; align-items: flex-start; flex: none; padding: 2px var(--sp-2); margin: 0 calc(-1 * var(--sp-2)); border: 0; border-radius: var(--r-sm); background: none; text-align: left; font: inherit; color: inherit;
    transition: background-color var(--dur-fast) var(--ease-in); }
  .stage:hover { background: var(--fill-hover); }
  .stage:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  .sn { font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-medium); color: var(--text-2); white-space: nowrap; }
  .stage[aria-current="step"] .sn { color: var(--accent-text); }
  .stage.off .sn { color: var(--text-3); }
  .sv { display: flex; align-items: center; gap: var(--sp-1); height: var(--lh-footnote); font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3);
    white-space: nowrap; overflow: hidden; font-variant-numeric: tabular-nums; }
  .sv :global(kbd) { height: 16px; min-width: 16px; padding: 0 4px; }
  .sv :global(.ok) { color: inherit; }
  .sv :global(.ok .ic) { width: 12px; height: 12px; }
  .sv :global(.pill) { height: 16px; padding: 0 6px; }
  .link { flex: none; width: 16px; height: .5px; margin: 11px var(--sp-2) 0; background: var(--line-strong); }
  .link.off { background: none; border-top: .5px dashed var(--line-strong); }
  /* The last stage gives way, so a long model ID ends in an ellipsis. */
  .stage:last-child { flex: 0 1 auto; min-width: 0; max-width: 100%; }
  /* Room for the usual values (Hold or Tap with a key; a model name or a download), so a changed value doesn't push the stages after it. */
  .stage:first-of-type { min-width: 112px; }
  .stage:nth-of-type(2) { min-width: 168px; max-width: 220px; }
  .stage:nth-of-type(2) .sv { max-width: 100%; }
  .stage:last-child .sv { max-width: 100%; }
  .clip { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .sv.fresh { animation: fresh var(--dur) var(--ease-out); }
  @keyframes fresh { from { color: var(--accent-text); } }
  /* Narrow: three equal shares, so every name shows and a changed value still moves nothing; values end in an ellipsis. */
  @container settings (width < 460px) {
    .link { width: 8px; margin-inline: var(--sp-1h); }
    .stage, .stage:first-of-type, .stage:nth-of-type(2), .stage:last-child { flex: 1 1 0; min-width: 0; max-width: none; }
    .sv { max-width: 100%; }
  }
</style>
