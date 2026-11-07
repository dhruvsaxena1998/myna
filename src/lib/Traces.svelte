<!-- Stats for nerds (experimental): the Overview over the stored dictation traces, the dictations, newest first,
     and one opened: where the time went, and what the rules step changed. -->
<script module lang="ts">
  import type { Pick, Period } from "./stats";
  // Period, filters and the picked row last as long as the window is open. They aren't saved.
  const f = $state({ period: "7" as Period, model: "all", app: "all", pick: null as Pick });
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ClearAll from "./ClearAll.svelte";
  import Overview from "./Overview.svelte";
  import { edits as editsOf, overview, picks, PERIODS } from "./stats";
  import type { Rules, Word } from "./Words.svelte";
  import { dur, KIND_ONE, KINDS, OUTCOME, outermost, title, when, type Cleanup, type Edit, type Trace } from "./trace";

  let { traces = $bindable(), words, onclear }: { traces: Trace[]; words: Word[]; onclear: () => void } = $props();
  let open = $state<{ t: Trace; rules: Rules } | null>(null);
  let error = $state("");
  let off = $state<Record<string, boolean>>({});
  let tip = $state<{ e: Edit; raw: string; x: number; y: number } | null>(null);

  // The window stays open on stale traces: a dictation since it opened shows up here.
  $effect(() => {
    invoke<Trace[]>("traces_all").then((r) => (traces = r), () => {});
  });

  const ov = $derived(overview(traces, { ...f, now: Date.now(), words }));
  const list = $derived(ov.traces.filter((t) => picks(t, f.pick)));
  const models = $derived([...new Set(traces.map((t) => t.model ?? ""))]);
  const apps = $derived([...new Set(traces.map((t) => t.app ?? ""))].sort((a, b) => a.localeCompare(b)));
  const onpick = (p: NonNullable<Pick>) => (f.pick = f.pick?.kind === p.kind && f.pick.v === p.v ? null : p);
  const picked = $derived(f.pick?.kind === "rule" ? KINDS[f.pick.v as keyof typeof KINDS] : f.pick?.v);

  async function show(id: number) {
    error = "";
    try {
      // Everything first, then swap: the page never paints half a trace.
      const [t, rules] = await Promise.all([invoke<Trace>("trace_get", { id }), invoke<Rules>("rules_get")]);
      off = {};
      open = { t, rules };
    } catch (err) {
      error = String(err);
    }
  }

  const bad = (o: string) => o !== "pasted";
  const stt = $derived(open?.t.steps.find((s) => s.kind === "speech-to-text"));
  const rulesStep = $derived(open?.t.steps.find((s) => s.kind === "rules"));
  const cleanupStep = $derived(open?.t.steps.find((s) => s.kind === "cleanup"));
  const cleanup = $derived(cleanupStep?.settings as Cleanup | null | undefined);
  const price = (c: Cleanup["cost"]) => (c?.kind === "usd" ? (c.usd < 0.01 ? "< $0.01" : `$${c.usd.toFixed(2)}`) : c?.kind === "free" ? "Free" : "Cost unknown");
  const edits = $derived(rulesStep?.edits ?? []);
  const raw = $derived(Array.from(stt?.text ?? ""));
  const shown = $derived(outermost(edits));
  const counts = $derived(Object.fromEntries(Object.keys(KINDS).map((k) => [k, edits.filter((e) => e.kind === k && !(e.kind === "word" && e.unchanged)).length])));
  const total = $derived(Object.values(counts).reduce((a, b) => a + b, 0));
  // Text between the outermost edits, so the page can lay them out in order.
  const parts = $derived.by(() => {
    const out: ({ text: string } | { e: Edit })[] = [];
    let at = 0;
    for (const e of shown) {
      out.push({ text: raw.slice(at, e.start).join("") }, { e });
      at = e.end;
    }
    return [...out, { text: raw.slice(at).join("") }];
  });
  const old = (e: Edit) => raw.slice(e.start, e.end).join("");
  const unchanged = (e: Edit) => e.kind === "word" && e.unchanged;
  const inner = (e: Edit) => edits.filter((o) => o !== e && o.start >= e.start && o.end <= e.end);

  // The bar: what the paste step took is what's left of the whole after the other three.
  const segs = $derived.by(() => {
    if (!open) return [];
    const { t } = open, parts = [t.load_ms ?? 0, stt?.ms ?? 0, rulesStep?.ms ?? 0, cleanupStep?.ms ?? 0];
    const all = t.paste_ms ?? parts.reduce((a, b) => a + b, 0);
    return [
      ["Model load wait", parts[0], "--t-load"],
      ["Speech to text", parts[1], "--t-stt"],
      ["Rules", parts[2], "--t-rules"],
      ["Cleanup", parts[3], "--t-clean"],
      ["Paste", all - parts.reduce((a, b) => a + b, 0), "--t-paste"],
    ].filter((s) => (s[1] as number) >= 1) as [string, number, string][];
  });
  const whole = $derived(segs.reduce((a, s) => a + s[1], 0));

  const settings = $derived(rulesStep?.settings as { language: string; tidy: boolean } | null | undefined);
  const language = $derived(open?.rules.languages.find(([c]) => c === settings?.language)?.[1] ?? settings?.language ?? "");
  const wordStatus = (w: { spelling: string; heard: string | null }) => {
    const hits = edits.filter((e) => e.kind === "word" && e.spelling === w.spelling && (e.heard ?? null) === (w.heard ?? null));
    return !hits.length ? "not heard" : hits.some((e) => e.kind === "word" && !e.unchanged) ? "fixed" : "already right";
  };

  function hover(ev: Event, e: Edit) {
    const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
    tip = { e, raw: stt?.text ?? "", x: Math.max(4, Math.min(r.left, innerWidth - 310)), y: r.bottom + 6 };
  }
