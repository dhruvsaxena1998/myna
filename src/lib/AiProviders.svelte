<!-- Settings › AI providers (`part="page"`) and its Add a provider drill-in (`"add"`): docs/settings.html, VIEWS.ai and VIEWS.add. One `ai`, held by the shell, so the page, the drill-in and the other pages agree. -->
<script lang="ts" module>
  export type Provider = { id: number; kind: "openai" | "openrouter" | "claude-code" | "codex"; name: string; url: string; has_key: boolean; off_device: boolean };
  export type Route = { provider: number; model: string; gone: boolean } | null;
  // `decide`: "fast" (uses the Fast model, also when missing), "none" (Not set: its provider was removed) or "<provider id>|<model>".
  export type Ai = { providers: Provider[]; main: Route; fast: Route; decide?: string };
  export type Model = { id: string; input_per_m: number | null; output_per_m: number | null; context: number | null; images: boolean | null };
  type Test = { st: "run" } | { st: "err"; msg: string } | { st: "ok"; ms: number; models: number | null; used: number | null; limit: number | null };
  type Slot = "main" | "fast";
  type Decision = { choice: string; confidence: number | null; cost: Cost; ms: number };
  type Try = { st: "run" } | { st: "err"; msg: string } | { st: "ok"; d: Decision };

  // [kind, label, name, address]
  const PRESETS: [Provider["kind"], string, string, string][] = [
    ["openai", "OpenAI-compatible", "", ""],
    ["openrouter", "OpenRouter", "OpenRouter", "https://openrouter.ai/api/v1"],
  ];
  // Agents (Experimental): the user's own CLI, signed in on this Mac. [kind, label, name, whose plan]
  const AGENTS: [Provider["kind"], string, string, string][] = [
    ["claude-code", "Claude Code (Agent)", "Claude Code", "Claude"],
    ["codex", "Codex (Agent)", "Codex", "ChatGPT"],
  ];
  export const isAgent = (kind: string) => AGENTS.some((a) => a[0] === kind);
  const planOf = (kind: string) => AGENTS.find((a) => a[0] === kind)?.[3] ?? "";
  const agentDesc = (kind: string) => `Agent · Runs on your ${planOf(kind)} plan; its terms apply`;
  const SLOTS: [Slot | "decide", string, string][] = [
    ["main", "Main model", "Writes summaries and answers your questions."],
    ["fast", "Fast model", "Quick jobs like tidying up text and small decisions."],
    ["decide", "Decision model", ""],
  ];
  // Test decision model asks this; the options aren't shown.
  const SAMPLE = { question: "Which folder should “Acme pricing follow-up” go in?", options: ["Inbox", "Acme", "Platform team", "1:1s", "None of these"], instructions: "Pick the folder this meeting belongs in." };
  const hostOf = (u: string) => { try { return new URL(u).hostname.replace(/^\[|\]$/g, ""); } catch { return ""; } };
  const loopback = (u: string) => /^(localhost|127(\.\d{1,3}){3}|::1)$/.test(hostOf(u));
  const usd = (n: number) => "$" + (n >= 0.01 || n === 0 ? n.toFixed(2) : n.toPrecision(2));
  const ctx = (n: number) => (n >= 1e6 ? `${+(n / 1e6).toFixed(1)}M` : `${Math.round(n / 1000)}K`);
  export const routeUses = (ai: Ai, id: number) => [ai.main, ai.fast].some((r) => r?.provider === id) || !!ai.decide?.startsWith(`${id}|`);
  const testText = (t: Extract<Test, { st: "ok" }>) => {
    const s = `${(t.ms / 1000).toFixed(1)} s`;
    if (t.models != null) return `Works · ${t.models} model${t.models === 1 ? "" : "s"} · ${s}`;
    return `Key works · ${usd(t.used ?? 0)} used${t.limit != null ? ` of ${usd(t.limit)}` : ""} · ${s}`;
  };
  const decided = ({ choice, confidence, cost, ms }: Decision, plan: boolean) =>
    [`Picked ${choice}`, ...(confidence != null ? [`${Math.round(confidence * 100)}% sure`] : []), `${(ms / 1000).toFixed(1)} s`,
      cost.kind === "usd" ? usd(cost.usd) : cost.kind === "free" ? "Free" : plan ? "On your plan" : "Cost unknown"].join(" · ");
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import HostPill from "./HostPill.svelte";
  import type { Cost } from "./meetings";

  // `open`: a drill-in under AI providers. `back`: leaves one. `agents`: Agents are on (Experimental), so Add offers them.
  let { ai = $bindable(), part, open: openSub = async () => {}, back = async () => {}, agents = false }: {
    ai: Ai; part: "page" | "add"; open?: (sub: string) => Promise<void> | void; back?: () => Promise<void> | void; agents?: boolean;
  } = $props();
  const kinds = $derived(agents ? [...PRESETS, ...AGENTS.map(([k, l, n]): (typeof PRESETS)[number] => [k, l, n, ""])] : PRESETS);
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
  let picker = $state<Slot | "decide" | null>(null);
  let q = $state("");
  let lists = $state<Record<number, { st: "load" | "ok" | "err"; models: Model[]; msg: string; stale: string }>>({});
  let dlists = $state<typeof lists>({}); // decision models, fetched live each time the picker opens
  let dtry = $state<Try | null>(null); // Test decision model
  let search = $state<HTMLInputElement>();
  const triggers: Partial<Record<Slot | "decide", HTMLButtonElement>> = {};

  const prov = (id: number) => providers.find((p) => p.id === id);
  // A list fetched while Settings is open beats the backend's flag from the saved list.
  const gone = (r: Route) => !!r && (lists[r.provider]?.st === "ok" ? !lists[r.provider].models.some((m) => m.id === r.model) : r.gone);
  const routeName = (r: Route) => (r ? `${prov(r.provider)?.name ?? "Unknown provider"} · ${r.model}` : "Not set");
  const none = $derived(!providers.length);
  // Decision model: its own pick, else the Fast model while that is usable; anything else is Not set.
  const dval = $derived(ai.decide ?? "fast");
  const dpick = $derived(dval.includes("|") ? { provider: +dval.slice(0, dval.indexOf("|")), model: dval.slice(dval.indexOf("|") + 1), gone: false } : null);
  const fastOk = $derived(!!ai.fast && !gone(ai.fast) && !!prov(ai.fast.provider));
  const droute = $derived(dpick && prov(dpick.provider) ? dpick : dval === "fast" && fastOk ? ai.fast : null);
  const routers = $derived(providers.filter((p) => p.kind === "openrouter"));

  function focusNow(n: HTMLElement) { n.focus(); }

  // --- Add form
  function pickKind(e: Event) {
    const kind = (e.currentTarget as HTMLSelectElement).value as Provider["kind"];
    const [, , name, url] = kinds.find((p) => p[0] === kind)!;
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
    if (!isAgent(draft.kind) && (!/^https?:$/.test(proto) || !hostOf(draft.url))) e.url = "Enter the full address, starting with http:// or https://.";
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
      const names = gone.map((s) => (s === "main" ? "Main model" : s === "fast" ? "Fast model" : s)); // a feature's own model comes named
      ai = { providers: ai.providers.filter((x) => x.id !== p.id), main: gone.includes("main") ? null : ai.main, fast: gone.includes("fast") ? null : ai.fast, decide: gone.includes("Decision model") ? "none" : ai.decide };
      if (!droute) dtry = null;
      delete tests[p.id];
      confirm = null;
      note = `Removed ${p.name} and its key.` + (names.length ? ` ${names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names[0]} ${names.length > 1 ? "are" : "is"} now Not set.` : "");
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

  async function testDecide() {
    dtry = { st: "run" };
    const t = await invoke<Decision>("ai_decide", SAMPLE).then((d): Try => ({ st: "ok", d }), (err): Try => ({ st: "err", msg: String(err) }));
    if (dtry?.st === "run") dtry = t; // unless a new pick cleared it
  }

  // --- Picker
  async function openPicker(slot: Slot | "decide") {
    if (picker === slot) return close();
    picker = slot; q = "";
    if (slot === "decide") {
      for (const p of routers) {
        if (dlists[p.id]?.st !== "ok") dlists[p.id] = { st: "load", models: [], msg: "", stale: "" };
        invoke<Model[]>("ai_decide_models", { id: p.id }).then(
          (models) => { dlists[p.id] = { st: "ok", models, msg: "", stale: "" }; },
          (err) => { if (dlists[p.id].st === "ok") dlists[p.id].stale = String(err); else dlists[p.id] = { st: "err", models: [], msg: String(err), stale: "" }; },
        );
      }
    } else for (const p of providers) {
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
  async function chooseDecide(value: string) {
    pickError = "";
    const was = ai.decide;
    ai = { ...ai, decide: value };
    dtry = null;
    close();
    try {
      await invoke("ai_decide_set", { value });
    } catch (err) {
      ai = { ...ai, decide: was };
      pickError = String(err);
    }
  }
  async function choose(slot: Slot | "decide", provider: number, model: string) {
    if (slot === "decide") return chooseDecide(`${provider}|${model}`);
    pickError = "";
    dtry = null;
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
    if (picker === "decide") {
      const parts = ["Quicker and cheaper"];
      if (m.input_per_m != null) parts.push(`${usd(m.input_per_m)} per 1M input tokens`);
      if (m.output_per_m === 0) parts.push("no output cost");
      if (m.context != null) parts.push(`${ctx(m.context)} context`);
      return parts.join(" · ");
    }
    if (p.kind === "openrouter") {
      const parts = [];
      if (m.input_per_m != null && m.output_per_m != null) parts.push(`${usd(m.input_per_m)} in · ${usd(m.output_per_m)} out per 1M tokens`);
      if (m.context != null) parts.push(`${ctx(m.context)} context`);
      if (m.images) parts.push("reads images");
      return parts.join(" · ") || "Cost unknown";
    }
    if (isAgent(p.kind)) return m.id === "default" ? `The model ${p.name} picks · On your plan` : "On your plan";
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
<!-- DESIGN.md › Field: an error line starts with an alert icon. -->
{#snippet alert()}<svg class="ic alert" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16.5v.01" /></svg>{/snippet}

{#if part === "add"}
  {@const or = draft.kind === "openrouter"}
  {@const ag = isAgent(draft.kind)}
  {@const msg = addError || (dtest?.st === "err" ? dtest.msg : "")}
  <form onsubmit={add} novalidate aria-label="Add a provider">
    <section>
      <ul class="list">
        <li class="field"><label for="add-kind">Kind</label>
          <select id="add-kind" class="select" value={draft.kind} onchange={pickKind} aria-describedby="add-kind-h">{#each kinds as [v, l]}<option value={v}>{l}</option>{/each}</select>
          <span class="help" id="add-kind-h" title={ag ? `${agentDesc(draft.kind)}. Sends text off your Mac.` : undefined}>{ag ? `Runs on your ${planOf(draft.kind)} plan; its terms apply. Sends text off your Mac.` : or ? "One key for models from many companies." : "Any server that speaks the OpenAI API."}</span></li>
        <li class="field"><label for="add-name">Name</label>
          <input id="add-name" class="input" bind:value={draft.name} maxlength="40" placeholder={or ? "" : "Server on this Mac"} autocomplete="off" spellcheck="false" oninput={() => (errs = { ...errs, name: undefined })} aria-invalid={!!errs.name} aria-describedby="add-name-h" />
          <span class="help" class:bad={errs.name} id="add-name-h" role="status">{#if errs.name}{@render alert()}{errs.name}{:else}Shown when you pick a model.{/if}</span></li>
        {#if ag}
        <li class="field"><span class="help">Signed in on this Mac, no key needed. Test checks it’s installed and signed in.</span></li>
        {:else}
        <li class="field"><label for="add-url">Address</label>
          <input id="add-url" class="input mono" type="url" bind:value={draft.url} placeholder={or ? "" : "http://localhost:11434/v1"} autocomplete="off" spellcheck="false" oninput={() => ((errs = { ...errs, url: undefined }), (dtest = null))} aria-invalid={!!errs.url} aria-describedby="add-url-h" />
          <span class="help" class:bad={errs.url} id="add-url-h" role="status">{#if errs.url}{@render alert()}{errs.url}{:else if hostOf(draft.url)}{@render pill(!loopback(draft.url))}{:else}{or ? "OpenRouter’s address. Leave it as it is." : "Where the server listens, ending in /v1."}{/if}</span></li>
        <li class="field"><label for="add-key">{or ? "Key" : "Key (optional)"}</label>
          <input id="add-key" class="input mono" type="password" bind:value={draft.key} autocomplete="off" spellcheck="false" placeholder={or ? "sk-or-…" : ""} oninput={() => ((errs = { ...errs, key: undefined }), (dtest = null))} aria-invalid={!!errs.key} aria-describedby="add-key-h" />
          <span class="help" class:bad={errs.key} id="add-key-h" role="status">{#if errs.key}{@render alert()}{errs.key}{:else}{or ? "Find it at openrouter.ai/keys." : "Only if your server asks for one."} Kept in your Mac’s Keychain.{/if}</span></li>
        {/if}
      </ul>
    </section>
    <div class="actions">
      <p class="err" class:neutral={!msg} role="status" title={msg || undefined}>{#if msg}{@render alert()}{msg}{:else}{(dtest?.st === "run" ? "Checking…" : dtest?.st === "ok" ? testText(dtest) : "")}{/if}</p>
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
              {#if isAgent(p.kind)}<span class="desc" title={agentDesc(p.kind)}>{agentDesc(p.kind)}</span>
              {:else}<span class="desc" title={addr}><span class="mono">{p.url}</span>{#if p.has_key}{" · Key in Keychain"}{/if}</span>{/if}
              <span class="desc test" class:bad={t?.st === "err"} aria-live="polite" title={t?.st === "err" ? t.msg : undefined}>
                {#if !t}Not tested yet{:else if t.st === "run"}Checking…{:else if t.st === "err"}{@render alert()}{t.msg}{:else}{testText(t)}{/if}
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
    <p class="err" class:neutral={!error} role="status">{#if error}{@render alert()}{error}{:else}{note}{/if}</p>
  </section>

  <section aria-labelledby="models-h">
    <h2 id="models-h">Models</h2>
    <p class="help">{none ? "Add a provider first, then pick a model for each." : "Features without a model stay off."}</p>
    <ul class="list">
      {#each SLOTS as [slot, title, text]}
        {@const d = slot === "decide"}
        {@const r = d ? droute : routes[slot]}
        {@const agentFast = slot === "fast" && !!r && isAgent(prov(r.provider)?.kind ?? "")}
        {@const help = agentFast ? "An Agent takes a few seconds: dictation cleanup waits up to 15 s before pasting." : !d ? text : droute ? "Which folder a meeting goes in, which pictures matter. Reads text only." : "Pick a model here, or set a Fast model above."}
        {@const ps = d ? routers : providers}
        {@const ls = d ? dlists : lists}
        <li class="row" class:dec={d}>
          <span>
            <span class="name titled"><span class="trunc">{title}</span>
              {#if !d && r && gone(r)}<span class="pill">No longer offered</span>{:else if r && prov(r.provider)}{@render pill(prov(r.provider)!.off_device)}{/if}</span>
            <span class="desc" title={help}>{help}</span>
          </span>
          <div class="pop-wrap" data-pop>
            <button type="button" class="select pk-btn" bind:this={() => triggers[slot], (n) => { triggers[slot] = n!; }} disabled={none} aria-haspopup="dialog" aria-expanded={picker === slot} onclick={() => openPicker(slot)}>{d ? (dpick && droute ? routeName(dpick) : droute ? "Fast model" : "Not set") : routeName(r)}</button>
            {#if picker === slot}
              <!-- svelte-ignore a11y_no_noninteractive_element_interactions (keys are handled for the controls inside) -->
              <div class="pop" role="dialog" aria-label="Choose {title}" tabindex="-1" onkeydown={popKeys}>
                <input class="input search" type="search" placeholder="Search models" aria-label="Search models" bind:this={search} bind:value={q} autocomplete="off" spellcheck="false" />
                <div class="pk-list" role="menu" aria-label={title}>
                  {#if d}
                    <button type="button" class="pk" role="menuitemradio" aria-checked={dval === "fast"} onclick={() => chooseDecide("fast")}>
                      <span class="pk-t ui"><span class="trunc">Fast model</span>{#if dval === "fast"}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}</span>
                      <span class="pk-m">{fastOk ? `Uses ${routeName(ai.fast)}` : "Not set: pick a Fast model above"}</span>
                    </button>
                    {#if !routers.length}<p class="pk-note">Add {PRESETS.find((k) => k[0] === "openrouter")![1]} above to use decision models.</p>{/if}
                  {/if}
                  {#each ps as p (p.id)}
                    {@const l = ls[p.id]}
                    {@const ms = l?.models.filter((m) => matches(p, m)) ?? []}
                    {#if l?.st === "load"}
                      <div class="pk-h">{p.name}</div>
                      {#each [0, 1] as i (i)}<div class="sk" aria-hidden="true"><i></i><i></i></div>{/each}
                    {:else if l?.st === "err"}
                      <div class="pk-h">{p.name}</div><p class="pk-note terr">Couldn’t list models: {l.msg}</p>
                    {:else if ms.length}
                      <div class="pk-h">{p.name}</div>
                      {#each ms as m (m.id)}
                        {@const cur = (d ? dpick : r)?.provider === p.id && (d ? dpick : r)!.model === m.id}
                        <button type="button" class="pk" role="menuitemradio" aria-checked={cur} onclick={() => choose(slot, p.id, m.id)}>
                          <span class="pk-t"><span class="trunc">{m.id}</span>{#if cur}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}</span>
                          <span class="pk-m">{meta(p, m)}</span>
                        </button>
                      {/each}
                      {#if l.stale}<p class="pk-note">Couldn’t refresh: {l.stale}</p>{/if}
                    {/if}
                  {/each}
                  {#if q.trim() && ps.length && ps.every((p) => ls[p.id]?.st !== "load" && !ls[p.id]?.models.some((m) => matches(p, m)) && ls[p.id]?.st !== "err")}
                    <p class="pk-note">No models match “{q}”.</p>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        </li>
      {/each}
      <li class="row dtest">
        <span>
          <span class="name">Test decision model</span>
          <span class="desc" title={SAMPLE.question}>{SAMPLE.question}</span>
          <span class="desc test" class:bad={dtry?.st === "err"} aria-live="polite" title={dtry?.st === "err" ? `Couldn’t decide: ${dtry.msg}` : undefined}>
            {#if !dtry}Not tested yet{:else if dtry.st === "run"}Checking…{:else if dtry.st === "err"}{@render alert()}Couldn’t decide: {dtry.msg}{:else}{decided(dtry.d, isAgent(prov(droute?.provider ?? -1)?.kind ?? ""))}{/if}
          </span>
        </span>
        <span class="end"><button type="button" class="btn" disabled={!droute || dtry?.st === "run"} onclick={testDecide}>Test</button></span>
      </li>
    </ul>
    <p class="err" role="status">{#if pickError}{@render alert()}{pickError}{/if}</p>
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
  .prov, .dtest { min-height: 80px; }
  /* Each line is reserved, so it truncates (full text in `title`) instead of wrapping on a narrow window. */
  .prov .desc, .dtest .desc, .dec .desc { white-space: nowrap; }
  .desc.test { margin-top: var(--sp-1); color: var(--text-3); font-variant-numeric: tabular-nums; }
  .desc.test.bad { color: var(--danger-text); }
  .err.neutral { color: var(--text-2); }
  /* 14px, inside the 16px line so an error never makes it taller. */
  .alert { width: var(--ic-sm); height: var(--ic-sm); margin: 1px var(--sp-1) 0 0; vertical-align: top; }
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
    border-radius: var(--r-lg); background: var(--mat-menu); -webkit-backdrop-filter: var(--blur); backdrop-filter: var(--blur);
    box-shadow: inset 0 0 0 .5px var(--edge), var(--shadow-menu); animation: rise var(--dur) var(--ease-out); }
  /* DESIGN.md › Motion: popovers enter with an 8px rise and a fade. */
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } }
  @media (prefers-reduced-motion: reduce) { .pop { animation: none; } }
  .pop .input { margin-bottom: var(--sp-1); }
  /* Fixed, not max: the list must not resize when models arrive, or the page under it jumps. */
  .pk-list { height: 300px; overflow-y: auto; scrollbar-width: thin; }
  .pk-h { padding: 6px 10px 2px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-3); }
  .pk-note { margin: 0; padding: 4px 10px; font-size: var(--fs-footnote); color: var(--text-3); }
  .pk { display: flex; flex-direction: column; align-items: stretch; width: 100%; min-height: 42px; box-sizing: border-box; padding: 4px 10px; border: 0; border-radius: var(--r-sm); background: none; color: var(--text); font: inherit; text-align: left; }
  /* Like a native menu: the item under the pointer or focus fills with the accent. */
  .pk:hover, .pk:focus-visible { background: var(--accent); color: var(--on-accent); }
  .pk:hover .pk-m, .pk:focus-visible .pk-m { color: inherit; }
  .pk:focus-visible { outline-offset: -2px; }
  .pk-t { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); font: 12px/18px var(--font-mono); }
  .pk-t.ui { font: var(--fs-body)/18px var(--font); } /* "Fast model" is a choice, not an ID */
  .pk-m { font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3); font-variant-numeric: tabular-nums; }
  /* Same height as a real row, so loading never shifts the list. */
  .sk { height: 42px; box-sizing: border-box; padding: 8px 10px; display: grid; gap: 6px; align-content: center; }
  .sk i { display: block; height: 8px; border-radius: 4px; background: var(--fill-track); }
  .sk i:first-child { width: 55%; }
  .sk i:last-child { width: 80%; }
</style>
