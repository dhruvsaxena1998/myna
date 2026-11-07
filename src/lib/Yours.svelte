<!-- "Your dictation" (docs/index.html `yoursView()`): personal numbers from the traces the window already loaded. -->
<script lang="ts">
  import type { Trace } from "./trace";
  import { minutes, TYPING_WPM, yours } from "./yours";

  let { traces }: { traces: Trace[] } = $props();
  let period = $state<7 | 30>(7);
  const y = $derived(yours(traces, period, Date.now()));
  const xs = $derived([0, Math.floor(y.buckets.length / 2), y.buckets.length - 1].map((i) => y.buckets[i].label));
  const n = (v: number) => v.toLocaleString("en");
  const PERIODS = [[7, "7 days"], [30, "30 days"]] as const;
</script>

<header data-tauri-drag-region>
  <h1>Your dictation</h1>
  <span class="grow" data-tauri-drag-region></span>
  <fieldset class="seg" aria-label="Period">
    {#each PERIODS as [v, label]}
      <label><input type="radio" name="yours-period" value={v} bind:group={period} /><span data-l={label}>{label}</span></label>
    {/each}
  </fieldset>
</header>

<div class="content">
  <div class="page">
    <div class="tiles">
      <div class="tile"><span class="label">Words dictated</span><span class="big">{n(y.words)}</span><span class="sub">pasted in the last {period} days</span></div>
      <div class="tile"><span class="label">Time saved</span><span class="big">{minutes(y.saved)}</span><span class="sub">against typing at {TYPING_WPM} words a minute</span></div>
      <div class="tile"><span class="label">Dictations</span><span class="big">{n(y.n)}</span><span class="sub">pasted in the last {period} days</span></div>
      <div class="tile"><span class="label">Streak</span><span class="big">{y.streak} {y.streak === 1 ? "day" : "days"}</span><span class="sub">{y.streak ? "days in a row" : "Dictate today to start one"}</span></div>
    </div>

    <section class="card" aria-labelledby="yr-chart">
      <div class="head"><h2 id="yr-chart">Words per day</h2></div>
      <div class="chart">
        <div class="y" aria-hidden="true"><span>{y.top}</span><span>{y.top / 2}</span><span>0</span></div>
        <div class="plot" role="group" aria-label="Words per day">
          {#each y.buckets as b}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div class="col" tabindex="0" role="img" aria-label="{b.label}: {b.words} word{b.words === 1 ? '' : 's'}">
              <div class="stack" style="height:{(b.words / y.top) * 100}%"></div>
              <div class="ctip" aria-hidden="true"><b>{b.label}</b><br />{b.words} word{b.words === 1 ? "" : "s"}</div>
            </div>
          {/each}
          {#if !y.n}<p class="none">No dictations in this period</p>{/if}
        </div>
      </div>
      <div class="x" aria-hidden="true">{#each xs as l}<span>{l}</span>{/each}</div>
    </section>

    <section class="card" aria-labelledby="yr-apps">
      <div class="head"><h2 id="yr-apps">Top apps</h2></div>
      {#each y.apps as a (a.name)}
        <div class="app"><span class="name">{a.name}</span><span class="w">{n(a.words)} words</span>
          <div class="thin" role="img" aria-label="{a.name}: {n(a.words)} words"><i style="width:{(a.words / y.apps[0].words) * 100}%"></i></div></div>
      {:else}
        <p class="help">No dictations in this period</p>
      {/each}
    </section>
  </div>
</div>

<style>
  .page { display: grid; grid-template-columns: minmax(0, 1fr); gap: var(--sp-3); align-content: start; max-width: 1000px; margin: 0 auto; container: yours / inline-size; }
  .tiles { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--sp-3); }
  @container yours (width < 640px) { .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .tile, .card { background: var(--bg-inset); border-radius: var(--r-lg); box-shadow: inset 0 0 0 .5px var(--line); padding: var(--sp-3) var(--sp-4); }
  /* Fixed rows: every value and sub line takes the same space. */
  .tile { display: grid; grid-template-rows: 16px 32px 34px; gap: 2px; }
  .label, .sub, .help { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .help { margin: 0; }
  .big { font-size: 26px; line-height: 32px; font-weight: var(--fw-semibold); white-space: nowrap; }
  .head { display: flex; align-items: baseline; margin-bottom: var(--sp-2); }
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }

  .chart { display: grid; grid-template-columns: 32px minmax(0, 1fr); gap: var(--sp-2); height: 132px; }
  .y { display: flex; flex-direction: column; justify-content: space-between; text-align: right; font-size: var(--fs-caption); color: var(--text-3); font-variant-numeric: tabular-nums; margin: -7px 0; }
  .plot { position: relative; display: flex; align-items: flex-end; gap: 2px; border-bottom: 1px solid var(--line-strong);
    background: linear-gradient(var(--line) 1px, transparent 1px) 0 0 / 100% 50%; }
  .col { position: relative; flex: 1; min-width: 0; height: 100%; display: flex; align-items: flex-end; justify-content: center; outline: 0; }
  .col:hover, .col:focus-visible { background: var(--fill-hover); border-radius: 4px 4px 0 0; }
  .col:focus-visible { box-shadow: 0 0 0 2px var(--focus-ring); }
  /* One series, so one colour: the fixed data colour, not the accent. */
  .stack { width: 100%; max-width: 24px; background: var(--o-pasted); border-radius: 4px 4px 0 0; min-height: 0; }
  .ctip { display: none; position: absolute; bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%); z-index: 5; white-space: nowrap; background: var(--bg-raised);
    box-shadow: var(--shadow-menu); border-radius: var(--r-sm); padding: 6px 9px; font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-variant-numeric: tabular-nums; pointer-events: none; }
  .col:hover .ctip, .col:focus-visible .ctip { display: block; }
  .col:first-child .ctip { left: 0; transform: none; } .col:last-of-type .ctip { left: auto; right: 0; transform: none; }
  .none { position: absolute; inset: 0; display: grid; place-items: center; margin: 0; color: var(--text-3); font-size: var(--fs-footnote); pointer-events: none; }
  .x { display: flex; justify-content: space-between; margin: 4px 0 0 40px; font-size: var(--fs-caption); color: var(--text-3); }

  .app { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 4px var(--sp-3); padding: var(--sp-2) 0; border-top: .5px solid var(--line); }
  .app:first-of-type { border-top: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: var(--fw-medium); }
  .w { color: var(--text-2); font-variant-numeric: tabular-nums; }
  .thin { grid-column: 1 / -1; height: 8px; border-radius: 4px; overflow: hidden; background: var(--fill-hover); }
  .thin i { display: block; height: 100%; background: var(--o-pasted); }

  .grow { flex: 1; }
  .seg { display: inline-flex; gap: 2px; margin: 0; padding: 2px; border: 0; border-radius: var(--r-full); background: var(--fill-field); }
  .seg label { position: relative; }
  .seg input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .seg span { display: grid; align-content: center; justify-items: center; height: 26px; padding: 0 12px; border-radius: var(--r-full); color: var(--text-2); white-space: nowrap; }
  .seg span::after { content: attr(data-l); height: 0; overflow: hidden; visibility: hidden; font-weight: var(--fw-medium); }
  .seg input:checked + span { background: var(--fill-control-on); color: var(--text); font-weight: var(--fw-medium); box-shadow: 0 0 0 .5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, .1); }
  .seg input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
</style>
