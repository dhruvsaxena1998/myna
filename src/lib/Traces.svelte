<!-- Stats for nerds (experimental): the stored dictation traces, newest first, and one opened:
     where the time went, and what the rules step changed. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Rules } from "./Words.svelte";
  import { dur, KIND_ONE, KINDS, OUTCOME, outermost, title, when, type Edit, type Row, type Trace } from "./trace";

  let { initial }: { initial: Row[] } = $props();
  // svelte-ignore state_referenced_locally
  let rows = $state(initial);
  let open = $state<{ t: Trace; rules: Rules } | null>(null);
  let error = $state("");
  let off = $state<Record<string, boolean>>({});
  let tip = $state<{ e: Edit; raw: string; x: number; y: number } | null>(null);

  // The window stays open on a stale list: a dictation since it opened shows up here.
  $effect(() => {
    invoke<Row[]>("traces_list").then((r) => (rows = r), () => {});
  });

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
    const { t } = open, parts = [t.load_ms ?? 0, stt?.ms ?? 0, rulesStep?.ms ?? 0];
    const all = t.paste_ms ?? parts.reduce((a, b) => a + b, 0);
    return [
      ["Model load wait", parts[0], "--t-load"],
      ["Speech to text", parts[1], "--t-stt"],
      ["Rules", parts[2], "--t-rules"],
      ["Paste", all - parts.reduce((a, b) => a + b, 0), "--t-paste"],
    ].filter((s) => (s[1] as number) >= 1) as [string, number, string][];
  });
  const whole = $derived(segs.reduce((a, s) => a + s[1], 0));

  const settings = $derived(rulesStep?.settings);
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

  {#if tip}
    {@const e = tip.e}
    <div class="tip" data-k={e.kind} style="left:{tip.x}px;top:{tip.y}px">
      <b>{KIND_ONE[e.kind]}</b>{#if e.kind === "word"} · matched by {e.matched === "heard" ? `heard-as “${e.heard}”` : "spelling"}{/if}<br />
      “{old(e)}” → {e.to ? `“${e.to}”` : "removed"}{unchanged(e) ? " (already right)" : ""}
      {#each [inner(e)] as ins}{#if ins.length}<br /><span class="meta">Contains {ins.map((o) => `${KIND_ONE[o.kind].toLowerCase()} “${old(o).trim()}”`).join(", ")}, removed first</span>{/if}{/each}
    </div>
  {/if}
{:else}
  <ul class="list" aria-label="Dictations">
    {#each rows as r (r.id)}
      <li>
        <button class="row" onclick={() => show(r.id)}>
          <span class="t"><span class="name">{title(r.heard)}</span><span class="meta">{[r.app, when(r.started)].filter(Boolean).join(" · ")}</span></span>
          <span class="badge" class:bad={bad(r.outcome)}>{OUTCOME[r.outcome] ?? r.outcome}</span>
        </button>
      </li>
    {:else}
      <li class="empty">No dictations yet. Each one you make with voice typing shows up here.</li>
    {/each}
  </ul>
  <p class="err" role="status">{error}</p>
{/if}

<style>
  /* Rule colours: the tokens have no categorical set yet, so these live here, light and dark. */
  .head, .card, .tip { --k-filler: light-dark(#2B6CD8, #6AA8FF); --k-repeat: light-dark(#C26A00, #FFB04A); --k-cutoff: light-dark(#8A47C2, #C99BFF);
    --k-number: light-dark(#0E8A7A, #4FD1BE); --k-word: light-dark(#C2306B, #FF7AA8);
    --t-load: light-dark(#C77700, #E0A030); --t-stt: light-dark(#0A64D8, #3D8BFF); --t-rules: light-dark(#5E9BEA, #7FB0FF); --t-paste: light-dark(#A9C8F3, #3A5F96); }
  [data-k="filler"] { --kc: var(--k-filler); } [data-k="repeat"] { --kc: var(--k-repeat); } [data-k="cutoff"] { --kc: var(--k-cutoff); }
  [data-k="number"] { --kc: var(--k-number); } [data-k="word"] { --kc: var(--k-word); }
  .meta { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-weight: var(--fw-regular, 400); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .list li + li { box-shadow: 0 -.5px 0 var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; width: 100%; padding: var(--sp-3) var(--sp-4); border: 0; background: none; color: inherit; font: inherit; text-align: left; border-radius: var(--r-xl); }
  .row:hover { background: var(--fill-hover); }
  .row:focus-visible, .back:focus-visible, .chip:focus-visible, .e:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .t { display: grid; min-width: 0; } .name { font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: var(--sp-4); color: var(--text-2); }
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
  .e del { text-decoration-thickness: 1.5px; opacity: .75; } .e ins { text-decoration: none; font-weight: 600; }
  .e[data-k] { background: color-mix(in srgb, var(--kc) 14%, transparent); box-shadow: inset 0 -2px 0 var(--kc); }
  .e[data-k] del { color: var(--kc); }
  .e.same { background: none; box-shadow: inset 0 -1.5px 0 color-mix(in srgb, var(--kc) 60%, transparent); }
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
