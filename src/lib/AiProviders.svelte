<!-- Settings › AI provider: providers, Main and Fast models (docs/index.html, aiPage). -->
<script lang="ts" module>
  export type Provider = { id: number; kind: "openai" | "openrouter"; name: string; url: string; has_key: boolean; off_device: boolean };
  export type Route = { provider: number; model: string; gone: boolean } | null;
  export type Ai = { providers: Provider[]; main: Route; fast: Route };
  type Model = { id: string; input_per_m: number | null; output_per_m: number | null; context: number | null; images: boolean | null };
  type Test = { st: "run" } | { st: "err"; msg: string } | { st: "ok"; ms: number; models: number | null; used: number | null; limit: number | null };
  type Slot = "main" | "fast";

  // [kind, label, name, address, what to type in the empty fields]
  const PRESETS: [Provider["kind"], string, string, string][] = [
    ["openai", "OpenAI-compatible", "", ""],
    ["openrouter", "OpenRouter", "OpenRouter", "https://openrouter.ai/api/v1"],
  ];
  const SLOTS: [Slot, string, string][] = [
    ["main", "Main model", "Writes summaries and answers your questions."],
    ["fast", "Fast model", "Quick jobs like tidying up text, and quick decisions when no Decision provider is set."],
  ];
  const hostOf = (u: string) => { try { return new URL(u).hostname.replace(/^\[|\]$/g, ""); } catch { return ""; } };
  const loopback = (u: string) => /^(localhost|127(\.\d{1,3}){3}|::1)$/.test(hostOf(u));
  const usd = (n: number) => "$" + (n >= 0.01 || n === 0 ? n.toFixed(2) : n.toPrecision(2));
  const ctx = (n: number) => (n >= 1e6 ? `${+(n / 1e6).toFixed(1)}M` : `${Math.round(n / 1000)}K`);
  const testText = (t: Extract<Test, { st: "ok" }>) => {
    const s = `${(t.ms / 1000).toFixed(1)} s`;
    if (t.models != null) return `Connected · ${t.models} model${t.models === 1 ? "" : "s"} · ${s}`;
    return `Key works · ${usd(t.used ?? 0)} used${t.limit != null ? ` of ${usd(t.limit)}` : ""} · ${s}`;
  };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import HostPill from "./HostPill.svelte";

  let { initial }: { initial: Ai } = $props();
  let providers = $derived(initial.providers); // assignable: add and remove update them
  let routes = $derived({ main: initial.main, fast: initial.fast });
  let tests = $state<Record<number, Test>>({});
  let notice = $state("");
  let error = $state("");
  let confirm = $state<number | null>(null);
  let adding = $state(false);
  let draft = $state({ kind: "openai" as Provider["kind"], name: "", url: "", key: "" });
  let errs = $state<{ name?: string; url?: string; key?: string }>({});
  let addError = $state("");
  let busy = $state(false);
  let open = $state<Slot | null>(null);
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
      providers = [...providers, p];
      invoke("ai_models", { id: p.id, fresh: true }).catch(() => {}); // warm the cache so the first picker open is instant
      adding = false;
      notice = "";
    } catch (err) {
      addError = String(err);
    } finally {
      busy = false;
    }
  }
  function startAdd() {
    draft = { kind: "openai", name: "", url: "", key: "" };
    errs = {}; addError = ""; adding = true;
  }

  // --- Remove
  async function remove(p: Provider) {
    error = "";
    try {
      const gone = await invoke<string[]>("ai_remove", { id: p.id });
      const names = gone.map((s) => (s === "main" ? "Main model" : "Fast model"));
      const list = names.length > 1 ? `${names[0]} and ${names[1]}` : names[0];
      providers = providers.filter((x) => x.id !== p.id);
      routes = { main: gone.includes("main") ? null : routes.main, fast: gone.includes("fast") ? null : routes.fast };
      delete tests[p.id];
      confirm = null;
      notice = `Removed ${p.name} and its key.` + (names.length ? ` ${list} ${names.length > 1 ? "are" : "is"} now Not set. Anything that used ${names.length > 1 ? "them" : "it"} is off until you pick another.` : "");
    } catch (err) {
      error = String(err);
    }
  }

  // --- Test
  async function test(p: Provider) {
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
    if (open === slot) return close();
    open = slot; q = "";
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
    const slot = open;
    open = null;
    if (refocus && slot) triggers[slot]?.focus();
  }
  async function choose(slot: Slot, provider: number, model: string) {
    error = "";
    const was = routes[slot];
    routes = { ...routes, [slot]: { provider, model } };
    close();
    try {
      await invoke("ai_set", { slot, provider, model });
    } catch (err) {
      routes = { ...routes, [slot]: was };
      error = String(err);
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
      return parts.join(" · ") || "cost unknown";
    }
    return p.off_device ? "cost unknown" : "Free · runs on your server";
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
    if (open && !(e.target as Element).closest("[data-pop]")) close(false);
  }
