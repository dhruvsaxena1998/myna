<!-- Settings › AI providers (`part="page"`) and its Add a provider drill-in (`"add"`): docs/settings.html, VIEWS.ai and VIEWS.add. One `ai`, held by the shell, so the page, the drill-in and the other pages agree. -->
<script lang="ts" module>
  export type Provider = { id: number; kind: "openai" | "openrouter"; name: string; url: string; has_key: boolean; off_device: boolean };
  export type Route = { provider: number; model: string; gone: boolean } | null;
  export type Ai = { providers: Provider[]; main: Route; fast: Route };
  type Model = { id: string; input_per_m: number | null; output_per_m: number | null; context: number | null; images: boolean | null };
  type Test = { st: "run" } | { st: "err"; msg: string } | { st: "ok"; ms: number; models: number | null; used: number | null; limit: number | null };
  type Slot = "main" | "fast";

  // [kind, label, name, address]
  const PRESETS: [Provider["kind"], string, string, string][] = [
    ["openai", "OpenAI-compatible", "", ""],
    ["openrouter", "OpenRouter", "OpenRouter", "https://openrouter.ai/api/v1"],
  ];
  const SLOTS: [Slot, string, string][] = [
    ["main", "Main model", "Writes summaries and answers your questions."],
    ["fast", "Fast model", "Quick jobs like tidying up text and small decisions."],
  ];
  const hostOf = (u: string) => { try { return new URL(u).hostname.replace(/^\[|\]$/g, ""); } catch { return ""; } };
  const loopback = (u: string) => /^(localhost|127(\.\d{1,3}){3}|::1)$/.test(hostOf(u));
  const usd = (n: number) => "$" + (n >= 0.01 || n === 0 ? n.toFixed(2) : n.toPrecision(2));
  const ctx = (n: number) => (n >= 1e6 ? `${+(n / 1e6).toFixed(1)}M` : `${Math.round(n / 1000)}K`);
  export const routeUses = (ai: Ai, id: number) => [ai.main, ai.fast].some((r) => r?.provider === id);
  const testText = (t: Extract<Test, { st: "ok" }>) => {
    const s = `${(t.ms / 1000).toFixed(1)} s`;
    if (t.models != null) return `Works · ${t.models} model${t.models === 1 ? "" : "s"} · ${s}`;
    return `Key works · ${usd(t.used ?? 0)} used${t.limit != null ? ` of ${usd(t.limit)}` : ""} · ${s}`;
  };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import HostPill from "./HostPill.svelte";

  // `open`: a drill-in under AI providers. `back`: leaves one.
  let { ai = $bindable(), part, open: openSub = async () => {}, back = async () => {} }: {
    ai: Ai; part: "page" | "add"; open?: (sub: string) => Promise<void> | void; back?: () => Promise<void> | void;
  } = $props();
  const providers = $derived(ai.providers);
  const routes = $derived({ main: ai.main, fast: ai.fast });
  let tests = $state<Record<number, Test>>({});
  let note = $state(""); // what a removal did, neutral
  let error = $state(""); // a removal that failed
  let pickError = $state(""); // a model pick that failed
  let confirm = $state<number | null>(null);
  let draft = $state({ kind: "openai" as Provider["kind"], name: "", url: "", key: "" });
  let errs = $state<{ name?: string; url?: string; key?: string }>({});
  let addError = $state("");
  let dtest = $state<Test | null>(null); // the draft's test; typing clears it
  let busy = $state(false);
  let picker = $state<Slot | null>(null);
  let q = $state("");
  let lists = $state<Record<number, { st: "load" | "ok" | "err"; models: Model[]; msg: string; stale: string }>>({});
  let search = $state<HTMLInputElement>();
  const triggers: Partial<Record<Slot, HTMLButtonElement>> = {};

  const prov = (id: number) => providers.find((p) => p.id === id);
  // A list fetched while Settings is open beats the backend's flag from the saved list.
  const gone = (r: Route) => !!r && (lists[r.provider]?.st === "ok" ? !lists[r.provider].models.some((m) => m.id === r.model) : r.gone);
  const routeName = (r: Route) => (r ? `${prov(r.provider)?.name ?? "Unknown provider"} · ${r.model}` : "Not set");
  const none = $derived(!providers.length);

  function focusNow(n: HTMLElement) { n.focus(); }

  // --- Add form
  function pickKind(e: Event) {
    const kind = (e.currentTarget as HTMLSelectElement).value as Provider["kind"];
    const [, , name, url] = PRESETS.find((p) => p[0] === kind)!;
    draft = { ...draft, kind, name, url };
    errs = {};
    dtest = null;
  }
  function validate() {
    const e: typeof errs = {};
    const name = draft.name.trim();
    if (!name) e.name = "Give it a name.";
    else if (providers.some((p) => p.name.toLowerCase() === name.toLowerCase())) e.name = "You already have one with this name.";
    let proto = "";
    try { proto = new URL(draft.url).protocol; } catch { /* not a URL */ }
    if (!/^https?:$/.test(proto) || !hostOf(draft.url)) e.url = "Enter the full address, starting with http:// or https://.";
    const key = draft.key.trim();
    if (draft.kind === "openrouter" && !key) e.key = "Paste your OpenRouter key.";
    else if (draft.kind === "openrouter" && !key.startsWith("sk-or-")) e.key = "OpenRouter keys start with “sk-or-”. Make sure you copied all of it.";
    errs = e;
    return !Object.keys(e).length;
  }
  async function add(e: SubmitEvent) {
    e.preventDefault();
    addError = "";
    if (!validate() || busy) return;
    busy = true;
    try {
      const p = await invoke<Provider>("ai_add", { kind: draft.kind, name: draft.name.trim(), url: draft.url.trim(), key: draft.key.trim() });
      ai = { ...ai, providers: [...ai.providers, p] };
      invoke("ai_models", { id: p.id, fresh: true }).catch(() => {}); // warm the cache so the first picker open is instant
      await back();
    } catch (err) {
      addError = String(err);
    } finally {
      busy = false;
    }
  }

  // Tests the address and key as typed, before anything is saved.
  async function testDraft() {
    addError = "";
    validate();
    errs = { ...errs, name: undefined }; // a name isn't needed to test
    if (errs.url || errs.key) return;
    dtest = { st: "run" };
    const t = await invoke<{ ms: number; models: number | null; used: number | null; limit: number | null }>("ai_test_draft", { kind: draft.kind, url: draft.url, key: draft.key }).then(
      (t): Test => ({ st: "ok", ...t }),
      (err): Test => ({ st: "err", msg: String(err) }),
    );
    if (dtest?.st === "run") dtest = t; // unless typing cleared it
  }

  // --- Remove
  async function remove(p: Provider) {
    error = ""; note = "";
    try {
      const gone = await invoke<string[]>("ai_remove", { id: p.id });
      const names = gone.map((s) => (s === "main" ? "Main model" : "Fast model"));
      ai = { providers: ai.providers.filter((x) => x.id !== p.id), main: gone.includes("main") ? null : ai.main, fast: gone.includes("fast") ? null : ai.fast };
      delete tests[p.id];
      confirm = null;
      note = `Removed ${p.name} and its key.` + (names.length ? ` ${names.join(" and ")} ${names.length > 1 ? "are" : "is"} now Not set.` : "");
    } catch (err) {
      error = String(err);
    }
  }

  // --- Test
  async function test(p: Provider) {
    note = "";
    tests[p.id] = { st: "run" };
    try {
      const t = await invoke<{ ms: number; models: number | null; used: number | null; limit: number | null }>("ai_test", { id: p.id });
      if (prov(p.id)) tests[p.id] = { st: "ok", ...t };
    } catch (err) {
      if (prov(p.id)) tests[p.id] = { st: "err", msg: String(err) };
    }
  }

  // --- Picker
  async function openPicker(slot: Slot) {
    if (picker === slot) return close();
    picker = slot; q = "";
    for (const p of providers) {
      if (lists[p.id]?.st !== "ok") lists[p.id] = { st: "load", models: [], msg: "", stale: "" }; // a list already shown stays put
      // the saved list first, then the live one swaps in; a failed refresh keeps what is shown
      invoke<Model[]>("ai_models", { id: p.id, fresh: false }).then(
        (models) => {
          lists[p.id] = { st: "ok", models, msg: "", stale: lists[p.id]?.stale ?? "" };
          invoke<Model[]>("ai_models", { id: p.id, fresh: true }).then(
            (models) => { lists[p.id] = { st: "ok", models, msg: "", stale: "" }; },
            (err) => { lists[p.id].stale = String(err); },
          );
        },
        (err) => { lists[p.id] = { st: "err", models: [], msg: String(err), stale: "" }; },
      );
    }
    await tick();
    search?.focus({ preventScroll: true });
    search?.closest(".pop")?.scrollIntoView({ block: "nearest" }); // the page scrolls, so a picker low on it would open out of view
  }
  function close(refocus = true) {
    const slot = picker;
    picker = null;
    if (refocus && slot) triggers[slot]?.focus();
  }
  async function choose(slot: Slot, provider: number, model: string) {
    pickError = "";
    const was = ai[slot];
    ai = { ...ai, [slot]: { provider, model, gone: false } };
    close();
    try {
      await invoke("ai_set", { slot, provider, model });
    } catch (err) {
      ai = { ...ai, [slot]: was };
      pickError = String(err);
    }
  }
  function matches(p: Provider, m: Model) {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    const hay = `${p.name} ${m.id}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  }
  function meta(p: Provider, m: Model) {
    if (p.kind === "openrouter") {
      const parts = [];
      if (m.input_per_m != null && m.output_per_m != null) parts.push(`${usd(m.input_per_m)} in · ${usd(m.output_per_m)} out per 1M tokens`);
      if (m.context != null) parts.push(`${ctx(m.context)} context`);
      if (m.images) parts.push("reads images");
      return parts.join(" · ") || "Cost unknown";
    }
    return p.off_device ? "Cost unknown" : "Free · runs on your server";
  }
  function popKeys(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); return close(); }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const els = [search!, ...(e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>("[role=menuitemradio]:not(:disabled)")];
    const i = Math.max(0, els.indexOf(document.activeElement as HTMLElement));
    els[Math.min(els.length - 1, Math.max(0, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
    e.preventDefault();
  }
  function outside(e: PointerEvent) {
    if (picker && !(e.target as Element).closest("[data-pop]")) close(false);
  }
</script>

<svelte:window onpointerdown={outside} />

{#snippet pill(off: boolean)}<HostPill {off} />{/snippet}

{#if part === "add"}
  {@const or = draft.kind === "openrouter"}
  {@const msg = addError || (dtest?.st === "err" ? dtest.msg : "")}
  <form onsubmit={add} novalidate aria-label="Add a provider">
    <section>
      <ul class="list">
        <li class="field"><label for="add-kind">Kind</label>
          <select id="add-kind" class="select" value={draft.kind} onchange={pickKind} aria-describedby="add-kind-h">{#each PRESETS as [v, l]}<option value={v}>{l}</option>{/each}</select>
          <span class="help" id="add-kind-h">{or ? "One key for models from many companies." : "Any server that speaks the OpenAI API."}</span></li>
        <li class="field"><label for="add-name">Name</label>
          <input id="add-name" class="input" bind:value={draft.name} maxlength="40" placeholder={or ? "" : "Server on this Mac"} autocomplete="off" spellcheck="false" oninput={() => (errs = { ...errs, name: undefined })} aria-invalid={!!errs.name} aria-describedby="add-name-h" />
          <span class="help" class:bad={errs.name} id="add-name-h" role="status">{errs.name ?? "Shown when you pick a model."}</span></li>
        <li class="field"><label for="add-url">Address</label>
          <input id="add-url" class="input mono" type="url" bind:value={draft.url} placeholder={or ? "" : "http://localhost:11434/v1"} autocomplete="off" spellcheck="false" oninput={() => ((errs = { ...errs, url: undefined }), (dtest = null))} aria-invalid={!!errs.url} aria-describedby="add-url-h" />
          <span class="help" class:bad={errs.url} id="add-url-h" role="status">{#if errs.url}{errs.url}{:else if hostOf(draft.url)}{@render pill(!loopback(draft.url))}{:else}{or ? "OpenRouter’s address. Leave it as it is." : "Where the server listens, ending in /v1."}{/if}</span></li>
        <li class="field"><label for="add-key">{or ? "Key" : "Key (optional)"}</label>
          <input id="add-key" class="input mono" type="password" bind:value={draft.key} autocomplete="off" spellcheck="false" placeholder={or ? "sk-or-…" : ""} oninput={() => ((errs = { ...errs, key: undefined }), (dtest = null))} aria-invalid={!!errs.key} aria-describedby="add-key-h" />
          <span class="help" class:bad={errs.key} id="add-key-h" role="status">{errs.key ?? `${or ? "Find it at openrouter.ai/keys." : "Only if your server asks for one."} Kept in your Mac’s Keychain.`}</span></li>
      </ul>
    </section>
    <div class="actions">
      <p class="err" class:neutral={!msg} role="status" title={msg || undefined}>{msg || (dtest?.st === "run" ? "Checking…" : dtest?.st === "ok" ? testText(dtest) : "")}</p>
      <button type="button" class="btn" disabled={dtest?.st === "run"} onclick={testDraft}>Test</button>
      <button type="button" class="btn" onclick={() => back()}>Cancel</button>
      <button class="btn primary" disabled={busy}>Add provider</button>
    </div>
  </form>
{:else}
  <section aria-labelledby="prov-h">
    <h2 id="prov-h">Providers</h2>
    <p class="help">Servers on this Mac keep your words on it. Testing is free.</p>
    <ul class="list">
      {#each providers as p (p.id)}
        {@const t = tests[p.id]}
        <li class="row prov">
          {#if confirm === p.id}
            {@const uses = routeUses(ai, p.id)}
            {@const text = uses ? "Models that use it become Not set." : "Nothing uses it right now."}
            <span role="alertdialog" aria-labelledby="rm-t"><span class="name" id="rm-t">Remove {p.name} and its key?</span><span class="desc" title={text}>{text}</span></span>
            <span class="end">
              <button type="button" class="btn" use:focusNow onclick={async () => { confirm = null; await tick(); document.getElementById(`rm-${p.id}`)?.focus(); }}>Cancel</button>
              <button type="button" class="btn danger" onclick={() => remove(p)}>Remove</button>
            </span>
          {:else}
            {@const addr = p.url + (p.has_key ? " · Key in Keychain" : "")}
            <span>
              <span class="name titled"><span class="trunc">{p.name}</span>{@render pill(p.off_device)}</span>
              <span class="desc" title={addr}><span class="mono">{p.url}</span>{#if p.has_key}{" · Key in Keychain"}{/if}</span>
              <span class="desc test" class:bad={t?.st === "err"} aria-live="polite" title={t?.st === "err" ? t.msg : undefined}>
                {#if !t}Not tested yet{:else if t.st === "run"}Checking…{:else if t.st === "err"}{t.msg}{:else}{testText(t)}{/if}
              </span>
            </span>
            <span class="end">
              <button type="button" class="btn" disabled={t?.st === "run"} onclick={() => test(p)}>Test</button>
              <button type="button" class="btn icon quiet" id="rm-{p.id}" aria-label="Remove {p.name}" onclick={() => (confirm = p.id)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button>
            </span>
          {/if}
        </li>
      {:else}
        <li class="empty">
          <b>No AI providers yet</b>
          <p>Add one to use summaries, chat and AI cleanup. Myna runs no AI itself.</p>
          <button type="button" class="btn primary" onclick={() => openSub("add")}>Add a provider</button>
        </li>
      {/each}
      {#if !none}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
        <li class="row go" onclick={() => openSub("add")}>
          <span><span class="name addp">Add a provider</span></span>
          <span class="end"><button type="button" class="chev" aria-label="Add a provider" onclick={(e) => { e.stopPropagation(); openSub("add"); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button></span>
        </li>
      {/if}
    </ul>
    <p class="err" class:neutral={!error} role="status">{error || note}</p>
  </section>

  <section aria-labelledby="models-h">
    <h2 id="models-h">Models</h2>
    <p class="help">{none ? "Add a provider first, then pick a model for each." : "Features without a model stay off."}</p>
    <ul class="list">
      {#each SLOTS as [slot, title, help]}
        {@const r = routes[slot]}
        <li class="row">
          <span>
            <span class="name titled"><span class="trunc">{title}</span>
              {#if r && gone(r)}<span class="pill">No longer offered</span>{:else if r && prov(r.provider)}{@render pill(prov(r.provider)!.off_device)}{/if}</span>
            <span class="desc" title={help}>{help}</span>
          </span>
          <div class="pop-wrap" data-pop>
            <button type="button" class="select pk-btn" bind:this={() => triggers[slot], (n) => { triggers[slot] = n!; }} disabled={none} aria-haspopup="dialog" aria-expanded={picker === slot} onclick={() => openPicker(slot)}>{routeName(r)}</button>
            {#if picker === slot}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys are handled for the controls inside) -->
              <div class="pop" role="dialog" aria-label="Choose {title}" tabindex="-1" onkeydown={popKeys}>
                <input class="input search" type="search" placeholder="Search models" aria-label="Search models" bind:this={search} bind:value={q} autocomplete="off" spellcheck="false" />
                <div class="pk-list" role="menu" aria-label={title}>
                  {#each providers as p (p.id)}
                    {@const l = lists[p.id]}
                    {@const ms = l?.models.filter((m) => matches(p, m)) ?? []}
                    {#if l?.st === "load"}
                      <div class="pk-h">{p.name}</div>
                      {#each [0, 1] as i (i)}<div class="sk" aria-hidden="true"><i></i><i></i></div>{/each}
                    {:else if l?.st === "err"}
                      <div class="pk-h">{p.name}</div><p class="pk-note terr">Couldn’t list models: {l.msg}</p>
                    {:else if ms.length}
                      <div class="pk-h">{p.name}</div>
                      {#each ms as m (m.id)}
                        {@const cur = r?.provider === p.id && r.model === m.id}
                        <button type="button" class="pk" role="menuitemradio" aria-checked={cur} onclick={() => choose(slot, p.id, m.id)}>
                          <span class="pk-t"><span class="trunc">{m.id}</span>{#if cur}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}</span>
                          <span class="pk-m">{meta(p, m)}</span>
                        </button>
                      {/each}
                      {#if l.stale}<p class="pk-note">Couldn’t refresh: {l.stale}</p>{/if}
                    {/if}
                  {/each}
                  {#if q.trim() && providers.every((p) => lists[p.id]?.st !== "load" && !lists[p.id]?.models.some((m) => matches(p, m)) && lists[p.id]?.st !== "err")}
                    <p class="pk-note">No models match “{q}”.</p>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    <p class="err" role="status">{pickError}</p>
  </section>
{/if}

<style>
  /* Name, then the host pill right after it, on one 18px line. */
  .titled { display: flex; align-items: center; gap: var(--sp-2); height: var(--lh-body); }
  .trunc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .addp { color: var(--accent-text); }
  /* Providers are 80px in every state: name, address, a reserved status line. */
  .prov { min-height: 80px; }
  .desc.test { margin-top: var(--sp-1); color: var(--text-3); font-variant-numeric: tabular-nums; }
  .desc.test.bad { color: var(--danger-text); }
  .err.neutral { color: var(--text-2); }
  .empty { display: grid; justify-items: start; gap: var(--sp-1); padding: var(--sp-4); }
  .empty b { font-weight: var(--fw-semibold); }
  .empty p { margin: 0 0 var(--sp-2); color: var(--text-2); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .field .help.bad { color: var(--danger-text); }
  .field .help { display: flex; align-items: center; height: var(--lh-footnote); } /* the pill is 20px: it overhangs, the line stays 16 */
  form { display: flex; flex-direction: column; gap: var(--sp-5); }
  .actions { align-items: center; justify-content: flex-end; }
  .actions .err { flex: 1; margin: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pop-wrap { position: relative; }
  .pk-btn { max-width: 240px; min-width: 0; overflow: hidden; white-space: nowrap; text-align: left; }
  .input { width: 100%; }
  .terr { color: var(--danger-text); }
  .pk:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .pop { position: absolute; z-index: 10; right: 0; top: calc(100% + var(--sp-1)); width: min(420px, calc(100vw - 96px)); box-sizing: border-box; padding: 5px;
    border-radius: var(--r-lg); background: var(--bg-raised); box-shadow: var(--shadow-menu); }
  .pop .input { margin-bottom: var(--sp-1); }
  /* Fixed, not max: the list must not resize when models arrive, or the page under it jumps. */
  .pk-list { height: 300px; overflow-y: auto; scrollbar-width: thin; }
  .pk-h { padding: 6px 10px 2px; font-size: var(--fs-caption); font-weight: var(--fw-semibold); color: var(--text-3); }
  .pk-note { margin: 0; padding: 4px 10px; font-size: var(--fs-footnote); color: var(--text-3); }
  .pk { display: flex; flex-direction: column; align-items: stretch; width: 100%; min-height: 42px; box-sizing: border-box; padding: 4px 10px; border: 0; border-radius: var(--r-sm); background: none; color: var(--text); font: inherit; text-align: left; }
  .pk:hover, .pk:focus-visible { background: var(--fill-hover); }
  .pk:focus-visible { outline-offset: -2px; }
  .pk-t { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); font: 12px/18px var(--font-mono); }
  .pk-m { font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3); font-variant-numeric: tabular-nums; }
  /* Same height as a real row, so loading never shifts the list. */
  .sk { height: 42px; box-sizing: border-box; padding: 8px 10px; display: grid; gap: 6px; align-content: center; }
  .sk i { display: block; height: 8px; border-radius: 4px; background: var(--fill-track); }
  .sk i:first-child { width: 55%; }
  .sk i:last-child { width: 80%; }
</style>