</script>

<header data-tauri-drag-region>
  <h1>Stats for nerds</h1>
  {#if !open}
    <span class="grow" data-tauri-drag-region></span>
    <ClearAll count={traces.length} onclear={() => { traces = []; f.pick = null; onclear(); }} />
  {/if}
</header>

<div class="content">
{#if open}
  {@const t = open.t}
  <div class="top">
    <button class="back" onclick={() => (open = null)}>‹ All dictations</button>
  </div>
  <div class="head">
    <div>
      <h2>{title(stt?.text ?? null)}</h2>
      <div class="meta">{[when(t.started), t.app, t.mode === "hold" ? "Hold to talk" : "Tap to start and stop", t.model].filter(Boolean).join(" · ")}</div>
    </div>
    <span class="badge" class:bad={bad(t.outcome)}>{OUTCOME[t.outcome] ?? t.outcome}</span>
  </div>
  <p class="reason meta" role="status">{t.reason ?? ""}</p>

  <section class="card" aria-label="Timing">
    <div class="line"><span class="big">{whole ? dur(t.paste_ms ?? whole) : "—"}</span><span class="meta">from letting go to {t.outcome === "pasted" ? "pasted" : "done"} · {(t.audio_ms / 1000).toFixed(1)} s of audio</span></div>
    <div class="tbar" role="img" aria-label={segs.map((s) => `${s[0]} ${dur(s[1])}`).join(", ")}>
      {#each segs as s}<i style="width:{(s[1] / whole) * 100}%;background:var({s[2]})"></i>{/each}
    </div>
    <div class="legend">{#each segs as s}<span style="--c:var({s[2]})">{s[0]} <b>{dur(s[1])}</b></span>{/each}</div>
  </section>

  {#if stt}
    <section class="card" aria-labelledby="chg-h">
      <h3 id="chg-h">What Myna changed <span class="meta">{total ? `${total} edit${total > 1 ? "s" : ""}` : "nothing"} · {language} · Tidy up {settings?.tidy ? "on" : "off"}{settings?.tidy && settings.language === "en" ? "" : " · only Words to remember ran"}</span></h3>
      <div class="text">
        {#each parts as p}{#if "e" in p}{@const e = p.e}{@const i = edits.indexOf(e)}
          {#if off[e.kind]}<span class="e dim">{e.to}</span>
          {:else}<!-- svelte-ignore a11y_no_noninteractive_tabindex --><span class="e" class:same={unchanged(e)} data-k={e.kind} data-i={i} tabindex="0" role="note" onmouseenter={(ev) => hover(ev, e)} onmouseleave={() => (tip = null)} onfocus={(ev) => hover(ev, e)} onblur={() => (tip = null)}>{#if unchanged(e)}{old(e)}{:else}<del>{old(e)}</del>{#if e.to}<ins>{e.to}</ins>{/if}{/if}</span>{/if}
        {:else}{p.text}{/if}{/each}
      </div>
      <p class="meta note">{rulesStep && !rulesStep.text ? "Nothing left after cleanup, so nothing was pasted." : ""}</p>
      {#if total}
        <div class="chips">
          {#each Object.entries(KINDS).filter(([k]) => counts[k]) as [k, label]}
            <button class="chip" data-k={k} aria-pressed={!off[k]} onclick={() => (off[k] = !off[k])}><i></i>{label} <b>{counts[k]}</b></button>
          {/each}
        </div>
      {/if}
      <div class="words"><span class="meta">Words to remember</span>
        {#each open.rules.settings.words as w}
          {@const s = wordStatus(w)}
          <span class="w" class:off={s === "not heard"}>{#if s === "not heard"}{w.heard ? `${w.heard} → ` : ""}{w.spelling}{:else}<b>{w.heard ? `${w.heard} → ` : ""}{w.spelling}</b>{/if}<span class="meta">{s}</span></span>
        {:else}<span class="meta">None on your list</span>{/each}
      </div>
    </section>
  {/if}

  {#if cleanupStep && cleanup}
    <section class="card" aria-labelledby="cln-h">
      <h3 id="cln-h">Cleanup <span class="meta">{[cleanup.model ?? "No Fast model", cleanup.model ? price(cleanup.cost) : "", dur(cleanupStep.ms)].filter(Boolean).join(" · ")}</span></h3>
      <div class="text">{cleanupStep.text}</div>
      <p class="meta note" role="status">{cleanup.fallback ? `Added without cleanup: ${cleanup.fallback}${cleanup.detail ? ` (${cleanup.detail})` : ""}` : ""}</p>
    </section>
  {/if}

  {#if tip}
    {@const e = tip.e}
    <div class="tip" data-k={e.kind} style="left:{tip.x}px;top:{tip.y}px">
      <b>{KIND_ONE[e.kind]}</b>{#if e.kind === "word"} · matched by {e.matched === "heard" ? `heard-as “${e.heard}”` : "spelling"}{/if}<br />
      “{old(e)}” → {e.to ? `“${e.to}”` : "removed"}{unchanged(e) ? " (already right)" : ""}
      {#each [inner(e)] as ins}{#if ins.length}<br /><span class="meta">Contains {ins.map((o) => `${KIND_ONE[o.kind].toLowerCase()} “${old(o).trim()}”`).join(", ")}, removed first</span>{/if}{/each}
    </div>
  {/if}
{:else}
  <div class="overview">
    <div class="filters">
      <fieldset class="seg" aria-label="Period">
        {#each PERIODS as [v, label]}
          <label><input type="radio" name="period" value={v} bind:group={f.period} /><span data-l={label}>{label}</span></label>
        {/each}
      </fieldset>
      <select class="select" aria-label="Model" bind:value={f.model}>
        <option value="all">All models</option>
        {#each models as m}<option value={m}>{m}</option>{/each}
      </select>
      <select class="select" aria-label="App" bind:value={f.app}>
        <option value="all">All apps</option>
        {#each apps as a}<option value={a}>{a}</option>{/each}
      </select>
    </div>
    <Overview {ov} pick={f.pick} {onpick} />
    <section aria-labelledby="list-h">
      <div class="lhead">
        <h2 id="list-h">Dictations <span class="n">{list.length}</span></h2>
        <span class="chipslot">{#if f.pick}<span class="only">Only {picked}<button aria-label="Show all dictations" onclick={() => (f.pick = null)}>×</button></span>{/if}</span>
      </div>
      {#if list.length}
        <ul class="list" aria-label="Dictations">
          {#each list as t (t.id)}
            {@const n = editsOf(t).length}
            <li>
              <button class="row" onclick={() => show(t.id)}>
                <span class="t"><span class="name">{title(t.steps.find((s) => s.kind === "speech-to-text")?.text ?? null)}</span><span class="meta">{[t.app, when(t.started), t.mode === "hold" ? "Hold" : "Tap"].join(" · ")}</span></span>
                <span class="nums">{t.outcome === "pasted" ? `${dur(t.paste_ms ?? 0)} · ${n} edit${n === 1 ? "" : "s"}` : ""}</span>
                <span class="badge" class:bad={bad(t.outcome)}>{OUTCOME[t.outcome] ?? t.outcome}</span>
              </button>
            </li>
          {/each}
        </ul>
      {:else if !traces.length}
        <div class="empty"><h3>No dictations yet</h3><p>Hold your voice typing key in any app and talk. Each dictation’s numbers show up here.</p></div>
      {:else}
        <div class="empty"><h3>No dictations in this period</h3><p>Pick a longer period to see older dictations.</p>
          {#if f.period !== "all"}<button class="btn" onclick={() => (f.period = "all")}>Show all</button>{/if}</div>
      {/if}
    </section>
    <p class="err" role="status">{error}</p>
  </div>
{/if}
</div>

<style>
  .grow { flex: 1; align-self: stretch; }
  /* Toolbar controls: 30px and centred with the title. The segmented control is a capsule track with a raised thumb. */
  .seg { display: inline-flex; gap: 2px; margin: 0; padding: 2px; border: 0; border-radius: var(--r-full); background: var(--fill-field); }
  .seg label { position: relative; }
  .seg input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .seg span { display: grid; align-content: center; justify-items: center; height: 26px; padding: 0 12px; border-radius: var(--r-full); color: var(--text-2); white-space: nowrap; }
  /* Room for the bold label, so choosing never widens the control. */
  .seg span::after { content: attr(data-l); height: 0; overflow: hidden; visibility: hidden; font-weight: var(--fw-medium); }
  .seg input:checked + span { background: var(--fill-control-on); color: var(--text); font-weight: var(--fw-medium); box-shadow: 0 0 0 .5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, .1); }
  .seg input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  .select { max-width: 200px; }
  /* minmax(0, 1fr): the tables' min-content width never pushes the cards past the panel; their .scroll scrolls instead. */
  .overview { display: grid; grid-template-columns: minmax(0, 1fr); gap: var(--sp-3); align-content: start; max-width: 1000px; margin: 0 auto; container: overview / inline-size; }
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2); }
  .lhead { display: flex; align-items: center; gap: var(--sp-3); height: 32px; margin: var(--sp-3) 0 var(--sp-1); }
  .lhead h2 { margin: 0; }
  .n { font-weight: var(--fw-regular); color: var(--text-3); font-variant-numeric: tabular-nums; margin-left: 4px; }
  /* The chip's space is always kept, so picking a row never moves the list. */
  .chipslot { min-height: 24px; display: inline-flex; }
  .only { display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 3px 0 10px; border-radius: var(--r-full); background: var(--accent-soft); color: var(--accent-text); font-size: var(--fs-footnote); font-weight: var(--fw-medium); }
  .only button { display: grid; place-items: center; width: 18px; height: 18px; border: 0; border-radius: 50%; background: none; color: inherit; font: inherit; font-size: 14px; line-height: 1; padding: 0; }
  .only button:hover { background: var(--accent-soft-strong); }
  .only button:focus-visible, .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .nums { color: var(--text-2); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .empty { padding: var(--sp-6) var(--sp-4); text-align: center; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .empty h3 { margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .empty p { margin: 0; color: var(--text-2); }
  .empty .btn { margin-top: var(--sp-3); }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); background: var(--fill-control); color: var(--text); font: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); }
  [data-k="filler"] { --kc: var(--k-filler); } [data-k="repeat"] { --kc: var(--k-repeat); } [data-k="cutoff"] { --kc: var(--k-cutoff); }
  [data-k="number"] { --kc: var(--k-number); } [data-k="word"] { --kc: var(--k-word); }
  .meta { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-weight: var(--fw-regular, 400); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); overflow: hidden; }
  .list li { position: relative; }
  .list li + li::before { content: ""; position: absolute; top: 0; left: var(--sp-4); right: 0; height: 1px; transform: scaleY(.5); transform-origin: top; background: var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: var(--sp-3); align-items: center; width: 100%; padding: var(--sp-3) var(--sp-4); border: 0; background: none; color: inherit; font: inherit; text-align: left; }
  .row:hover { background: var(--fill-hover); }
  .row:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; } /* inside: the list clips */
  .back:focus-visible, .chip:focus-visible, .e:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .t { display: grid; min-width: 0; } .name { font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { font-size: var(--fs-caption); border-radius: var(--r-full); padding: 0 8px; height: 20px; display: inline-flex; align-items: center; background: var(--fill-hover); color: var(--text-2); font-weight: 500; white-space: nowrap; }
  .badge.bad { background: var(--caution-soft); color: var(--caution-text); }
  .top { margin-bottom: var(--sp-2); }
  .back { border: 0; background: none; padding: 0; font: inherit; color: var(--accent-text); border-radius: var(--r-sm); }
  .head { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--sp-3); }
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .reason { margin: 2px 0 var(--sp-2); min-height: var(--lh-footnote); }
  .card { border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); padding: var(--sp-3) var(--sp-4); }
  .card + .card { margin-top: var(--sp-3); }
  h3 { margin: 0 0 2px; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--sp-2); }
  .line { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; margin-bottom: var(--sp-2); }
  .big { font-size: 26px; line-height: 32px; font-weight: var(--fw-semibold); font-variant-numeric: tabular-nums; }
  .tbar { display: flex; height: 10px; border-radius: 5px; overflow: hidden; background: var(--fill-track); gap: 2px; }
  .tbar i { display: block; height: 100%; }
  .legend { display: flex; flex-wrap: wrap; gap: 4px 16px; margin-top: var(--sp-2); min-height: var(--lh-footnote); font-size: var(--fs-footnote); color: var(--text-2); }
  .legend span { display: inline-flex; align-items: center; gap: 6px; font-variant-numeric: tabular-nums; }
  .legend span::before { content: ""; width: 8px; height: 8px; border-radius: 2px; background: var(--c); }
  .text { font-size: var(--fs-reading); line-height: 30px; margin-top: var(--sp-2); user-select: text; }
  .note { margin: 4px 0 0; min-height: var(--lh-footnote); }
  .e { border-radius: 4px; padding: 1px; }
  /* One mark per edit: what went is struck in the rule's colour, what came is underlined in it. The tint only shows on hover or focus, to tie the edit to its chip. */
  .e del { color: var(--text-3); text-decoration: line-through 1px var(--kc); } .e ins { text-decoration: underline 2px var(--kc); text-underline-offset: 4px; font-weight: var(--fw-medium); }
  .e[data-k]:hover, .e[data-k]:focus-visible { background: color-mix(in srgb, var(--kc) 14%, transparent); }
  .e.same { text-decoration: underline 1.5px color-mix(in srgb, var(--kc) 60%, transparent); text-underline-offset: 4px; }
  .e.dim { padding: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: var(--sp-3); }
  .chip { height: var(--h-ctl-sm, 24px); border-radius: var(--r-full); border: 0; padding: 0 10px 0 8px; display: inline-flex; align-items: center; gap: 6px; background: var(--fill-control); color: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong); font: inherit; font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .chip i { width: 8px; height: 8px; border-radius: 50%; background: var(--kc); }
  .chip[aria-pressed="false"] { opacity: .45; }
  .words { display: flex; flex-wrap: wrap; gap: 6px 14px; margin-top: var(--sp-3); padding-top: var(--sp-3); border-top: .5px solid var(--line); font-size: var(--fs-footnote); }
  .w { display: inline-flex; gap: 5px; align-items: baseline; } .w.off { color: var(--text-3); } .w b { font-weight: var(--fw-semibold); }
  .tip { position: fixed; z-index: 90; pointer-events: none; background: var(--bg-raised); border-radius: var(--r-sm); box-shadow: var(--shadow-menu); padding: 8px 10px; font-size: var(--fs-footnote); line-height: var(--lh-footnote); max-width: 300px; }
  .tip b { color: var(--kc); }
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
