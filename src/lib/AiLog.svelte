<!-- AI log (experimental): every call Myna made to an AI provider, newest first, and one opened with what was sent
     and what came back. Laid out like Stats for nerds (Traces.svelte). The backend half is #247. -->
<script module lang="ts">
  import type { Cost } from "./meetings";
  export type Outcome = "ok" | "error" | "timeout" | "refused" | "blocked";
  export type Summary = { id: string; at: number; feature: string; step?: string | null; provider?: string | null; kind?: string | null; model?: string | null;
    method: string; outcome: Outcome; total_ms?: number | null; cost?: Cost | null; preview?: string | null };
  type Decision = { question?: string | null; options?: string[] | null; instructions?: string | null; via?: string | null; choice?: string | null; confidence?: number | null };
  export type Entry = Omit<Summary, "method" | "outcome" | "preview"> & { method?: string | null; outcome?: Outcome | null;
    system?: string | null; prompt?: string | null; decision?: Decision | null; text?: string | null; raw?: string | null; raw_cut?: boolean | null; error?: string | null;
    ttft_ms?: number | null; limit_s?: number | null; tokens_in?: number | null; tokens_out?: number | null; audio_ms?: number | null; segments?: unknown };
  const OUTCOME: Record<string, string> = { ok: "OK", error: "Error", timeout: "Timed out", refused: "Refused", blocked: "Blocked" };
  // Filters last as long as the window is open, like Stats for nerds'.
  const f = $state({ feature: "all", errors: false });
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ClearAll from "./ClearAll.svelte";
  import { dur, when } from "./trace";

  let calls = $state<Summary[]>([]);
  let open = $state<Entry | null>(null);
  let error = $state("");
  let copied = $state("");
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

  const features = $derived([...new Set(calls.map((c) => c.feature))]);
  const list = $derived(calls.filter((c) => (f.feature === "all" || c.feature === f.feature) && (!f.errors || c.outcome !== "ok")));
  const label = (c: Pick<Summary, "feature" | "step">) => [c.feature, c.step].filter(Boolean).join(" · ");
  const usd = (n: number) => "$" + (n >= 0.01 || n === 0 ? n.toFixed(2) : n.toPrecision(2));
  const price = (c: Cost | null | undefined, kind?: string | null) =>
    !c ? "—" : c.kind === "usd" ? usd(c.usd) : c.kind === "free" ? "Free" : kind === "agent" ? "On your plan" : "Cost unknown";
  const ms = (n: number | null | undefined) => (n == null ? "—" : dur(n));
  // A JSON reply reads better indented; anything else (SSE lines, stream-json, a cut body) is shown as it came.
  const pretty = (raw: string) => { try { return JSON.stringify(JSON.parse(raw), null, 2); } catch { return raw; } };

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
  const sections = $derived.by(() => {
    if (!open) return [];
    const e = open, d = e.decision;
    const decision = d && [d.question && `Question: ${d.question}`, d.options?.length && `Options: ${d.options.join(", ")}`, d.instructions && `Instructions: ${d.instructions}`,
      d.via && `Via: ${d.via}`, d.choice && `Choice: ${d.choice}${d.confidence != null ? ` (${Math.round(d.confidence * 100)}% sure)` : ""}`].filter(Boolean).join("\n");
    const segments = e.segments == null ? "" : typeof e.segments === "string" ? e.segments : JSON.stringify(e.segments, null, 2);
    return ([
      ["system", "System prompt", e.system, false], ["prompt", "Prompt", e.prompt, false], ["decision", "Decision details", decision, false],
      ["text", "Text", e.text, false], ["segments", "Segments", segments, true], ["raw", e.raw_cut ? "Raw reply (cut at 256 KB)" : "Raw reply", e.raw && pretty(e.raw), true], ["error", "Error", e.error, false],
    ] as [string, string, string | null | undefined, boolean][]).filter(([, , text]) => text);
  });
</script>

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}

