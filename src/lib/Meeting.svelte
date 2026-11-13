<!-- One meeting (docs/index.html `detail()`): its transcript is the document until summaries arrive, then it moves to the side panel. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { clock, dayHead, groups, meta, writing, type Page, type Progress } from "./meetings";

  let { page, progress, onback, onretry, oninclude }: { page: Page; progress?: Progress; onback: () => void; onretry: (id: number) => void; oninclude: (ids: number[]) => void } = $props();
  let copied = $state(false);
  let error = $state("");
  let timer: ReturnType<typeof setTimeout>;

  async function copy() {
    error = "";
    try {
      // The backend decides what leaves the meeting: lines said while muted stay out.
      await invoke("text_copy", { text: await invoke("meeting_text", { id: page.id }) });
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1500);
    } catch (err) {
      error = String(err);
    }
  }
  const date = $derived(dayHead(page.started));
</script>

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}

<header data-tauri-drag-region>
  <button class="btn plain" aria-label="Back to meetings" onclick={onback}>{@render icon('<path d="M15 5l-7 7 7 7"/>')}Meetings</button>
  <span class="grow"></span>
  {#if page.segments.length}
    <button class="btn" onclick={copy}>{@render icon(copied ? '<path d="M5 12.5l4.5 4.5L19 7.5"/>' : '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>')}{copied ? "Copied" : "Copy"}</button>
  {/if}
</header>
<div class="content">
  <article aria-labelledby="mt">
    <h1 id="mt">{page.title}</h1>
    <p class="meta">{date[1] ? `${date[1]} · ` : `${date[0]} · `}{meta(page)}</p>
    {#if page.note}<p class="note">{page.note}</p>{/if}
    <p class="err" role="status">{error}</p>
    {#if page.segments.length}
      <div class="transcript">
        {#each groups(page.segments) as g}
          {#if "left" in g}
            <div class="seg"><div class="who"><b>Me</b><time>{clock(g.left[0].start_ms)}</time></div>
              <p class="left"><span>Said while muted · left out</span><button class="btn small" onclick={() => oninclude(g.left.map((l) => l.id))}>Include</button></p></div>
          {:else}
            <div class="seg"><div class="who"><b>{g.line.speaker}</b><time>{clock(g.line.start_ms)}</time></div>
              <p>{g.line.text}{#if g.line.included}<span class="mark">Said while muted</span>{/if}</p></div>
          {/if}
        {/each}
      </div>
    {:else if page.state === "failed"}
      <p class="say fail">{page.failure ?? "Myna couldn't write up this meeting."}</p>
      <button class="btn" onclick={() => onretry(page.id)}>Try again</button>
    {:else if writing(page)}
      <p class="say" role="status">Writing up your notes… {Math.round((progress?.progress ?? 0) * 100)}%</p>
    {:else}
      <p class="say">Nobody said anything Myna could hear in this meeting.</p>
    {/if}
  </article>
</div>

<style>
  .grow { flex: 1; }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: var(--sp-1h); height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn.plain { background: none; box-shadow: none; padding: 0 var(--sp-2); }
  .btn.plain:hover { background: var(--fill-hover); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  article { max-width: var(--w-doc, 720px); margin: 0 auto; user-select: text; }
  h1 { margin: 0; font-size: var(--fs-title1); line-height: 1.2; font-weight: var(--fw-bold, 700); }
  .meta { margin: var(--sp-1) 0 0; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .note { margin: var(--sp-1) 0 0; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .err { margin: var(--sp-1) 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .transcript { margin-top: var(--sp-3); }
  .seg { display: grid; grid-template-columns: 112px minmax(0, 1fr); gap: var(--sp-4); padding: var(--sp-2) 0; }
  .who { text-align: right; padding-top: 2px; }
  .who b { display: block; font-weight: var(--fw-semibold); }
  .who time { display: block; color: var(--text-3); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .seg p { margin: 0; font-size: var(--fs-reading); line-height: var(--lh-reading); overflow-wrap: anywhere; }
  .left { display: flex; align-items: center; gap: var(--sp-2); color: var(--text-3); font-size: var(--fs-footnote); }
  .btn.small { height: 24px; padding: 0 10px; font-size: var(--fs-footnote); }
  .mark { display: block; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .say { margin: var(--sp-4) 0 var(--sp-3); color: var(--text-2); }
  .say.fail { color: var(--danger-text); }
</style>
