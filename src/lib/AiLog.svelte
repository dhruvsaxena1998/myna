<!-- AI log (experimental): every call Myna made to an AI provider, one row per dictation or meeting, newest first;
     a row opens its calls in the order they ran, and a call opens with what was sent and what came back. Laid out
     like Stats for nerds (Traces.svelte). The backend half is #247. -->
<script module lang="ts">
  import type { Cost } from "./meetings";
  export type Outcome = "ok" | "error" | "timeout" | "refused" | "blocked";
  export type Summary = { id: string; at: number; feature: string; step?: string | null; entity?: string | null; provider?: string | null; kind?: string | null; model?: string | null;
    method: string; outcome: Outcome; total_ms?: number | null; cost?: Cost | null; preview?: string | null };
  type Decision = { question?: string | null; options?: string[] | null; instructions?: string | null; via?: string | null; choice?: string | null; confidence?: number | null };
  export type Entry = Omit<Summary, "method" | "outcome" | "preview"> & { method?: string | null; outcome?: Outcome | null;
    system?: string | null; prompt?: string | null; decision?: Decision | null; text?: string | null; error?: string | null;
    ttft_ms?: number | null; limit_s?: number | null; tokens_in?: number | null; tokens_out?: number | null; audio_ms?: number | null; segments?: unknown };
  const OUTCOME: Record<string, string> = { ok: "OK", error: "Error", timeout: "Timed out", refused: "Refused", blocked: "Blocked" };
  const METHOD: Record<string, string> = { chat: "Chat", chat_stream: "Streamed chat", decide: "Decision", check: "Connection check", models: "Model list", key_usage: "Key usage", transcribe: "Transcription" };
  const KIND: Record<string, string> = { meeting: "Meeting", dictation: "Dictation" };
  // Filters last as long as the window is open, like Stats for nerds'.
  const f = $state({ errors: false });
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ClearAll from "./ClearAll.svelte";
  import { dur, when } from "./trace";

  let calls = $state<Summary[]>([]);
  let open = $state<Entry | null>(null);
  // The dictation or meeting being looked at: its `entity`.
  let group = $state<string | null>(null);
  let error = $state("");
  let copied = $state("");
  let content = $state<HTMLElement>();
  let timer: ReturnType<typeof setTimeout>;

  $effect(() => {
    // A call that lands before the list does stays on top, once.
    invoke<Summary[]>("ai_log_list", {}).then((r) => {
      const seen = new Set(calls.map((c) => c.id));
      calls = [...calls, ...r.filter((c) => !seen.has(c.id))];
    }, (err) => (error = String(err)));
    const un = listen<Summary>("ai-log", ({ payload }) => (calls = [payload, ...calls.filter((c) => c.id !== payload.id)]));
    return () => void un.then((u) => u());
  });
  // Each view opens at its top, not where the last one was scrolled to.
  $effect(() => {
    void [open, group];
    if (content) content.scrollTop = 0;
  });

  // One row per dictation or meeting, newest first; a call made for neither (Test, Decision) is a row of its own.
  type Group = { key: string; calls: Summary[] };
  const groups = $derived.by(() => {
    const by = new Map<string, Summary[]>();
    for (const c of calls) {
      const k = c.entity ?? c.id;
      by.get(k)?.push(c) ?? by.set(k, [c]);
    }
    return [...by].map(([key, calls]): Group => ({ key, calls }));
  });
  const bad = (c: Summary) => c.outcome !== "ok";
  const list = $derived(groups.filter((g) => !f.errors || g.calls.some(bad)));
  const current = $derived(groups.find((g) => g.key === group));
  // In the order they ran.
  const inGroup = $derived((current?.calls ?? []).filter((c) => !f.errors || bad(c)).reverse());
  const kind = (g: Group) => (g.calls[0].entity ? KIND[g.calls[0].entity.split(":")[0]] : undefined);
  // A meeting goes by its Title's answer, a dictation by its cleaned text; else by what it is.
  const said = (g: Group, feature?: string) => g.calls.find((c) => !bad(c) && c.preview && (!feature || c.feature === feature))?.preview;
  const name = (g: Group) => (kind(g) === "Meeting" ? said(g, "Title") : kind(g) === "Dictation" ? said(g) : undefined) ?? kind(g) ?? label(g.calls[0]);
  const failed = (g: Group) => g.calls.filter(bad).length;
  // Time spent waiting: calls that overlap (speech-to-text chunks run side by side) count once, gaps between runs don't count.
  function span(cs: Summary[]) {
    let sum = 0, end = -Infinity;
    for (const [a, b] of cs.map((c) => [c.at, c.at + (c.total_ms ?? 0)]).sort((x, y) => x[0] - y[0])) {
      if (b > end) sum += b - Math.max(a, end), end = b;
    }
    return sum;
  }
  const ms = (n: number | null | undefined) => (n == null ? "—" : dur(n));
  const where = (c: Pick<Summary, "provider" | "model">) => [c.provider, c.model].filter(Boolean).join(" · ");
  // The provider and model most of a group's calls used, said once in its header.
  const usual = $derived.by(() => {
    const n = new Map<string, number>();
    for (const c of current?.calls ?? []) n.set(where(c), (n.get(where(c)) ?? 0) + 1);
    return [...n].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "";
  });
  const groupMeta = $derived(current ? [kind(current) !== name(current) && kind(current), `${current.calls.length} ${current.calls.length === 1 ? "call" : "calls"}`, ms(span(current.calls)), usual].filter(Boolean).join(" · ") : "");
  function pick(g: Group) {
    if (g.calls.length === 1 && !g.calls[0].entity) return show(g.calls[0].id);
    group = g.key;
  }
  const home = () => ((open = null), (group = null));
  const label = (c: Pick<Summary, "feature" | "step">) => [c.feature, c.step].filter(Boolean).join(" · ");
  const usd = (n: number) => "$" + (n >= 0.01 || n === 0 ? n.toFixed(2) : n.toPrecision(2));
  const price = (c: Cost | null | undefined, kind?: string | null) =>
    !c ? "—" : c.kind === "usd" ? usd(c.usd) : c.kind === "free" ? "Free" : kind === "agent" ? "On your plan" : "Cost unknown";

  async function show(id: string) {
    error = "";
    try {
      const e = await invoke<Entry | null>("ai_log_get", { id });
      if (!e) throw "This call is no longer in the AI log.";
      open = e;
    } catch (err) {
      error = String(err);
    }
  }
  async function copy(key: string, text: string) {
    try {
      await invoke("text_copy", { text });
      copied = key;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = ""), 1500);
    } catch (err) {
      error = String(err);
    }
  }

  const details = $derived.by(() => {
    if (!open) return [];
    const e = open, tokens = e.tokens_in == null && e.tokens_out == null ? "—" : `${e.tokens_in?.toLocaleString("en-US") ?? "?"} in · ${e.tokens_out?.toLocaleString("en-US") ?? "?"} out`;
    return [
      ["Provider", e.provider ?? "—"], ["Kind", e.kind === "agent" ? "Agent" : e.kind === "endpoint" ? "Endpoint" : (e.kind ?? "—")], ["Model", e.model ?? "—"],
      ["Outcome", OUTCOME[e.outcome ?? ""] ?? e.outcome ?? "—"], ["First token", ms(e.ttft_ms)], ["Total", ms(e.total_ms)],
      ["Time limit", e.limit_s == null ? "—" : `${e.limit_s} s`], ["Tokens", tokens], ["Cost", price(e.cost, e.kind)],
      ...(e.audio_ms != null ? [["Clip", `${(e.audio_ms / 1000).toFixed(1)} s`]] : []),
    ];
  });
  // Why it failed comes first; the raw reply isn't shown (the text Myna took from it is).
  const sections = $derived.by(() => {
    if (!open) return [];
    const e = open, d = e.decision;
    const decision = d && [d.question && `Question: ${d.question}`, d.options?.length && `Options: ${d.options.join(", ")}`, d.instructions && `Instructions: ${d.instructions}`,
      d.via && `Via: ${d.via}`, d.choice && `Choice: ${d.choice}${d.confidence != null ? ` (${Math.round(d.confidence * 100)}% sure)` : ""}`].filter(Boolean).join("\n");
    const segments = e.segments == null ? "" : typeof e.segments === "string" ? e.segments : JSON.stringify(e.segments, null, 2);
    return ([
      ["error", "Error", e.error, false], ["system", "System prompt", e.system, false], ["prompt", "Prompt", e.prompt, false], ["decision", "Decision details", decision, false],
      ["text", "Text", e.text, false], ["segments", "Segments", segments, true],
    ] as [string, string, string | null | undefined, boolean][]).filter(([, , text]) => text);
  });