<header data-tauri-drag-region>
  <h1>AI log</h1>
  {#if !open}
    <span class="grow" data-tauri-drag-region></span>
    <select class="select" aria-label="Feature" bind:value={f.feature}>
      <option value="all">All features</option>
      {#each features as x}<option value={x}>{x}</option>{/each}
      {#if f.feature !== "all" && !features.includes(f.feature)}<option value={f.feature}>{f.feature}</option>{/if}
    </select>
    <button class="btn toggle" aria-pressed={f.errors} onclick={() => (f.errors = !f.errors)}>Errors only</button>
    <ClearAll count={calls.length} ask="Delete every call in the AI log? This can’t be undone." clear={() => invoke("ai_log_clear")} onclear={() => { calls = []; f.feature = "all"; }} />
  {/if}
</header>

<div class="content">
{#if open}
  {@const e = open}
  <div class="top">
    <button class="back" onclick={() => (open = null)}>{@render icon('<path d="M14.5 6 8.5 12l6 6" />')}All calls</button>
  </div>
  <div class="head">
    <div>
      <h2>{label(e)}</h2>
      <div class="meta">{[when(e.at), e.method].filter(Boolean).join(" · ")}</div>
    </div>
    <span class="badge" class:bad={e.outcome !== "ok"}>{OUTCOME[e.outcome ?? ""] ?? e.outcome}</span>
  </div>
  <p class="err" role="status">{error}</p>
  <section class="card" aria-label="Details">
    <dl class="details">{#each details as [k, v]}<div><dt>{k}</dt><dd>{v}</dd></div>{/each}</dl>
  </section>
  {#each sections as [key, name, text, mono] (key)}
    <section class="card" aria-labelledby="sec-{key}">
      <div class="shead">
        <h3 id="sec-{key}">{name}</h3>
        <button class="btn sm" onclick={() => copy(key, text!)} aria-label="Copy {name.toLowerCase()}">{copied === key ? "Copied" : "Copy"}</button>
      </div>
      <pre class="body" class:mono class:bad={key === "error"}>{text}</pre>
    </section>
  {/each}
{:else}
  <div class="page">
    {#if list.length}
      <ul class="list" aria-label="AI calls">
        {#each list as c (c.id)}
          <li>
            <button class="row" onclick={() => show(c.id)}>
              <span class="t"><span class="name">{label(c)}</span><span class="meta">{[when(c.at), [c.provider, c.model].filter(Boolean).join(" · ")].filter(Boolean).join(" · ")}</span></span>
              <span class="nums">{ms(c.total_ms)}</span>
              <span class="badge" class:bad={c.outcome !== "ok"}>{OUTCOME[c.outcome] ?? c.outcome}</span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if !calls.length}
      <div class="empty"><h3>No AI calls yet</h3><p>Use a feature that calls an AI provider and it shows up here.</p></div>
    {:else}
      <div class="empty"><h3>No calls match</h3><p>Pick All features or turn off Errors only to see the rest.</p>
        <button class="btn" onclick={() => { f.feature = "all"; f.errors = false; }}>Show all calls</button></div>
    {/if}
    <p class="err" role="status">{error}</p>
  </div>
{/if}
</div>

<style>
  .grow { flex: 1; align-self: stretch; }
  .select { max-width: 200px; }
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
  .t { display: grid; min-width: 0; }
  .name { font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { font-size: var(--fs-caption); border-radius: var(--r-full); padding: 0 8px; height: 20px; display: inline-flex; align-items: center; background: var(--fill-hover); color: var(--text-2); font-weight: 500; white-space: nowrap; }
  .badge.bad { background: var(--danger-soft); color: var(--danger-text); }
  .empty { padding: var(--sp-6) var(--sp-4); text-align: center; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .empty h3 { margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .empty p { margin: 0; color: var(--text-2); }
  .empty .btn { margin-top: var(--sp-3); }
  .top { margin-bottom: var(--sp-2); }
  .back { display: inline-flex; align-items: center; gap: 2px; min-height: 24px; margin-left: -4px; padding: 0 4px 0 0; border: 0; background: none; font: inherit; color: var(--accent-text); border-radius: var(--r-sm); }
  .back:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .ic { width: 16px; height: 16px; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .head { display: flex; justify-content: space-between; align-items: flex-start; gap: var(--sp-3); }
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .card { border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); padding: var(--sp-3) var(--sp-4); }
  .card + .card { margin-top: var(--sp-3); }
  .details { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: var(--sp-3) var(--sp-4); margin: 0; }
  dt { color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  dd { margin: 0; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; user-select: text; }
  .shead { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); margin-bottom: var(--sp-2); }
  h3 { margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  /* A long transcript or reply scrolls inside its own section, so the rest of the entry stays in reach. */
  .body { margin: 0; max-height: 320px; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; line-height: var(--lh-body); user-select: text;
    scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  .body.mono { font-family: var(--font-mono); font-size: 12px; line-height: 18px; }
  .body.bad { color: var(--danger-text); }
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
