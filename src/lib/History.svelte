<!-- Voice typing history (docs/index.html `clipsView()`): what was said with voice typing, newest first, from the stored traces. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { when, type Row } from "./trace";

  // `rows`: every trace, so deleting one also leaves Stats for nerds. `said`: those that left text, the ones listed here.
  let { rows = $bindable(), said }: { rows: Row[]; said: Row[] } = $props();
  let q = $state("");
  let more = $state<Record<number, boolean>>({});
  let copied = $state<string | null>(null);
  let error = $state("");
  let search: HTMLInputElement;

  const shown = $derived.by(() => {
    const words = q.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return said.filter((r) => words.every((w) => `${r.text} ${r.app ?? ""}`.toLowerCase().includes(w)));
  });

  let timer: ReturnType<typeof setTimeout>;
  async function copy(key: string, text: string) {
    error = "";
    try {
      await invoke("text_copy", { text });
      copied = key;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = null), 1500);
    } catch (err) {
      error = String(err);
    }
  }

  async function remove(id: number) {
    error = "";
    try {
      await invoke("trace_delete", { id });
      rows = rows.filter((r) => r.id !== id);
    } catch (err) {
      error = String(err);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if (e.key === "/" && !typing && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      search.focus();
      search.select();
    }
  }
</script>

<svelte:window {onkeydown} />

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}
{#snippet copyIcon(key: string)}{@render icon(copied === key ? '<path d="M5 12.5l4.5 4.5L19 7.5"/>' : '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>')}{/snippet}

<header data-tauri-drag-region>
  <h1>Voice typing history</h1>
  <span class="grow"></span>
  <label class="search">
    {@render icon('<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>')}
    <input bind:this={search} bind:value={q} type="search" placeholder="Search what you said" aria-label="Search what you said" autocomplete="off" spellcheck="false"
      onkeydown={(e) => { if (e.key === "Escape" && q) { e.preventDefault(); q = ""; } }} />
    <kbd aria-hidden="true">/</kbd>
  </label>
</header>
<div class="content">
  <p class="note">Everything you’ve said with voice typing, kept on this Mac. Newest first.</p>
  <p class="err" role="status">{error}</p>
  {#if shown.length}
    <ul aria-label="Voice typing history">
      {#each shown as r (r.id)}
        {@const text = r.text ?? ""}
        <li>
          <div class="body">
            <p class:clamp={!more[r.id]}>{text}</p>
            <span class="meta">{[r.app, when(r.started)].filter(Boolean).join(" · ")}{#if text.length > 140} · <button class="link" aria-expanded={!!more[r.id]} onclick={() => (more[r.id] = !more[r.id])}>{more[r.id] ? "Show less" : "Show more"}</button>{/if}</span>
          </div>
          <div class="acts">
            {#if r.heard && r.heard !== text}
              <button class="icon" aria-label="Copy what I said" title="Copy what I said" onclick={() => copy(`raw-${r.id}`, r.heard!)}>{#if copied === `raw-${r.id}`}{@render copyIcon(`raw-${r.id}`)}{:else}{@render icon('<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>')}{/if}</button>
            {/if}
            <button class="icon" aria-label="Copy again" title="Copy again" onclick={() => copy(`text-${r.id}`, text)}>{@render copyIcon(`text-${r.id}`)}</button>
            <button class="icon" aria-label="Delete" title="Delete" onclick={() => remove(r.id)}>{@render icon('<path d="M7 7l10 10M17 7L7 17"/>')}</button>
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <div class="empty">
      <h2>{q.trim() ? "No matches" : "Nothing here yet"}</h2>
      <p>{q.trim() ? "Try another word, or check the spelling." : "Hold your voice typing key in any app and talk. What you say shows up here too."}</p>
      {#if q.trim()}<button class="btn" onclick={() => { q = ""; search.focus(); }}>Clear search</button>{/if}
    </div>
  {/if}
</div>

<style>
  .grow { flex: 1; }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .search { position: relative; display: flex; align-items: center; flex: 0 1 220px; min-width: 120px; }
  .search > .ic { position: absolute; left: 10px; color: var(--text-3); pointer-events: none; }
  .search input { width: 100%; box-sizing: border-box; height: var(--h-ctl); padding: 0 30px; border: 0; border-radius: var(--r-full); background: var(--fill-field);
    box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; outline: none; -webkit-appearance: none; appearance: none; }
  .search input::-webkit-search-cancel-button { display: none; }
  .search input:focus-visible { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .search kbd { position: absolute; right: 8px; padding: 1px 6px; border-radius: 5px; font: 500 11px/1.4 var(--font); color: var(--text-3); box-shadow: inset 0 0 0 .5px var(--line-strong); pointer-events: none; }
  .search input:focus + kbd, .search input:not(:placeholder-shown) + kbd { display: none; }
  .note { margin: 0; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .err { margin: 0 0 var(--sp-2); min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; gap: var(--sp-3); padding: var(--sp-3) var(--sp-2); border-bottom: .5px solid var(--line); }
  li:last-child { border-bottom: 0; }
  .body { flex: 1; min-width: 0; display: grid; gap: 2px; }
  p { margin: 0; font-size: 14px; line-height: 1.6; user-select: text; overflow-wrap: anywhere; }
  .clamp { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .meta { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .link { border: 0; background: none; padding: 0; font: inherit; color: var(--accent-text); border-radius: var(--r-sm); }
  /* Space is kept while hidden, so a row never reflows on hover. */
  .acts { display: flex; gap: 2px; flex: none; opacity: 0; transition: opacity var(--dur) var(--ease-out); }
  li:hover .acts, li:focus-within .acts { opacity: 1; }
  .icon { width: var(--h-ctl); height: var(--h-ctl); display: grid; place-items: center; border: 0; border-radius: var(--r-md); background: none; color: var(--text-2); }
  .icon:hover { background: var(--fill-hover); }
  .icon:focus-visible, .link:focus-visible, .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .empty { padding: var(--sp-6) var(--sp-4); text-align: center; }
  .empty h2 { margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .empty p { margin: 0 0 var(--sp-3); color: var(--text-2); font-size: var(--fs-body); }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
</style>
