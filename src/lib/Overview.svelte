<!-- Stats for nerds' Overview (#105): headline tiles, dictations per day, and By model, By rule and By app.
     A table row picks which dictations the list below shows; the Overview itself isn't filtered by it. -->
<script lang="ts">
  import { OUTCOMES, STEPS, shown, why, type Cell, type Overview, type Pick } from "./stats";
  import { dur, KINDS } from "./trace";

  let { ov, pick, onpick }: { ov: Overview; pick: Pick; onpick: (p: NonNullable<Pick>) => void } = $props();

  const pct = (c: Cell) => `${Math.round(c.v * 100)}%`;
  const split = (s: Overview["split"]) => OUTCOMES.map(([k, l]) => `${s[k]} ${l.toLowerCase()}`);
  const on = (kind: NonNullable<Pick>["kind"], v: string) => pick?.kind === kind && pick.v === v;
  const key = (e: KeyboardEvent, p: NonNullable<Pick>) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onpick(p); }
  };
  const xs = $derived([0, Math.floor(ov.chart.buckets.length / 2), ov.chart.buckets.length - 1].map((i) => ov.chart.buckets[i].label));
</script>

<!-- Under 10 dictations a median, p90, rate or share would mislead: a dash, and the tooltip says why. -->
{#snippet g(c: { n: number }, text: string)}{#if shown(c)}{text}{:else}<span class="gap" title={why(c.n)}>—</span>{/if}{/snippet}
{#snippet bar(parts: [string, number, string][], label: string)}
  <div class="thin" role="img" aria-label={label}>{#each parts.filter((p) => p[1] > 0) as p}<i style="flex:{p[1]} 1 0;background:var({p[2]})"></i>{/each}</div>
{/snippet}
{#snippet legend(items: [string, string][])}
  <div class="legend">{#each items as [l, c]}<span style="--c:var({c})">{l}</span>{/each}</div>
{/snippet}

<div class="tiles">
  <div class="tile">
    <span class="label">Dictations</span>
    <span class="big">{ov.n.toLocaleString("en")}</span>
    <span class="sub">{@render bar(OUTCOMES.map(([k, l, c]) => [l, ov.split[k], c]), split(ov.split).join(", "))}<span class="split">{split(ov.split).join(" · ")}</span></span>
  </div>
  <div class="tile">
    <span class="label">Let go → pasted</span>
    <span class="big">{@render g(ov.med, dur(ov.med.v))}</span>
    <span class="sub">{#if shown(ov.med)}median<br />p90 {@render g(ov.p90, dur(ov.p90.v))}{:else}{why(ov.med.n)}{/if}</span>
  </div>
  <div class="tile">
    <span class="label">Cold starts</span>
    <span class="big">{@render g(ov.cold, pct(ov.cold))}</span>
    <span class="sub">{#if shown(ov.cold)}waited for the model<br />median wait {@render g(ov.wait, dur(ov.wait.v))}{:else}{why(ov.cold.n)}{/if}</span>
  </div>
  <div class="tile">
    <span class="label">Edits per 100 words</span>
    <span class="big">{@render g(ov.per100, ov.per100.v.toFixed(1))}</span>
    <span class="sub">{ov.words.toLocaleString("en")} words heard, before cleanup</span>
  </div>
</div>

<section class="card" aria-labelledby="ov-chart">
  <div class="head"><h2 id="ov-chart">Dictations {ov.chart.hourly ? "by hour" : "per day"}</h2>{@render legend(OUTCOMES.map(([, l, c]) => [l, c]))}</div>
  <div class="chart">
    <div class="y" aria-hidden="true"><span>{ov.chart.top}</span><span>{ov.chart.top / 2}</span><span>0</span></div>
    <div class="plot" role="group" aria-label="Dictations {ov.chart.hourly ? 'by hour' : 'per day'}">
      {#each ov.chart.buckets as b}
        {@const parts = OUTCOMES.filter(([k]) => b.split[k])}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <div class="col" tabindex="0" role="img" aria-label="{b.label}: {b.n} dictation{b.n === 1 ? '' : 's'}{b.n ? ', ' + split(b.split).join(', ') : ''}">
          <div class="stack" style="height:{(b.n / ov.chart.top) * 100}%">{#each parts as [k, , c]}<i style="flex:{b.split[k]} 1 0;background:var({c})"></i>{/each}</div>
          <div class="ctip" aria-hidden="true"><b>{b.label}</b><br />{b.n} dictation{b.n === 1 ? "" : "s"}{#if b.n}<br />{parts.map(([k, l]) => `${l} ${b.split[k]}`).join(" · ")}{/if}</div>
        </div>
      {/each}
      {#if !ov.n}<p class="none">No dictations in this period</p>{/if}
    </div>
  </div>
  <div class="x" aria-hidden="true">{#each xs as l}<span>{l}</span>{/each}</div>
</section>

<section class="card tcard" aria-labelledby="ov-model">
  <div class="head"><h2 id="ov-model">By model</h2>{@render legend(STEPS)}</div>
  <div class="scroll">
    <table>
      <thead><tr>
        <th scope="col">Model</th><th scope="col" class="num">Dictations</th>
        <th scope="col" class="num" title="Median · p90, pasted dictations only">Let go → pasted</th>
        <th scope="col" class="num" title="Median of each step, pasted dictations only">Where the time goes</th>
      </tr></thead>
      <tbody>
        {#each ov.models as m (m.name)}
          {@const p = { kind: "model" as const, v: m.name }}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <tr tabindex="0" class="act" aria-selected={on("model", m.name)} onclick={() => onpick(p)} onkeydown={(e) => key(e, p)}>
            <!-- Four columns fit the default window; the rest of the numbers ride on a second line. -->
            <th scope="row"><span class="mname">{m.name}</span>
              <span class="more"><span title="Seconds of audio per second of speech-to-text, pasted dictations only">Speed {@render g(m.speed, `${Math.round(m.speed.v)}×`)}</span> · Cold starts {@render g(m.cold, pct(m.cold))} · Nothing heard {@render g(m.nothing, pct(m.nothing))} · Failed {@render g(m.failed, pct(m.failed))} · <span title="Median words per dictation">Words {@render g(m.words, String(m.words.v))}</span></span></th>
            <td class="num">{m.n}</td>
            <td class="num">{@render g(m.med, `${dur(m.med.v)} · ${dur(m.p90.v)}`)}</td>
            <td class="num">{#if shown(m.steps)}{@render bar(STEPS.map(([l, c], i) => [l, m.steps.v[i], c]), STEPS.map(([l], i) => `${l} ${dur(m.steps.v[i])}`).join(", "))}{:else}{@render g(m.steps, "")}{/if}</td>
          </tr>
        {:else}
          <tr><td colspan="4" class="empty-row">No dictations in this period</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<section class="card tcard" aria-labelledby="ov-rule">
  <div class="head"><h2 id="ov-rule">By rule</h2></div>
  <div class="scroll">
    <table>
      <thead><tr>
        <th scope="col">Rule</th><th scope="col" class="num">Edits</th>
        <th scope="col" class="num" title="Counts the words speech to text heard, before cleanup">Per 100 words</th>
        <th scope="col" class="num" title="Share of dictations with at least one edit of this rule">Dictations touched</th>
      </tr></thead>
      <tbody>
        {#each ov.rules as r (r.kind)}
          {@const p = { kind: "rule" as const, v: r.kind }}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
          <tr tabindex="0" class="act" aria-selected={on("rule", r.kind)} onclick={() => onpick(p)} onkeydown={(e) => key(e, p)}>
            <th scope="row"><span class="dot" data-k={r.kind}></span>{KINDS[r.kind]}</th>
            <td class="num">{r.edits}</td>
            <td class="num">{@render g(r.per100, r.per100.v.toFixed(1))}</td>
            <td class="num">{@render g(r.touched, pct(r.touched))}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <div class="under">
    <div>
      <h3>Fillers removed most</h3>
      {#if ov.fillers.length}<ul class="kv">{#each ov.fillers as [f, c]}<li><span>“{f}”</span><b>{c}</b></li>{/each}</ul>
      {:else}<p class="help">None in this period</p>{/if}
    </div>
    <div>
      <h3>Words to remember</h3>
      {#if ov.heardWords.length}<ul class="kv">{#each ov.heardWords as w}<li><span>{w.heard ? `${w.heard} → ` : ""}{w.spelling}</span><b>fixed {w.fixed} · already right {w.right}</b></li>{/each}</ul>
      {:else}<p class="help">{ov.neverHeard.length ? "None heard in this period" : "None on your list"}</p>{/if}
      {#if ov.neverHeard.length}
        <h3 class="never">Never heard</h3>
        <p class="help">{ov.neverHeard.map((w) => (w.heard ? `${w.heard} → ` : "") + w.spelling).join(", ")}. Myna hasn’t heard these in this period. If you say them, check how speech to text writes them and add that as “heard as”.</p>
      {/if}
    </div>
  </div>
</section>

<section class="card tcard" aria-labelledby="ov-app">
  <div class="head"><h2 id="ov-app">By app</h2></div>
  <div class="scroll">
    <table>
      <thead><tr>
        <th scope="col">App</th><th scope="col" class="num">Dictations</th><th scope="col" class="num">Pasted</th>
        <th scope="col" class="num">Failed or not pasted</th><th scope="col" class="num" title="Median, pasted dictations only">Let go → pasted</th>
      </tr></thead>
      <tbody>
        {#each ov.apps as a (a.name)}
          {#if a.name === null}
            <tr class="other"><th scope="row">Other <span class="help">({a.others} apps)</span></th>{@render appCells(a)}</tr>
          {:else}
            {@const p = { kind: "app" as const, v: a.name }}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
            <tr tabindex="0" class="act" aria-selected={on("app", a.name)} onclick={() => onpick(p)} onkeydown={(e) => key(e, p)}><th scope="row">{a.name}</th>{@render appCells(a)}</tr>
          {/if}
        {:else}
          <tr><td colspan="5" class="empty-row">No dictations in this period</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

{#snippet appCells(a: Overview["apps"][number])}
  <td class="num">{a.n}</td><td class="num">{@render g(a.pastedShare, pct(a.pastedShare))}</td>
  <td class="num">{@render g(a.failedShare, pct(a.failedShare))}</td><td class="num">{@render g(a.med, dur(a.med.v))}</td>
{/snippet}

<style>
  .tiles { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--sp-3); }
  /* Two across when the panel is narrow: follows the panel (Traces' .overview), not the screen. */
  @container overview (width < 640px) { .tiles { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .tile, .card { background: var(--bg-inset); border-radius: var(--r-lg); box-shadow: inset 0 0 0 .5px var(--line); padding: var(--sp-3) var(--sp-4); }
  /* Fixed rows: a dash and a real value, one line of split or two, take the same space. */
  .tile { display: grid; grid-template-rows: 16px 32px 50px; gap: 2px; }
  .label, .sub, .help { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .help { margin: 0; }
  .big { font-size: 26px; line-height: 32px; font-weight: var(--fw-semibold); }
  .sub .thin { margin: 4px 0 6px; }
  .split { display: block; font-variant-numeric: tabular-nums; }
  .gap { color: var(--text-3); cursor: help; }
  .thin { display: flex; gap: 2px; height: 8px; border-radius: 4px; overflow: hidden; background: var(--fill-hover); min-width: 60px; }
  .thin i { display: block; height: 100%; }
  .head { display: flex; align-items: baseline; flex-wrap: wrap; gap: var(--sp-1) var(--sp-4); justify-content: space-between; margin-bottom: var(--sp-2); }
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  h3 { margin: 0 0 var(--sp-1); font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-weight: var(--fw-semibold); color: var(--text-2); }
  .legend { display: flex; flex-wrap: wrap; gap: 4px 14px; font-size: var(--fs-footnote); color: var(--text-2); }
  .legend span { display: inline-flex; align-items: center; gap: 6px; }
  .legend span::before { content: ""; width: 8px; height: 8px; border-radius: 2px; background: var(--c); }

  .chart { display: grid; grid-template-columns: 24px minmax(0, 1fr); gap: var(--sp-2); height: 132px; }
  .y { display: flex; flex-direction: column; justify-content: space-between; text-align: right; font-size: var(--fs-caption); color: var(--text-3); font-variant-numeric: tabular-nums; margin: -7px 0; }
  .plot { position: relative; display: flex; align-items: flex-end; gap: 2px; border-bottom: 1px solid var(--line-strong);
    background: linear-gradient(var(--line) 1px, transparent 1px) 0 0 / 100% 50%; }
  .col { position: relative; flex: 1; min-width: 0; height: 100%; display: flex; align-items: flex-end; justify-content: center; outline: 0; }
  .col:hover, .col:focus-visible { background: var(--fill-hover); border-radius: 4px 4px 0 0; }
  .col:focus-visible { box-shadow: 0 0 0 2px var(--focus-ring); }
  .stack { width: 100%; max-width: 24px; display: flex; flex-direction: column-reverse; gap: 2px; border-radius: 4px 4px 0 0; overflow: hidden; }
  .stack i { display: block; min-height: 2px; }
  .ctip { display: none; position: absolute; bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%); z-index: 5; white-space: nowrap; background: var(--bg-raised);
    box-shadow: var(--shadow-menu); border-radius: var(--r-sm); padding: 6px 9px; font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-variant-numeric: tabular-nums; pointer-events: none; }
  .col:hover .ctip, .col:focus-visible .ctip { display: block; }
  .col:first-child .ctip { left: 0; transform: none; } .col:last-of-type .ctip { left: auto; right: 0; transform: none; }
  .none { position: absolute; inset: 0; display: grid; place-items: center; margin: 0; color: var(--text-3); font-size: var(--fs-footnote); pointer-events: none; }
  .x { display: flex; justify-content: space-between; margin: 4px 0 0 32px; font-size: var(--fs-caption); color: var(--text-3); }

  .tcard { padding-bottom: var(--sp-2); }
  .scroll { overflow-x: auto; scrollbar-width: thin; }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body); }
  th, td { height: 32px; padding: 0 var(--sp-2); text-align: left; font-weight: var(--fw-regular); white-space: nowrap; border-top: .5px solid var(--line); }
  thead th { height: 24px; border-top: 0; color: var(--text-3); font-size: var(--fs-footnote); }
  thead th[title] { cursor: help; text-decoration: underline dotted var(--line-strong); text-underline-offset: 3px; }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  td.num:last-child .thin { margin-left: auto; width: 120px; }
  tbody th { font-weight: var(--fw-medium); overflow: hidden; text-overflow: ellipsis; max-width: 240px; }
  tr.act { cursor: pointer; }
  tr.act:hover { background: var(--fill-hover); }
  tr.act:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
  tr[aria-selected="true"] { background: var(--accent-soft); }
  tr[aria-selected="true"] th { font-weight: var(--fw-semibold); }
  .other th { color: var(--text-2); }
  .empty-row { color: var(--text-3); text-align: center; }
  .mname { display: block; overflow: hidden; text-overflow: ellipsis; }
  .more { display: block; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-weight: var(--fw-regular); font-variant-numeric: tabular-nums; }
  th:has(.more) { height: auto; padding-block: var(--sp-2); max-width: none; white-space: normal; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 8px; background: var(--kc); vertical-align: 1px; }
  [data-k="filler"] { --kc: var(--k-filler); } [data-k="repeat"] { --kc: var(--k-repeat); } [data-k="cutoff"] { --kc: var(--k-cutoff); }
  [data-k="number"] { --kc: var(--k-number); } [data-k="word"] { --kc: var(--k-word); }
  .under { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: var(--sp-5); border-top: .5px solid var(--line); padding: var(--sp-3) var(--sp-2) var(--sp-2); }
  .never { margin-top: var(--sp-3); }
  .kv { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; font-size: var(--fs-footnote); }
  .kv li { display: flex; justify-content: space-between; gap: var(--sp-3); }
  .kv b { font-weight: var(--fw-regular); color: var(--text-2); font-variant-numeric: tabular-nums; }
</style>