</script>

{#snippet sep()}<svg class="ic sep" viewBox="0 0 24 24" aria-hidden="true"><path d="m9.5 6 6 6-6 6" /></svg>{/snippet}
{#snippet errorsOnly()}<button class="btn toggle" aria-pressed={f.errors} onclick={() => (f.errors = !f.errors)}>Errors only</button>{/snippet}
{#snippet nomatch(what: string)}
  <div class="empty"><h3>No calls match</h3><p>{what}</p>
    <button class="btn" onclick={() => (f.errors = false)}>Show all calls</button></div>
{/snippet}

<!-- The way back stays in the toolbar, so a long call never scrolls it away. -->
<header data-tauri-drag-region>
  {#if group || open}
    <button class="crumb" onclick={home}>AI log</button>{@render sep()}
  {/if}
  {#if open && current}
    <button class="crumb" onclick={() => (open = null)}>{name(current)}</button>{@render sep()}
  {/if}
  <h1>{open ? label(open) : current ? name(current) : "AI log"}</h1>
  <span class="grow" data-tauri-drag-region></span>
  {#if !open && calls.length}{@render errorsOnly()}{/if}
  {#if !open && !group}
    <ClearAll count={calls.length} ask="Delete every call in the AI log? This can’t be undone." clear={() => invoke("ai_log_clear")} onclear={() => (calls = [])} />
  {/if}
</header>

<div class="content" bind:this={content}>
{#if open}
  {@const e = open}
  <div class="head">
    <span class="meta">{[when(e.at), METHOD[e.method ?? ""] ?? e.method].filter(Boolean).join(" · ")}</span>
    <span class="badge" class:bad={e.outcome !== "ok"}>{OUTCOME[e.outcome ?? ""] ?? e.outcome}</span>
  </div>
  <section class="card" aria-label="Details">
    <dl class="details">{#each details as [k, v]}<div><dt>{k}</dt><dd>{v}</dd></div>{/each}</dl>
  </section>
  {#each sections as [key, title, text, mono] (key)}
    <section class="card" aria-labelledby="sec-{key}">
      <div class="shead">
        <h2 id="sec-{key}">{title}</h2>
        <button class="btn sm" onclick={() => copy(key, text!)} aria-label="Copy {title.toLowerCase()}">{copied === key ? "Copied" : "Copy"}</button>
      </div>
      <pre class="body" class:mono class:bad={key === "error"}>{text}</pre>
    </section>
  {/each}
  <p class="err" role="status">{error}</p>
{:else if group}
  <div class="page">
    <p class="meta sub">{groupMeta}</p>
    {#if inGroup.length}
      {@render rows(inGroup)}
    {:else}
      {@render nomatch("None of these calls failed. Turn off Errors only to see them.")}
    {/if}
    <p class="err" role="status">{error}</p>
  </div>
{:else}
  <div class="page">
    {#if list.length}
      <ul class="list" aria-label="AI calls">
        {#each list as g (g.key)}
          {@const n = failed(g)}
          {@const c = g.calls[0]}
          {@const k = kind(g)}
          <li>
            <button class="row" onclick={() => pick(g)}>
              <span class="t"><span class="name">{name(g)}</span><span class="meta">{[k !== name(g) && k, when(c.at), g.calls.length > 1 ? `${g.calls.length} calls` : !k && where(c)].filter(Boolean).join(" · ")}</span></span>
              <span class="nums">{ms(span(g.calls))}</span>
              <span class="badge" class:bad={n > 0}>{g.calls.length === 1 ? (OUTCOME[c.outcome] ?? c.outcome) : n ? `${n} failed` : "OK"}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if !calls.length}
      <div class="empty"><h3>No AI calls yet</h3><p>Use a feature that calls an AI provider and it shows up here.</p>
        <button class="btn" onclick={() => invoke("settings_open", { page: "ai" })}>Open AI providers</button></div>
    {:else}
      {@render nomatch("Turn off Errors only to see the rest.")}
    {/if}
    <p class="err" role="status">{error}</p>
  </div>
{/if}
</div>

{#snippet rows(list: Summary[])}
  <ul class="list" aria-label="AI calls">
    {#each list as c (c.id)}
      <li>
        <button class="row" onclick={() => show(c.id)}>
          <span class="t"><span class="name">{label(c)}</span><span class="meta">{[when(c.at), where(c) !== usual && where(c)].filter(Boolean).join(" · ")}</span></span>
          <span class="nums">{ms(c.total_ms)}</span>
          <span class="badge" class:bad={bad(c)}>{OUTCOME[c.outcome] ?? c.outcome}</span>
        </button>
      </li>
    {/each}
  </ul>
{/snippet}

<style>
  .grow { flex: 1; align-self: stretch; }
  .toggle[aria-pressed="true"] { background: var(--accent-soft-strong); color: var(--accent-text); box-shadow: none; }
  .page { display: grid; grid-template-columns: minmax(0, 1fr); gap: var(--sp-3); align-content: start; max-width: 1000px; margin: 0 auto; }
  .meta { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .nums { color: var(--text-2); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); overflow: hidden; }
  .list li { position: relative; }
  .list li + li::before { content: ""; position: absolute; top: 0; left: var(--sp-4); right: 0; height: 1px; transform: scaleY(.5); transform-origin: top; background: var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: var(--sp-3); align-items: center; width: 100%; padding: var(--sp-3) var(--sp-4); border: 0; background: none; color: inherit; font: inherit; text-align: left; }
  .row:hover { background: var(--fill-hover); }
  .row:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; } /* inside: the list clips */
  .t { display: grid; min-width: 0; min-height: 36px; align-content: center; } /* 60px rows with the 12px padding */
  .name { font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { font-size: var(--fs-caption); border-radius: var(--r-full); padding: 0 8px; height: 20px; display: inline-flex; align-items: center; background: var(--fill-hover); color: var(--text-2); font-weight: 500; white-space: nowrap; }
  .badge.bad { background: var(--danger-soft); color: var(--danger-text); }
  .empty { padding: var(--sp-6) var(--sp-4); text-align: center; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .empty h3 { margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .empty p { margin: 0; color: var(--text-2); }
  .empty .btn { margin-top: var(--sp-3); }
  /* Breadcrumbs: the places above this one, in title2 like the page title, quieter. */
  header { min-width: 0; } /* a long name truncates instead of widening the page */
  header h1 { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .crumb { flex: 0 1 auto; min-width: 0; max-width: 35%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 0 2px; border: 0; background: none; border-radius: var(--r-sm);
    font: inherit; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); color: var(--text-3); }
  .crumb:hover { color: var(--accent-text); }
  .crumb:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .sep { flex: none; color: var(--text-3); margin: 0 -4px; }
  .ic { width: 16px; height: 16px; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: var(--sp-3); margin-bottom: var(--sp-3); }
  .sub { margin: 0; }
  .card { border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); padding: var(--sp-3) var(--sp-4); }
  .card + .card { margin-top: var(--sp-3); }
  .details { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: var(--sp-3) var(--sp-4); margin: 0; }
  dt { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  dd { margin: 0; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; user-select: text; }
  .shead { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); margin-bottom: var(--sp-2); }
  .shead h2 { margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  /* A long transcript or reply scrolls inside its own section, so the rest of the entry stays in reach. */
  .body { margin: 0; max-height: 320px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; line-height: var(--lh-body); user-select: text;
    scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  .body.mono { font-family: var(--font-mono); font-size: 12px; line-height: 18px; }
  .body.bad { color: var(--danger-text); }
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
