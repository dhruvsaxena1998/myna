<!-- The main window (docs/index.html `app()`): only what's built so far. Meetings, Chat, Folders and Tags join the sidebar with their stories. -->
<script lang="ts">
  let { data } = $props();
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import ErrorState from "$lib/ErrorState.svelte";
  import History from "$lib/History.svelte";
  import Traces from "$lib/Traces.svelte";
  import Yours from "$lib/Yours.svelte";
  import { glassOn, syncGlass, type Look } from "$lib/look";
  import type { Row, Trace } from "$lib/trace";
  import type { Rules } from "$lib/Words.svelte";

  // svelte-ignore state_referenced_locally
  let look = $state<Look | null>(data.appearance.ok ? data.appearance.look : null);
  // svelte-ignore state_referenced_locally
  let rows = $state<Row[]>(data.traces?.ok ? data.traces.rows : []);
  // svelte-ignore state_referenced_locally
  let all = $state<Trace[]>(data.all?.ok ? data.all.traces : []);
  // svelte-ignore state_referenced_locally
  let words = $state(data.words);
  let current = $state<"history" | "yours" | "nerds">("history");
  // Stats for nerds is experimental: its entry shows only while it and Experimental features are on.
  const nerds = $derived(!!look?.experimental && look.nerds);
  const view = $derived(current === "nerds" && !nerds ? "history" : current);
  // History lists what was said; a dictation that left no text has only its trace in Stats for nerds.
  const said = $derived(rows.filter((r) => r.text));

  // Settings changes the look while this window is open: follow it.
  $effect(() => {
    const un = listen<Look>("look", ({ payload }) => (look = payload));
    return () => void un.then((f) => f());
  });
  $effect(() => {
    void [look && glassOn(look), look?.glass, look?.accent]; // the backend picks the tint
    syncGlass(look);
  });

  // The window stays loaded while hidden: coming back to it picks up dictations made since.
  $effect(() => {
    const off = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (!payload) return;
      if (data.traces?.ok) invoke<Row[]>("traces_list").then((r) => (rows = r), () => {});
      if (data.all?.ok) invoke<Trace[]>("traces_all").then((r) => (all = r), () => {});
      invoke<Rules>("rules_get").then((r) => (words = r.settings.words), () => {});
    });
    return () => void off.then((f) => f());
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey && e.key === ",") {
      e.preventDefault();
      invoke("settings_open");
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}

<div class="window">
  <nav class="sidebar" aria-label="Sidebar">
    <div class="titlebar" data-tauri-drag-region></div>
    <button class="nav" aria-current={view === "history"} onclick={() => (current = "history")}>
      {@render icon('<rect x="5" y="5" width="14" height="16" rx="2"/><path d="M9 5V3.5h6V5M9 11h6M9 15h4"/>')}<span class="label">Voice typing history</span><span class="count">{said.length}</span>
    </button>
    <button class="nav" aria-current={view === "yours"} onclick={() => (current = "yours")}>
      {@render icon('<path d="M3 17l5-5 4 4 8-9"/><path d="M15 7h5v5"/>')}<span class="label">Your dictation</span>
    </button>
    {#if nerds}
      <button class="nav" aria-current={view === "nerds"} onclick={() => (current = "nerds")}>
        {@render icon('<path d="M5 20V11M12 20V5M19 20v-6"/>')}<span class="label">Stats for nerds</span>
      </button>
    {/if}
    <div class="foot">
      <button class="nav" onclick={() => invoke("settings_open")}>
        {@render icon('<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>')}<span class="label">Settings</span><kbd>⌘,</kbd>
      </button>
    </div>
  </nav>

  <main>
    {#if !data.storage.ok}
      <header data-tauri-drag-region><h1>Myna</h1></header>
      <div class="content"><ErrorState title="Myna couldn't open its data folder" message={data.storage.error} /></div>
    {:else if !data.traces?.ok || ((view === "nerds" || view === "yours") && !data.all?.ok)}
      {@const failed = !data.traces?.ok ? data.traces : data.all}
      <header data-tauri-drag-region><h1>{view === "nerds" ? "Stats for nerds" : view === "yours" ? "Your dictation" : "Voice typing history"}</h1></header>
      <div class="content"><ErrorState title="Myna couldn't read your dictations" message={failed?.ok === false ? failed.error : ""} /></div>
    {:else if view === "yours"}
      <Yours traces={all} />
    {:else if view === "nerds"}
      <Traces bind:traces={all} {words} onclear={() => (rows = [])} />
    {:else}
      <History bind:rows {said} onclear={() => (all = [])} />
    {/if}
  </main>
</div>

<style>
  /* Same frame as the Settings window: native vibrancy behind a tinted sidebar panel and a content panel. */
  .window { display: grid; grid-template-columns: var(--w-sidebar) minmax(0, 1fr); gap: var(--win-inset);
    height: 100vh; box-sizing: border-box; padding: var(--win-inset); }
  .sidebar { display: flex; flex-direction: column; gap: 2px; padding: 0 var(--sp-2) var(--sp-2); border-radius: var(--r-xl); background: var(--mat-sidebar); box-shadow: inset 0 0 0 .5px var(--edge); }
  :global(html[data-liquid]) main { box-shadow: inset 0 0 0 .5px var(--edge); }
  .titlebar { height: var(--h-titlebar); flex: none; }
  .nav { display: flex; align-items: center; gap: var(--sp-2); width: 100%; height: var(--h-row); padding: 0 var(--sp-2); border: 0; border-radius: var(--r-md);
    background: none; color: inherit; font: inherit; text-align: left; }
  .nav:hover { background: var(--fill-hover); }
  .nav:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
  /* As the Settings sidebar: stronger than --accent-soft-strong on see-through material, solid on Liquid Glass. */
  .nav[aria-current="true"] { background: color-mix(in srgb, var(--accent) 28%, transparent); }
  .nav[aria-current="true"] .ic { color: var(--accent-text); }
  :global(html[data-liquid]) .nav[aria-current="true"] { background: var(--accent); color: var(--on-accent); }
  :global(html[data-liquid]) .nav[aria-current="true"] .ic, :global(html[data-liquid]) .nav[aria-current="true"] .count { color: var(--on-accent); }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; color: var(--text-2); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count, kbd { color: var(--text-3); font: inherit; font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .foot { margin-top: auto; }
  main { display: grid; grid-template-rows: auto minmax(0, 1fr); min-height: 0; border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: hidden; }
  main > :global(header) { height: var(--h-toolbar); display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-5); }
  main > :global(header h1) { pointer-events: none; margin: 0; white-space: nowrap; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  main > :global(.content) { position: relative; padding: 0 var(--sp-5) var(--sp-5); overflow: auto; scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
</style>
