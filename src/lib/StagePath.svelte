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
    {#if i}<span class="link" class:off={!on} aria-hidden="true"><svg viewBox="0 0 24 24"><path d="M9 6l6 6-6 6" /></svg></span>{/if}
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
  /* Three columns, the last a little wider for the cleanup model: the gaps between stages never change, and a changed value moves nothing; values end in an ellipsis. */
  .stage-path { flex: 1; display: grid; grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto minmax(0, 1.4fr); align-items: start; min-width: 0; max-width: 640px; }
  .stage { display: flex; flex-direction: column; align-items: flex-start; min-width: 0; padding: 2px var(--sp-2); margin: 0 calc(-1 * var(--sp-2)); border: 0; border-radius: var(--r-sm); background: none; text-align: left; font: inherit; color: inherit;
    transition: background-color var(--dur-fast) var(--ease-in); }
  .stage:hover { background: var(--fill-hover); }
  .stage:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  /* Names at title3, the current one in --text: it reads as the page's title, the others as the way there. */
  .sn { max-width: 100%; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .stage[aria-current="step"] .sn { color: var(--text); }
  .stage.off .sn { color: var(--text-disabled); }
  .sv { display: flex; align-items: center; gap: var(--sp-1); max-width: 100%; height: var(--lh-footnote); font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3);
    white-space: nowrap; overflow: hidden; font-variant-numeric: tabular-nums; }
  .sv :global(kbd) { height: 16px; min-width: 16px; padding: 0 4px; }
  .sv :global(.ok) { color: inherit; }
  .sv :global(.ok .ic) { width: 12px; height: 12px; }
  .sv :global(.pill) { height: 16px; padding: 0 6px; }
  /* A chevron centred on the names' line (2px padding + (20 − 14) / 2). */
  .link { width: 14px; height: 14px; margin: 5px var(--sp-3) 0; }
  .link svg { display: block; width: 14px; height: 14px; fill: none; stroke: var(--text-3); stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .link.off svg { stroke: var(--text-disabled); }
  .clip { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .sv.fresh { animation: fresh var(--dur) var(--ease-out); }
  @keyframes fresh { from { color: var(--accent-text); } }
  /* Narrow: names drop to body size so all three fit whole. */
  @container settings (width < 460px) {
    .link { margin: 3px var(--sp-1h) 0; }
    .sn { font-size: var(--fs-body); line-height: var(--lh-body); }
  }
</style>