</script>

<svelte:window onpointerdown={outside} />

{#snippet pill(off: boolean)}<HostPill {off} />{/snippet}

{#if notice}
  <div class="notice" role="status">
    <svg class="ic caution-i" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16.5v.01" /></svg>
    <span>{notice}</span>
    <button type="button" class="btn icon quiet" aria-label="Dismiss" onclick={() => (notice = "")}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button>
  </div>
{/if}

<section class="group" aria-labelledby="prov-h">
  <h2 id="prov-h">Providers</h2>
  <p class="help">Servers on this Mac keep your words here. Anything else gets them, even another computer on your network. Test checks the address and key without spending anything.</p>
  {#if none && !adding}
    <div class="list empty">
      <h3>No AI providers yet</h3>
      <p>Add one to use summaries, chat and AI clean-up. Myna runs no AI itself.</p>
      <button type="button" class="btn primary" onclick={startAdd}>Add a provider</button>
    </div>
  {:else}
    {#if !none}
      <ul class="list">
        {#each providers as p (p.id)}
          {@const t = tests[p.id]}
          <li class="prov">
            {#if confirm === p.id}
              <div class="confirm" role="alertdialog" aria-labelledby="rm-t">
                <span id="rm-t">Remove {p.name}? Its key is deleted from your Keychain.{[routes.main, routes.fast].some((r) => r?.provider === p.id) ? " Models that use it become Not set." : ""}</span>
                <button type="button" class="btn" use:focusNow onclick={async () => { confirm = null; await tick(); document.getElementById(`rm-${p.id}`)?.focus(); }}>Cancel</button>
                <button type="button" class="btn danger" onclick={() => remove(p)}>Remove</button>
              </div>
            {:else}
              <div class="top">
                <span class="pname"><span class="name">{p.name}</span><span class="url">{p.url}</span>{#if p.has_key}<span class="help">Key in Keychain</span>{/if}</span>
                {@render pill(p.off_device)}
                <button type="button" class="btn tbtn" disabled={t?.st === "run"} onclick={() => test(p)}>Test</button>
                <button type="button" class="btn icon quiet" id="rm-{p.id}" aria-label="Remove {p.name}" onclick={() => (confirm = p.id)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button>
              </div>
              <div class="tline" aria-live="polite">
                {#if !t}<span class="help">Not tested yet</span>
                {:else if t.st === "run"}<span class="help">Checking…</span>
                {:else if t.st === "err"}<span class="terr">{t.msg}</span><button type="button" class="btn quiet sm" onclick={() => test(p)}>Retry</button>
                {:else}<span class="help trunc">{testText(t)}</span>{/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if adding}
      <form class="list form" aria-labelledby="add-h" onsubmit={add} novalidate>
        <h3 id="add-h">Add an AI provider</h3>
        <div class="two">
          <div class="field"><label for="add-kind">Kind</label>
            <select id="add-kind" class="select" value={draft.kind} onchange={pickKind}>{#each PRESETS as [v, l]}<option value={v}>{l}</option>{/each}</select>
            <span class="help kind-h">{draft.kind === "openrouter" ? "One key for models from many companies." : "Ollama, LM Studio, oMLX or any server that speaks the OpenAI API."}</span></div>
          <div class="field"><label for="add-name">Name</label>
            <input id="add-name" class="input" bind:value={draft.name} maxlength="40" placeholder={draft.kind === "openai" ? "Ollama on this Mac" : ""} autocomplete="off" spellcheck="false" aria-invalid={!!errs.name} aria-describedby="add-name-h add-name-e" />
            <span class="help" id="add-name-h">Shown when you pick a model.</span><span class="err" id="add-name-e" role="status">{errs.name ?? ""}</span></div>
        </div>
        <div class="field"><label for="add-url">Address</label>
          <input id="add-url" class="input mono" type="url" bind:value={draft.url} placeholder={draft.kind === "openai" ? "http://localhost:11434/v1" : ""} autocomplete="off" spellcheck="false" aria-invalid={!!errs.url} aria-describedby="add-url-h add-url-e" />
          <span class="help pill-slot" id="add-url-h">{#if hostOf(draft.url)}{@render pill(!loopback(draft.url))}{/if}</span><span class="err" id="add-url-e" role="status">{errs.url ?? ""}</span></div>
        <div class="field"><label for="add-key">{draft.kind === "openrouter" ? "Key" : "Key (optional)"}</label>
          <input id="add-key" class="input mono" type="password" bind:value={draft.key} autocomplete="off" spellcheck="false" placeholder={draft.kind === "openrouter" ? "sk-or-…" : ""} aria-invalid={!!errs.key} aria-describedby="add-key-h add-key-e" />
          <span class="help" id="add-key-h">{draft.kind === "openrouter" ? "Find it at openrouter.ai/keys." : "Only if your server asks for one."} Myna keeps it in your Mac’s Keychain.</span>
          <span class="err" id="add-key-e" role="status">{errs.key ?? ""}</span></div>
        <p class="err" role="status">{addError}</p>
        <div class="actions"><button type="button" class="btn" onclick={() => (adding = false)}>Cancel</button><button class="btn primary" disabled={busy}>Add</button></div>
      </form>
    {:else}
      <div class="addrow"><button type="button" class="btn" onclick={startAdd}>Add a provider</button></div>
    {/if}
  {/if}
  <p class="err" role="status">{error}</p>
</section>

<section class="group" aria-labelledby="models-h">
  <h2 id="models-h">Models</h2>
  <ul class="list">
    {#each SLOTS as [slot, title, help]}
      {@const r = routes[slot]}
      <li class="srow">
        <span><span class="name">{title}</span><span class="desc">{help}</span>
          {#if r && gone(r)}<span class="notset">No longer offered by {prov(r.provider)?.name}. What uses it stays off until you pick another.</span>
          {:else if r && prov(r.provider)}{@render pill(prov(r.provider)!.off_device)}
          {:else}<span class="notset">Not set{none ? ". Add a provider first." : ". What uses it stays off until you pick one."}</span>{/if}</span>
        <div class="pop-wrap" data-pop>
          <button type="button" class="btn pk-btn" bind:this={() => triggers[slot], (n) => { triggers[slot] = n!; }} disabled={none} aria-haspopup="dialog" aria-expanded={open === slot} onclick={() => openPicker(slot)}>
            <span class="trunc">{routeName(r)}</span><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 10l5 5 5-5" /></svg></button>
          {#if open === slot}
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
  <p class="help after">Every feature uses one of these. A feature that needs something else lets you pick there.</p>
</section>

<style>
  h3 { margin: 0; font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .help.after { margin-top: var(--sp-2); }
  .list.empty { padding: var(--sp-5) var(--sp-4); text-align: center; }
  .list.empty p { margin: var(--sp-1) 0 var(--sp-3); color: var(--text-2); }
  .prov { padding: var(--sp-3) var(--sp-4); }
  .top { display: flex; align-items: center; gap: var(--sp-2); }
  .pname { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .name { display: block; font-weight: var(--fw-semibold); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .url { font: 12px/16px ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .confirm { display: flex; align-items: center; gap: var(--sp-2); min-height: 52px; }
  .confirm span { flex: 1; }
  /* Reserved line, so a test result never moves the rows below. */
  .tline { display: flex; align-items: center; gap: var(--sp-2); min-height: 24px; margin-top: var(--sp-1); font-variant-numeric: tabular-nums; }
  .tline .help { margin: 0; min-width: 0; }
  .trunc { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .terr { color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .tbtn { min-width: 64px; justify-content: center; }
  .addrow { margin-top: var(--sp-3); }
  .notice { display: flex; align-items: center; gap: var(--sp-2); margin-bottom: var(--sp-4); padding: var(--sp-2) var(--sp-2) var(--sp-2) var(--sp-4); border-radius: var(--r-lg); background: var(--caution-soft); }
  .notice span { flex: 1; }
  .caution-i { color: var(--caution-text); }
  .notset { display: block; margin-top: var(--sp-1); color: var(--caution-text); }
  .srow { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .srow :global(.pill) { margin-top: var(--sp-1); }
  .form { margin-top: var(--sp-3); padding: var(--sp-3) var(--sp-4); display: grid; gap: var(--sp-2); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: var(--sp-3); }
  .field { display: grid; gap: var(--sp-1); align-content: start; }
  .field label { font-weight: var(--fw-semibold); }
  .field .help { margin: 0; min-height: var(--lh-footnote); }
  .field .kind-h { min-height: calc(var(--lh-footnote) * 2); } /* both kinds' help fit, so switching never moves the form */
  .input { height: var(--h-ctl); box-sizing: border-box; width: 100%; padding: 0 var(--sp-3); border: 0; border-radius: var(--r-md); background: var(--fill-field); color: var(--text); font: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong); }
  .input:focus-visible { outline: none; box-shadow: inset 0 0 0 1.5px var(--accent); }
  .input[aria-invalid="true"] { box-shadow: inset 0 0 0 1.5px var(--danger); }
  .mono { font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }
  /* Reserved line so an error never shifts the form. */
  .err { margin: 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .actions { display: flex; justify-content: flex-end; gap: var(--sp-2); }
  .ic { width: 14px; height: 14px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .btn:focus-visible, .pk:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .btn.primary { background: var(--accent); color: var(--on-accent); box-shadow: var(--shadow-solid); }
  .btn.primary:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 88%, black); }
  .btn.danger { background: var(--danger); color: var(--on-solid); box-shadow: var(--shadow-solid); }
  .btn.quiet { background: none; box-shadow: none; color: var(--text-2); }
  .btn.icon { width: var(--h-ctl-sm); height: var(--h-ctl-sm); padding: 0; justify-content: center; }
  .btn.sm { height: var(--h-ctl-sm); padding: 0 10px; }
  .pop-wrap { position: relative; }
  .pk-btn { max-width: 360px; }
  .pop { position: absolute; z-index: 10; right: 0; top: calc(100% + var(--sp-1)); width: min(420px, calc(100vw - 96px)); box-sizing: border-box; padding: 5px;
    border-radius: var(--r-lg); background: var(--bg-raised); box-shadow: var(--shadow-menu); }
  .search { margin-bottom: 5px; }
  /* Fixed, not max: the list must not resize when models arrive, or the page under it jumps. */
  .pk-list { height: 300px; overflow-y: auto; scrollbar-width: thin; }
  .pk-h { padding: 6px 10px 2px; font-size: var(--fs-caption); font-weight: var(--fw-semibold); color: var(--text-3); }
  .pk-note { margin: 0; padding: 4px 10px; font-size: var(--fs-footnote); color: var(--text-3); }
  .pk { display: flex; flex-direction: column; align-items: stretch; width: 100%; min-height: 42px; box-sizing: border-box; padding: 4px 10px; border: 0; border-radius: var(--r-sm); background: none; color: var(--text); font: inherit; text-align: left; }
  .pk:hover, .pk:focus-visible { background: var(--fill-hover); }
  .pk:focus-visible { outline-offset: -2px; }
  .pk-t { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); font: 12px/18px ui-monospace, SFMono-Regular, Menlo, monospace; }
  .pk-m { font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3); font-variant-numeric: tabular-nums; }
  /* Same height as a real row, so loading never shifts the list. */
  .sk { height: 42px; box-sizing: border-box; padding: 8px 10px; display: grid; gap: 6px; align-content: center; }
  .sk i { display: block; height: 8px; border-radius: 4px; background: var(--fill-track); }
  .sk i:first-child { width: 55%; }
  .sk i:last-child { width: 80%; }
  /* Holds the pill's height while the address is empty, so typing one doesn't push the form down. */
  .pill-slot { display: block; min-height: 20px; }
</style>
