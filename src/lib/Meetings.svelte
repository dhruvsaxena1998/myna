<!-- Meetings (docs/index.html `meetingsList()`): recorded meetings by day, with the live card while one records or is written up. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ClearAll from "./ClearAll.svelte";
  import ErrorState from "./ErrorState.svelte";
  import { clock, dayHead, meta, writing, type Meeting, type Progress, type Running } from "./meetings";

  // `onchange`: a meeting was renamed or deleted, so the list reloads.
  let { meetings, running, progress, on, error, onstart, onstop, onenable, onopen, onretry, onlocal, onchange }: {
    meetings: Meeting[]; running: Running; progress: Record<number, Progress>; on: boolean; error: string;
    onstart: () => void; onstop: () => void; onenable: () => void; onopen: (id: number) => void; onretry: (id: number) => void; onlocal: (id: number) => void; onchange: () => void;
  } = $props();

  let editing = $state<number | null>(null); // the row whose name is being typed
  let name = $state("");
  let deleting = $state<number | null>(null); // the row asking "Delete this meeting?"
  let failed = $state("");
  async function act(cmd: string, args: Record<string, unknown>) {
    failed = "";
    try {
      await invoke(cmd, args);
      onchange();
    } catch (err) {
      failed = String(err);
    }
  }
  function rename(m: Meeting) {
    if (editing !== m.id) return; // Enter saves and the blur that follows must not save again
    editing = null;
    if (name.trim() && name.trim() !== m.title) act("meeting_rename", { id: m.id, title: name });
  }
  const focus = (el: HTMLInputElement) => (el.focus(), el.select());
  const focusOn = (el: HTMLElement) => el.focus();

  // One tick a second drives the recording's timer.
  let now = $state(Date.now());
  $effect(() => {
    if (!running) return;
    now = Date.now();
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  // A meeting being written up shows as its card, not a row, so finishing swaps one block for one of the same height.
  const busy = $derived(meetings.filter(writing));
  const listed = $derived(meetings.filter((m) => !writing(m)));
  const groups = $derived.by(() => {
    const out: { head: string; sub: string; items: Meeting[] }[] = [];
    for (const m of listed) {
      const [head, sub] = dayHead(m.started);
      const g = out.at(-1);
      g && g.head === head && g.sub === sub ? g.items.push(m) : out.push({ head, sub, items: [m] });
    }
    return out;
  });
  const percent = (id: number) => Math.round((progress[id]?.progress ?? 0) * 100);
</script>

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}

<header data-tauri-drag-region>
  <h1>All meetings</h1>
  <span class="grow"></span>
  {#if on && listed.length}
    <ClearAll count={listed.length} onclear={onchange} ask="Delete every meeting, with its transcript, summaries and audio? This can’t be undone." clear={() => invoke("meeting_delete", { id: null })} />
  {/if}
  {#if on && !running}
    <button class="btn primary" aria-label="Start notes" disabled={busy.length > 0} title={busy.length ? "Wait until the last meeting’s notes are written up" : undefined} onclick={onstart}>
      {@render icon('<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>')}Start notes
    </button>
  {/if}
</header>
<div class="content">
  {#if error}<ErrorState title="Myna couldn't open that meeting" message={error} />{/if}
  {#if failed}<p class="err" role="status">{failed}</p>{/if}
  {#if !on}
    <div class="empty">
      <h2>Meeting notes are off</h2>
      <p>Turn them on to record your calls and get notes, a summary and answers to your questions. Myna will ask for permission to hear your calls first.</p>
      <button class="btn primary" onclick={onenable}>Turn on meeting notes</button>
    </div>
  {:else}
    {#if running}
      {@const paused = running.mic === "paused"}
      <section class="card rec" class:paused aria-label={paused ? "Meeting recording paused" : "Meeting recording in progress"}>
        <div class="main"><span class="label"><span class="dot" aria-hidden="true"></span>{paused ? "Paused" : "Recording"}{running.source ? ` · ${running.source}` : ""}</span><span class="time">{clock(now - running.started)}</span></div>
        <span class="lvl" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span>
        <span class="mic">{@render icon('<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21"/>')}{running.words}</span>
        <span class="grow"></span>
        <button class="btn" onclick={() => act("meeting_pause", { paused: !paused })}>
          {@render icon(paused ? '<path d="M8 5.5v13l10.5-6.5z"/>' : '<path d="M9 6v12M15 6v12"/>')}{paused ? "Resume" : "Pause"}
        </button>
        <button class="btn danger" onclick={onstop}>{@render icon('<rect x="7" y="7" width="10" height="10" rx="2"/>')}Stop</button>
      </section>
    {/if}
    {#each busy as m (m.id)}
      <section class="card" role="status">
        <span class="dot idle" aria-hidden="true"></span><span class="strong">Writing up your notes…</span>
        <span class="sub">{percent(m.id)}%</span>
      </section>
    {/each}
    {#if groups.length}
      {#each groups as g, i}
        <section aria-labelledby="dg-{i}">
          <h2 id="dg-{i}"><span>{g.head}</span>{#if g.sub}<span class="dsub">{g.sub}</span>{/if}</h2>
          <ul>
            {#each g.items as m (m.id)}
              <li>
                <div class="body">
                  {#if editing === m.id}
                    <input class="name" aria-label="Meeting name" maxlength="200" bind:value={name} use:focus onblur={() => rename(m)}
                      onkeydown={(e) => { if (e.key === "Enter") rename(m); else if (e.key === "Escape") editing = null; }} />
                  {:else}
                    <button class="title" onclick={() => onopen(m.id)}>{m.title}</button>
                  {/if}
                  {#if m.state === "failed"}
                    <span class="meta fail">{m.failure ?? "Myna couldn't write up this meeting."}</span>
                  {:else}
                    <span class="meta">{meta(m)}</span>
                  {/if}
                </div>
                {#if m.incomplete}<button class="btn" onclick={() => onlocal(m.id)}>Transcribe locally</button>{/if}
                {#if m.state === "failed"}<button class="btn" onclick={() => onretry(m.id)}>Try again</button>{/if}
                {#if deleting === m.id}
                  <span class="ask">Delete this meeting and its audio?</span>
                  <!-- Takes focus from the Delete icon it replaced; Esc cancels, like Clear all. -->
                  <button class="btn" use:focusOn onclick={() => (deleting = null)} onkeydown={(e) => e.key === "Escape" && (deleting = null)}>Cancel</button>
                  <button class="btn danger" onclick={() => ((deleting = null), act("meeting_delete", { id: m.id }))} onkeydown={(e) => e.key === "Escape" && (deleting = null)}>Delete</button>
                {:else if editing !== m.id}
                  <span class="acts">
                    <button class="icon" aria-label="Rename" title="Rename" onclick={() => ((name = m.title), (editing = m.id))}>{@render icon('<path d="M4 20h4L19 9l-4-4L4 16v4zM13.5 6.5l4 4"/>')}</button>
                    <button class="icon" aria-label="Delete" title="Delete" onclick={() => (deleting = m.id)}>{@render icon('<path d="M5 7h14M10 7V5h4v2M7 7l1 12h8l1-12"/>')}</button>
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {:else if !running && !busy.length}
      <div class="empty">
        <h2>No meetings yet</h2>
        <p>Press Start notes, or use your shortcut from any app, and Myna writes up the call when it ends. The audio stays on this Mac.</p>
      </div>
    {/if}
  {/if}
</div>

<style>
  .grow { flex: 1; }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: var(--sp-1h); height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); flex: none;
    background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; }
  .btn:hover:not(:disabled) { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:disabled { color: var(--text-disabled); }
  .btn:focus-visible, .title:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .btn.primary { background: var(--accent); color: var(--on-accent); box-shadow: var(--shadow-solid); }
  .btn.primary:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 88%, black); }
  .btn.primary:disabled { background: var(--fill-control); color: var(--text-disabled); box-shadow: inset 0 0 0 .5px var(--line-strong); }
  .btn.danger { background: var(--danger); color: var(--on-solid); box-shadow: var(--shadow-solid); }
  .btn.danger:hover { background: color-mix(in srgb, var(--danger) 88%, black); }

  h2 { display: flex; align-items: baseline; gap: var(--sp-2); margin: var(--sp-4) 0 var(--sp-2); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .dsub { font-weight: var(--fw-regular); font-size: var(--fs-footnote); color: var(--text-3); }
  ul { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); overflow: hidden; }
  li { position: relative; display: flex; align-items: center; gap: var(--sp-3); min-height: 52px; padding: var(--sp-3) var(--sp-4); box-sizing: border-box; }
  li + li::before { content: ""; position: absolute; top: 0; left: var(--sp-4); right: 0; height: 1px; transform: scaleY(.5); transform-origin: top; background: var(--line); }
  li:hover { background: var(--fill-hover); }
  .body { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .title { justify-self: start; max-width: 100%; padding: 0; border: 0; background: none; color: inherit; font: inherit; font-weight: var(--fw-semibold); text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* The whole row opens the meeting; the button is its keyboard and screen reader target. */
  .title::after { content: ""; position: absolute; inset: 0; }
  li .btn, .acts, .name { position: relative; z-index: 1; }
  .name { width: 100%; box-sizing: border-box; height: var(--h-ctl); padding: 0 var(--sp-2); margin-left: calc(-1 * var(--sp-2)); border: 0; border-radius: var(--r-md);
    background: var(--bg-raised); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; font-weight: var(--fw-semibold); }
  .name:focus-visible, .icon:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Space is kept while hidden, so a row never reflows on hover (as in Voice typing history). */
  .acts { display: flex; gap: 2px; flex: none; opacity: 0; transition: opacity var(--dur) var(--ease-out); }
  li:hover .acts, li:focus-within .acts { opacity: 1; }
  .icon { width: var(--h-ctl); height: var(--h-ctl); display: grid; place-items: center; border: 0; border-radius: var(--r-md); background: none; color: var(--text-2); }
  .icon:hover { background: var(--fill-hover); }
  .ask { color: var(--text-2); font-size: var(--fs-footnote); }
  .err { margin: var(--sp-2) 0 0; color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .meta { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-variant-numeric: tabular-nums; }
  .meta.fail { color: var(--danger-text); user-select: text; }

  .card { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-3) var(--sp-4); min-height: 52px; box-sizing: border-box; margin-top: var(--sp-2); padding: var(--sp-3) var(--sp-4); border-radius: var(--r-xl);
    background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .card.rec { padding: var(--sp-4) var(--sp-5); background: color-mix(in srgb, var(--rec) 9%, var(--bg-content)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--rec) 30%, transparent); }
  .strong { font-weight: var(--fw-semibold); }
  .sub { color: var(--text-3); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .main { display: grid; gap: 2px; }
  .label { display: flex; align-items: center; gap: var(--sp-2); font-weight: var(--fw-semibold); }
  .time { font-size: var(--fs-title2); line-height: 1.15; font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--rec); flex: none; animation: pulse 1.6s ease-in-out infinite; }
  .dot.idle, .paused .dot { background: var(--text-3); animation: none; }
  .paused .lvl { color: var(--text-3); }
  .paused .lvl i { animation: none; transform: scaleY(.3); }
  .lvl { display: flex; align-items: center; gap: 2px; height: 22px; color: var(--rec); }
  .lvl i { width: 3px; height: 22px; border-radius: 2px; background: currentColor; animation: lvl .9s ease-in-out infinite; }
  .lvl i:nth-child(2) { animation-delay: .15s; } .lvl i:nth-child(3) { animation-delay: .3s; } .lvl i:nth-child(4) { animation-delay: .45s; } .lvl i:nth-child(5) { animation-delay: .6s; }
  .mic { display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); }
  @keyframes lvl { 0%, 100% { transform: scaleY(.3); } 50% { transform: scaleY(1); } }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: .4; } }

  .empty { padding: var(--sp-6) var(--sp-4); text-align: center; }
  .empty h2 { display: block; margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); }
  .empty p { margin: 0 auto var(--sp-3); max-width: 42ch; color: var(--text-2); font-size: var(--fs-body); }
</style>
