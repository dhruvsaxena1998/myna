<!-- One meeting (docs/index.html `detail()`, `summaryDoc()`, `regenPop()`): the Summary is the document; the transcript opens beside it. -->
<script lang="ts" module>
  import MarkdownIt from "markdown-it";

  type Ask = { template: string; prompt: string; provider: number; model: string };
  type Template = { id: string; name: string; text: string };
  // Each meeting's last run, for Try again: kept when the page is left.
  const asks = new Map<number, Ask>();

  // The model writes the Markdown: html: false escapes raw HTML, and links and images stay the text it wrote.
  const md = new MarkdownIt({ html: false }).disable(["link", "autolink", "image", "reference"]);
  const text = md.renderer.rules.text!;
  md.renderer.rules.text = (...a) =>
    text(...a).replace(/\[(\d{2}):(\d{2}):(\d{2})\]/g, (t, h, m, s) => `<button type="button" class="ts" data-ms="${((+h * 60 + +m) * 60 + +s) * 1000}" aria-label="Show ${t.slice(1, -1)} in the transcript">${t.slice(1, -1)}</button>`);
  /** The Summary as HTML, each `[hh:mm:ss]` a button that opens the transcript there. */
  const render = (src: string) => md.render(src);
</script>

<script lang="ts">
  import { convertFileSrc, invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { tick, untrack } from "svelte";
  import type { Ai, Model } from "./AiProviders.svelte";
  import HostPill from "./HostPill.svelte";
  import { changes, clock, dayHead, groups, hm, meta, writing, type CleanupRun, type Cost, type Page, type Progress, type SummaryRun, type Version } from "./meetings";

  // `onreload`: the page's lines changed in the backend (a speaker renamed), so it reads them again.
  let { page, progress, summary, cleanup, onback, onretry, onlocal, oninclude, onreload }: {
    page: Page; progress?: Progress; summary?: SummaryRun; cleanup?: CleanupRun; onback: () => void; onretry: (id: number) => void; onlocal: (id: number) => void; oninclude: (ids: number[]) => void; onreload: () => Promise<void>;
  } = $props();
  let copied = $state<"" | "doc" | "panel">("");
  let error = $state("");
  let timer: ReturnType<typeof setTimeout>;
  let sum = $state<{ versions: Version[]; showing: number | null } | null>(null);
  let ai = $state<Ai | null>(null);
  let panel = $state(false);
  let orig = $state(false); // Show original: the transcript as Myna heard it, before cleanup
  let hit = $state<number | null>(null);
  let pop = $state<"" | "vers" | "regen">("");
  let draft = $state<string | null>(null); // the Markdown being edited
  let templates = $state<Template[]>([]);
  let models = $state<Record<number, Model[]>>({});
  let form = $state({ tpl: "", prompt: "", pick: "" }); // pick: "provider id|model"
  let est = $state<number | null>(null);

  // Play: the meeting's audio, both sides mixed, loaded the first time Play is pressed.
  let audio = $state<HTMLAudioElement>();
  let src = $state("");
  let paused = $state(true);
  let at = $state(0); // where it's playing, ms
  // svelte-ignore state_referenced_locally
  let length = $state(page.duration_ms ?? 0); // the page is remade for each meeting
  let audioErr = $state("");
  async function play(from?: number) {
    if (!audio) return;
    audioErr = "";
    if (!src) {
      try {
        src = convertFileSrc(await invoke<string>("meeting_audio", { id: page.id }));
      } catch (err) {
        return void (audioErr = String(err));
      }
      await tick();
      from ??= at; // dragged before the audio loaded
    }
    if (from !== undefined) audio.currentTime = from / 1000;
    audio.play().catch((err) => (audioErr = `Couldn't play the audio: ${err}`));
  }
  // The row being said: the last one starting at or before the playhead, once it has played.
  const playing = $derived.by(() => {
    if (paused && !at) return null;
    const row = rows.findLast((g) => ("left" in g ? g.left : g.lines)[0].start_ms <= at);
    return row ? ("left" in row ? row.left : row.lines)[0].id : null;
  });
  $effect(() => {
    if (panel && playing !== null && !paused) document.getElementById(`seg-${playing}`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  });

  const date = $derived(dayHead(page.started));
  const rows = $derived(groups(page.segments));
  const cleaned = $derived(page.segments.some((l) => l.clean));
  const shown = $derived(sum?.versions.find((v) => v.id === sum!.showing));
  const busy = $derived(summary?.state === "reading" || summary?.state === "merging" || summary?.state === "writing");
  let sumModel = $state<{ provider: number; model: string } | null>(null); // the Summary model (summary::model), Main's unless the user picked one
  let sumErr = $state(""); // why there's none: "Summary model: Not set", or Main's
  const done = $derived(summary?.state === "done" ? summary : null);
  const split = (pick: string) => [+pick.slice(0, pick.indexOf("|")), pick.slice(pick.indexOf("|") + 1)] as const;
  const chosen = $derived(ai?.providers.find((p) => p.id === split(form.pick)[0]));
  // The cached list, plus the Summary model in case its list was never fetched.
  const ids = (p: Ai["providers"][number]) => [...new Set([...(models[p.id] ?? []).map((m) => m.id), ...(sumModel?.provider === p.id ? [sumModel.model] : [])])];
  const usd = (n: number) => "$" + (n >= 1 ? n.toFixed(2) : n.toPrecision(2));
  const price = (c: Cost) => (c.kind === "usd" ? usd(c.usd) : c.kind === "free" ? "Free" : "Cost unknown");
  const cost = (v: Version) => (v.edited ? "Edited" : price(v.cost));
  const when = (ms: number) => `${dayHead(ms)[0]} ${hm(ms)}`;

  async function load() {
    try {
      const r = await invoke<{ versions: Version[]; showing: number | null }>("summary_get", { id: page.id });
      if (!sum && !r.versions.length) panel = true; // no Summary yet: the transcript shows from the start
      sum = r;
    } catch (err) {
      error = String(err);
    }
  }
  const loadAi = () =>
    Promise.all([
      invoke<{ provider: number; model: string }>("summary_model").then((m) => ((sumModel = m), (sumErr = "")), (err) => ((sumModel = null), (sumErr = String(err)))),
      invoke<Ai>("ai_get").then((a) => (ai = a), () => {}),
    ]);
  const own = $derived(sumErr.startsWith("Summary model")); // the user's own pick is Not set, not Main
  // On open, and when a run finishes: the newest version shows.
  $effect(() => {
    // The new version shows at once, so the streamed text never gives way to the old one while the list loads.
    const v = done?.version;
    untrack(() => {
      if (v && sum) sum = { versions: [v, ...sum.versions.filter((x) => x.id !== v.id)], showing: v.id };
      load();
    });
  });
  // A model set in Settings shows here on coming back.
  $effect(() => {
    loadAi();
    const off = getCurrentWindow().onFocusChanged(({ payload }) => payload && loadAi());
    return () => void off.then((f) => f());
  });

  async function copy(what: "doc" | "panel") {
    error = "";
    try {
      // The backend decides what leaves the meeting: lines said while muted stay out.
      await invoke("text_copy", { text: what === "doc" ? shown!.text : await invoke("meeting_text", { id: page.id, original: orig }) });
      copied = what;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = ""), 1500);
    } catch (err) {
      error = String(err);
    }
  }
  /** Runs `f`, then shows the Summary as it is now. False when it failed. */
  async function act(f: () => Promise<unknown>) {
    error = "";
    try {
      await f();
      await load();
      return true;
    } catch (err) {
      error = String(err);
      return false;
    }
  }

  // A [hh:mm:ss] in the Summary: the transcript opens at the first line starting at or after it.
  async function jump(e: MouseEvent) {
    const ms = (e.target as Element).closest<HTMLElement>("[data-ms]")?.dataset.ms;
    if (ms === undefined) return;
    const all = (g: (typeof rows)[number]) => ("left" in g ? g.left : g.lines);
    const first = (g: (typeof rows)[number]) => all(g)[0];
    const row = rows.find((g) => all(g).at(-1)!.start_ms >= +ms) ?? rows.at(-1); // the row holding the first line at or after it
    if (!row) return;
    (panel = true), (hit = first(row).id);
    await tick();
    const el = document.getElementById(`seg-${hit}`);
    el?.scrollIntoView({ block: "center" });
    el?.focus({ preventScroll: true });
  }

  async function openRegen() {
    if (pop === "regen") return (pop = "");
    (pop = "regen"), (error = ""), (est = null);
    await loadAi();
    templates = await invoke<Template[]>("summary_templates_get").catch((err) => ((error = String(err)), []));
    const t = templates.find((t) => t.name === shown?.template) ?? templates[0];
    form = { tpl: t?.id ?? "", prompt: t?.text ?? "", pick: sumModel ? `${sumModel.provider}|${sumModel.model}` : "" };
    await Promise.all((ai?.providers ?? []).map((p) => invoke<Model[]>("ai_models", { id: p.id, fresh: false }).then((l) => { models[p.id] = l; }, () => {})));
  }
  // The estimate shows only above $1; a price that isn't known shows nothing.
  $effect(() => {
    const pick = form.pick;
    if (pop !== "regen" || !pick) return;
    est = null;
    const [provider, model] = split(pick);
    invoke<number | null>("summary_estimate", { id: page.id, provider, model }).then((u) => form.pick === pick && (est = u), () => {});
  });
  function start(a: Ask) {
    asks.set(page.id, a);
    pop = "";
    error = "";
    invoke("summary_run", { id: page.id, ...a }).catch(() => {}); // how it goes, failing included, comes as `meeting-summary`
  }
  function submit(e: SubmitEvent) {
    e.preventDefault();
    const [provider, model] = split(form.pick);
    start({ template: templates.find((t) => t.id === form.tpl)?.name ?? "", prompt: form.prompt.trim(), provider, model });
  }
  // Try again: the same template, prompt and model. A run this page didn't start (the automatic one) asks again.
  function again() {
    const a = asks.get(page.id);
    if (a) start(a);
    else openRegen();
  }
  const setup = () => invoke("settings_open", { page: own ? "meetings" : "ai" });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (await act(() => invoke("summary_edit", { id: page.id, text: draft }))) draft = null;
  }
  // A speaker's popover: `row` is the paragraph whose name was clicked, `from` the name it has now.
  let spk = $state<{ row: number; from: string; name: string; err: string } | null>(null);
  const focus = (el: HTMLInputElement) => (el.focus(), el.select());
  async function renameSpeaker(e: SubmitEvent) {
    e.preventDefault();
    if (!spk) return;
    const { from, name } = spk;
    if (name.trim() === from) return void (spk = null);
    try {
      await invoke("speaker_rename", { id: page.id, from, to: name });
      spk = null;
      await onreload();
    } catch (err) {
      if (spk) spk.err = String(err);
    }
  }

  function outside(e: PointerEvent) {
    if ((pop || spk) && !(e.target as Element).closest("[data-pop]")) (pop = ""), (spk = null);
  }
  function keys(e: KeyboardEvent) {
    if (e.key === "Escape" && (pop || spk)) (e.preventDefault(), (pop = ""), (spk = null));
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={keys} />

{#snippet icon(d: string)}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html d}</svg>{/snippet}
{#snippet copyIcon(done: boolean)}{@render icon(done ? '<path d="M5 12.5l4.5 4.5L19 7.5"/>' : '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>')}{/snippet}

<!-- No whitespace around the name: the row reads "Ana 00:10". -->
{#snippet who(l: (typeof page.segments)[number])}{#if l.speaker === "Me"}<b>Me</b>{:else}<span class="pop-wrap" data-pop><button class="name" aria-haspopup="dialog" aria-expanded={spk?.row === l.id}
  onclick={() => (spk = spk?.row === l.id ? null : { row: l.id, from: l.speaker, name: l.speaker, err: "" })}>{l.speaker}</button>{#if spk?.row === l.id}{@render speaker(spk)}{/if}</span>{/if}{/snippet}

{#snippet speaker(s: NonNullable<typeof spk>)}
  <div class="pop spk left" role="dialog" aria-label="Speaker {s.from}"><form onsubmit={renameSpeaker}>
    <label class="field"><span class="label">Name</span>
      <input class="input" maxlength="100" bind:value={s.name} use:focus aria-describedby="spk-h" /></label>
    <span class="help" id="spk-h">Renames every line of {s.from} in this meeting.</span>
    {#if s.err}<p class="err" role="status">{s.err}</p>{/if}
    <div class="end"><span class="grow"></span><button class="btn primary" disabled={!s.name.trim()}>Rename</button></div>
  </form></div>
{/snippet}

{#snippet regen(left: boolean)}
  <div class="pop regen" class:left role="dialog" aria-labelledby="rg-h"><form onsubmit={submit}>
    <h3 id="rg-h">{shown ? "Regenerate the summary" : "Summarize this meeting"}</h3>
    {#if !ai?.providers.length}
      <p class="help">Add an AI provider to summarize. Myna runs no AI itself.</p>
      <div class="end"><button type="button" class="btn" onclick={setup}>Set up</button></div>
    {:else}
      <label class="field"><span class="label">Template</span>
        <select class="select" value={form.tpl} onchange={(e) => { form.tpl = e.currentTarget.value; form.prompt = templates.find((t) => t.id === form.tpl)?.text ?? ""; }}>
          {#each templates as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
        </select></label>
      <label class="field"><span class="label">Instructions</span>
        <textarea class="input" rows="5" bind:value={form.prompt} aria-describedby="rg-ph"></textarea>
        <span class="help" id="rg-ph">Changes here are for this version only.</span></label>
      <label class="field"><span class="label">Model</span>
        <select class="select" bind:value={form.pick} aria-describedby="rg-mh">
          {#if !form.pick}<option value="" disabled>Choose a model</option>{/if}
          {#each ai.providers as p (p.id)}<optgroup label={p.name}>{#each ids(p) as m (m)}<option value="{p.id}|{m}">{m}</option>{/each}</optgroup>{/each}
        </select>
        <span class="help" id="rg-mh">Picking a model here doesn’t change your Summary model.</span></label>
      {#if est !== null && est > 1}<p class="est" role="status">About {usd(est)} for this meeting.</p>{/if}
      <div class="end">{#if chosen}<HostPill off={chosen.off_device} />{/if}<span class="grow"></span>
        <button class="btn primary" disabled={!form.pick || !form.prompt.trim()}>{shown ? "Regenerate" : "Summarize"}</button></div>
    {/if}
  </form></div>
{/snippet}

<header data-tauri-drag-region>
  <button class="btn plain" aria-label="Back to meetings" onclick={onback}>{@render icon('<path d="M15 5l-7 7 7 7"/>')}Meetings</button>
  <span class="grow"></span>
  {#if page.segments.length}
    <button class="btn" aria-pressed={panel} onclick={() => (panel = !panel)}>{@render icon('<path d="M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01"/>')}Transcript</button>
  {/if}
  {#if shown}
    <button class="btn" onclick={() => copy("doc")}>{@render copyIcon(copied === "doc")}{copied === "doc" ? "Copied" : "Copy"}</button>
  {/if}
</header>
<div class="content" class:split={panel}>
  <div class="doc">
    <article aria-labelledby="mt">
      <h1 id="mt">{page.title}</h1>
      <p class="meta">{date[1] ? `${date[1]} · ` : `${date[0]} · `}{meta(page)}</p>
      {#if page.note}<p class="note">{page.note}</p>{/if}
      {#if !writing(page)}
        <div class="player" role="group" aria-label="Meeting audio">
          <!-- svelte-ignore a11y_media_has_caption -->
          <audio bind:this={audio} {src} preload="metadata" onplay={() => (paused = false)} onpause={() => (paused = true)} onended={() => (paused = true)}
            ontimeupdate={() => (at = audio!.currentTime * 1000)} onloadedmetadata={() => isFinite(audio!.duration) && (length = audio!.duration * 1000)}></audio>
          <button class="btn icon" aria-label={paused ? "Play" : "Pause"} onclick={() => (paused ? play() : audio?.pause())}>
            {@render icon(paused ? '<path d="M8 5.5v13l10.5-6.5z"/>' : '<path d="M8 5v14M16 5v14"/>')}
          </button>
          <span class="ptime">{clock(at)}</span>
          <input type="range" aria-label="Position" min="0" max={Math.max(1, length)} step="1000" value={at}
            oninput={(e) => { at = +e.currentTarget.value; if (src && audio) audio.currentTime = at / 1000; }} />
          <span class="ptime">{clock(length)}</span>
        </div>
        {#if audioErr}<p class="err" role="status">{audioErr}</p>{/if}
      {/if}
      <p class="err" role="status">{error}</p>
      {#if page.segments.length}
        <section class="sumdoc" aria-label="Summary">
          {#if sum?.versions.length}
            <div class="sum-bar">
              {#if sum.versions.length > 1}
                <div class="pop-wrap" data-pop>
                  <button class="btn plain" aria-haspopup="menu" aria-expanded={pop === "vers"} onclick={() => (pop = pop === "vers" ? "" : "vers")}>{sum.versions.length} versions{@render icon('<path d="M7 10l5 5 5-5"/>')}</button>
                  {#if pop === "vers"}
                    <div class="pop vmenu left" role="menu" aria-label="Versions">
                      {#each sum.versions as v (v.id)}
                        <div class="vrow">
                          <button class="pk" role="menuitemradio" aria-checked={v.id === sum.showing} onclick={() => ((pop = ""), act(() => invoke("summary_pick", { id: page.id, version: v.id })))}>
                            <span class="pk-t"><span class="trunc">{v.template} · {v.model}</span>{#if v.id === sum.showing}{@render icon('<path d="M5 12.5l4.5 4.5L19 7.5"/>')}{/if}</span>
                            <span class="pk-m">{when(v.created)}{v.edited ? " · Edited" : ""}</span>
                          </button>
                          <button class="btn icon plain" aria-label="Delete the {v.template} version from {when(v.created)}" disabled={sum.versions.length === 1}
                            onclick={() => act(() => invoke("summary_delete", { id: page.id, version: v.id }))}>{@render icon('<path d="M7 7l10 10M17 7L7 17"/>')}</button>
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>
              {/if}
              <span class="grow"></span>
              <button class="btn" disabled={busy || draft !== null} onclick={() => (draft = shown?.text ?? "")}>Edit</button>
              <div class="pop-wrap" data-pop>
                <button class="btn" aria-haspopup="dialog" aria-expanded={pop === "regen"} disabled={busy || draft !== null} onclick={openRegen}>Regenerate</button>
                {#if pop === "regen"}{@render regen(false)}{/if}
              </div>
            </div>
          {/if}
          {#if summary?.state === "writing"}
            <p class="sum-prog" role="status">Writing the {asks.get(page.id)?.template ?? "Summary"}…</p>
            <div class="sum" aria-busy="true">{@html render(summary.text)}</div>
          {:else if busy}
            <p class="sum-prog" role="status">{summary?.state === "reading" ? `Reading part ${summary.n} of ${summary.of}…` : `Writing the ${asks.get(page.id)?.template ?? "Summary"}…`}</p>
          {:else if summary?.state === "failed"}
            <p class="sum-prog" role="status"><span class="fail">Couldn’t summarize: {summary.error}</span> <button class="btn small" onclick={again}>Try again</button></p>
          {/if}
          {#if draft !== null}
            <form class="sum-edit" onsubmit={save}>
              <textarea class="input" aria-label="Summary text, in Markdown" bind:value={draft}></textarea>
              <div class="end"><span class="help grow">Saving adds an Edited version. Regenerate never overwrites it.</span>
                <button type="button" class="btn" onclick={() => (draft = null)}>Cancel</button><button class="btn primary">Save</button></div>
            </form>
          {:else if shown && summary?.state !== "writing"}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the timestamps are buttons; this only hears their clicks) -->
            <div class="sum" onclick={jump}>{@html render(shown.text)}</div>
            <p class="sum-cost">{[cost(shown), shown.template, shown.model, `Summarized in ${shown.parts} part${shown.parts === 1 ? "" : "s"}`].join(" · ")}</p>
          {:else if sum && ai && !busy}
            <div class="sum-empty">
              {#if sumModel}
                <h2>No summary yet</h2>
                <div class="pop-wrap" data-pop>
                  <button class="btn primary" aria-haspopup="dialog" aria-expanded={pop === "regen"} onclick={openRegen}>Summarize</button>
                  {#if pop === "regen"}{@render regen(true)}{/if}
                </div>
              {:else}
                <h2>Set a {own ? "Summary" : "Main"} model to summarize</h2>
                <p class="help">Summaries are written by an AI model you choose. Myna runs none itself.</p>
                <button class="btn" onclick={setup}>Set up</button>
              {/if}
            </div>
          {/if}
        </section>
      {:else if page.state === "failed"}
        <p class="say fail">{page.failure ?? "Myna couldn't write up this meeting."}</p>
        <button class="btn" onclick={() => onretry(page.id)}>Try again</button>
        {#if page.incomplete}<button class="btn" onclick={() => onlocal(page.id)}>Transcribe locally</button>{/if}
      {:else if writing(page)}
        <p class="say" role="status">Writing up your notes… {Math.round((progress?.progress ?? 0) * 100)}%{#if page.stt} · {page.stt.model} on {page.stt.provider} <HostPill off={page.stt.off} what="audio" />{/if}</p>
      {:else}
        <p class="say">Nobody said anything Myna could hear in this meeting.</p>
      {/if}
    </article>
  </div>
  {#if panel}
    <aside class="panel" aria-labelledby="tp-h">
      <div class="panel-head">
        <h2 id="tp-h">Transcript</h2><span class="grow"></span>
        {#if cleaned}<button class="btn" aria-pressed={orig} onclick={() => (orig = !orig)}>{orig ? "Show cleaned" : "Show original"}</button>{/if}
        <button class="btn" onclick={() => copy("panel")}>{@render copyIcon(copied === "panel")}{copied === "panel" ? "Copied" : "Copy"}</button>
        <button class="btn icon plain" aria-label="Close transcript" onclick={() => (panel = false)}>{@render icon('<path d="M7 7l10 10M17 7L7 17"/>')}</button>
      </div>
      <div class="transcript">
        {#if cleanup?.state === "reading"}<p class="tmeta" role="status">Cleaning up the transcript: part {cleanup.n} of {cleanup.of}…</p>
        {:else if cleanup?.state === "failed"}<p class="tmeta fail" role="status">Couldn’t clean up the transcript: {cleanup.error}</p>{/if}
        {#if page.stt?.cost}<p class="tmeta">Transcribed by {page.stt.model} · {page.stt.provider} · {price(page.stt.cost)}</p>{/if}
        {#if cleaned}<p class="tmeta">{orig ? "What Myna heard, before cleanup." : `Cleaned up by ${page.cleanup?.model ?? "AI"}${page.cleanup ? ` · ${price(page.cleanup.cost)}` : ""}. Point at an underlined word to see what Myna heard.`}</p>{/if}
        {#each rows as g}
          {#if "left" in g}
            <div class="seg" class:hit={hit === g.left[0].id} class:playing={playing === g.left[0].id} id="seg-{g.left[0].id}" tabindex="-1"><div class="who"><b>Me</b><button class="at" aria-label="Play from {clock(g.left[0].start_ms)}" onclick={() => play(g.left[0].start_ms)}><time>{clock(g.left[0].start_ms)}</time></button></div>
              <p class="left"><span>Said while muted · left out</span><button class="btn small" onclick={() => oninclude(g.left.map((l) => l.id))}>Include</button></p></div>
          {:else}
            {@const l = g.lines[0]}
            <div class="seg" class:hit={hit === l.id} class:playing={playing === l.id} id="seg-{l.id}" tabindex="-1"><div class="who">{@render who(l)}<button class="at" aria-label="Play from {clock(l.start_ms)}" onclick={() => play(l.start_ms)}><time>{clock(l.start_ms)}</time></button></div>
              <p>{#each g.lines as line, n}{n ? " " : ""}{#if line.clean && !orig}{#each changes(line.text, line.clean) as c, i}{i ? " " : ""}{#if c.heard === undefined}{c.text}{:else}<mark class="chg" title={c.heard ? `Heard “${c.heard}”` : "Added in cleanup"}>{c.text}</mark>{/if}{/each}{:else}{line.text}{/if}{/each}{#if l.included}<span class="mark">Said while muted</span>{/if}</p></div>
          {/if}
        {/each}
      </div>
    </aside>
  {/if}
</div>

<style>
  .grow { flex: 1; }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .btn { display: inline-flex; align-items: center; justify-content: center; gap: var(--sp-1h); height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong); color: var(--text); font: inherit; }
  .btn:hover:not(:disabled) { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:disabled { color: var(--text-disabled); }
  .btn.plain { background: none; box-shadow: none; padding: 0 var(--sp-2); }
  .btn.plain:hover:not(:disabled) { background: var(--fill-hover); }
  .btn.icon { width: var(--h-ctl); padding: 0; }
  .btn.primary { background: var(--accent); color: var(--on-accent); font-weight: var(--fw-medium); box-shadow: var(--shadow-solid); }
  .btn.primary:hover:not(:disabled) { background: var(--accent-hover); }
  .btn.primary:disabled { background: var(--fill-control); color: var(--text-disabled); box-shadow: inset 0 0 0 .5px var(--line-strong); }
  .btn[aria-pressed="true"] { background: var(--accent-soft-strong); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* The transcript beside the document: each scrolls on its own. */
  .content.split { display: grid; grid-template-columns: minmax(0, 1fr) minmax(280px, 400px); grid-template-rows: minmax(0, 1fr); gap: var(--sp-4); overflow: hidden; }
  .split .doc { min-height: 0; overflow: auto; scrollbar-width: thin; }
  .panel { min-height: 0; overflow: auto; scrollbar-width: thin; padding: 0 var(--sp-4) var(--sp-4); border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); user-select: text; }
  .panel-head { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; gap: var(--sp-2); padding: var(--sp-3) 0 var(--sp-2); background: var(--bg-inset); }
  .panel-head h2 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  article { max-width: var(--w-doc, 720px); margin: 0 auto; user-select: text; }
  h1 { margin: 0; font-size: var(--fs-title1); line-height: 1.2; font-weight: var(--fw-bold, 700); }
  .meta { margin: var(--sp-1) 0 0; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .note { margin: var(--sp-1) 0 0; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .err { margin: var(--sp-1) 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .help { margin: 0; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .seg { display: grid; grid-template-columns: minmax(0, 1fr); gap: 2px; padding: var(--sp-2) 0; }
  .seg.hit { margin: 0 calc(-1 * var(--sp-2)); padding-left: var(--sp-2); padding-right: var(--sp-2); border-radius: var(--r-md); background: var(--accent-soft); }
  .seg:focus { outline: none; }
  .who { display: flex; align-items: baseline; gap: var(--sp-2); }
  .who b { font-weight: var(--fw-semibold); }
  .name { padding: 0 var(--sp-1); margin: 0 calc(-1 * var(--sp-1)); border: 0; border-radius: var(--r-sm); background: none; color: var(--text); font: inherit; font-weight: var(--fw-semibold); }
  .name:hover, .name[aria-expanded="true"] { background: var(--fill-hover); }
  .name:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  .spk { width: 260px; padding: var(--sp-3); font-weight: normal; }
  .spk form { display: grid; gap: var(--sp-2); }
  .spk .input { width: 100%; box-sizing: border-box; }
  .who time { color: var(--text-3); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .at { padding: 0 var(--sp-1); margin: 0 calc(-1 * var(--sp-1)); border: 0; border-radius: var(--r-sm); background: none; font: inherit; }
  .at:hover { background: var(--fill-hover); }
  .at:hover time { color: var(--accent-text); }
  .at:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  .seg.playing { margin: 0 calc(-1 * var(--sp-2)); padding-left: var(--sp-2); padding-right: var(--sp-2); border-radius: var(--r-md); background: var(--fill-hover); box-shadow: inset 2px 0 0 var(--accent); }
  .player { display: flex; align-items: center; gap: var(--sp-2); margin-top: var(--sp-3); max-width: 520px; }
  .player input { flex: 1; min-width: 0; accent-color: var(--accent); }
  .ptime { color: var(--text-3); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .seg p { margin: 0; font-size: var(--fs-reading); line-height: var(--lh-reading); overflow-wrap: anywhere; }
  .left { display: flex; align-items: center; gap: var(--sp-2); color: var(--text-3); font-size: var(--fs-footnote); }
  .btn.small { height: 24px; padding: 0 10px; font-size: var(--fs-footnote); }
  .tmeta { margin: 0 0 var(--sp-2); color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .chg { background: none; color: inherit; text-decoration: underline dotted var(--accent-text); text-decoration-thickness: 1.5px; text-underline-offset: 3px; cursor: help; }
  .mark { display: block; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .say { margin: var(--sp-4) 0 var(--sp-3); color: var(--text-2); }
  .say.fail, .fail { color: var(--danger-text); }
  .sumdoc { margin-top: var(--sp-3); }
  .sum-bar, .end { display: flex; align-items: center; gap: var(--sp-2); }
  .sum-prog { margin: var(--sp-4) 0 0; color: var(--text-2); font-size: var(--fs-footnote); }
  .sum { margin-top: var(--sp-3); font-size: var(--fs-reading); line-height: var(--lh-reading); overflow-wrap: anywhere; }
  .sum :global(:is(h1, h2, h3, h4)) { margin: var(--sp-5) 0 var(--sp-2); font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .sum :global(p) { margin: 0 0 var(--sp-2); }
  .sum :global(:is(ul, ol)) { margin: 0 0 var(--sp-2); padding-left: 20px; }
  .sum :global(.ts) { padding: 0 2px; border: 0; border-radius: var(--r-xs); background: none; color: var(--accent-text); font: inherit; font-size: .88em; font-variant-numeric: tabular-nums; }
  .sum :global(.ts:hover) { text-decoration: underline; }
  .sum-cost { margin: var(--sp-5) 0 0; padding-top: var(--sp-2); border-top: .5px solid var(--line); color: var(--text-3); font-size: var(--fs-footnote); font-variant-numeric: tabular-nums; }
  .sum-edit { margin-top: var(--sp-4); display: grid; gap: var(--sp-2); }
  .sum-edit textarea { min-height: 300px; font: 13px/1.55 var(--font-mono); resize: vertical; }
  .sum-empty { margin-top: var(--sp-4); padding: var(--sp-5); border-radius: var(--r-xl); background: var(--bg-inset); display: grid; gap: var(--sp-2); justify-items: start; }
  .sum-empty h2 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .pop-wrap { position: relative; }
  .pop { position: absolute; z-index: 10; right: 0; top: calc(100% + var(--sp-1)); box-sizing: border-box; padding: 5px; border-radius: var(--r-lg); background: var(--mat-menu);
    -webkit-backdrop-filter: var(--blur); backdrop-filter: var(--blur); box-shadow: inset 0 0 0 .5px var(--edge), var(--shadow-menu); animation: rise var(--dur) var(--ease-out); user-select: none; }
  .pop.left { left: 0; right: auto; }
  /* DESIGN.md › Motion: popovers enter with an 8px rise and a fade. */
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } }
  @media (prefers-reduced-motion: reduce) { .pop { animation: none; } }
  .regen { width: min(440px, calc(100vw - 96px)); padding: var(--sp-4); display: grid; gap: var(--sp-3); }
  .regen form { display: contents; }
  .regen h3 { margin: 0; font-size: var(--fs-body); font-weight: var(--fw-semibold); }
  .field { display: grid; gap: var(--sp-1); }
  .label { font-weight: var(--fw-medium); }
  .field textarea { resize: vertical; }
  .est { margin: 0; color: var(--caution-text); font-size: var(--fs-footnote); }
  .vmenu { width: 340px; }
  .vrow { display: flex; align-items: center; gap: 2px; }
  .pk { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: stretch; padding: 4px 10px; border: 0; border-radius: var(--r-sm); background: none; color: var(--text); font: inherit; text-align: left; }
  .pk:hover, .pk:focus-visible { background: var(--accent); color: var(--on-accent); outline: none; }
  .pk:hover .pk-m, .pk:focus-visible .pk-m { color: inherit; }
  .pk-t { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); }
  .trunc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pk-m { font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3); font-variant-numeric: tabular-nums; }
</style>
