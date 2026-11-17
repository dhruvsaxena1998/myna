<!-- Meeting notes › Models: Transcribe with (#45). The Local model is the default; an Endpoint replaces it after the meeting only, and only once its model passes Test. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Ai, Model } from "./AiProviders.svelte";
  import HostPill from "./HostPill.svelte";
  import type { Cost } from "./meetings";

  let { ai }: { ai: Ai | null } = $props();
  type Check = { ms: number; segments: number; cost: Cost };
  let value = $state("local"); // "local", "none" (Not set: its provider was removed) or "<provider id>|<model>"
  let endpoint = $state(false); // the Endpoint row shows: saved, Not set, or picked here and not tested yet
  let pid = $state<number | null>(null);
  let model = $state("");
  let list = $state<string[]>([]);
  let test = $state<{ st: "run" } | { st: "err"; msg: string } | { st: "ok"; c: Check } | null>(null);
  let error = $state("");

  const providers = $derived(ai?.providers ?? []);
  const pick = (v: string) => (v.includes("|") ? ([+v.slice(0, v.indexOf("|")), v.slice(v.indexOf("|") + 1)] as const) : null);
  const saved = $derived(pick(value));
  const savedProv = $derived(saved ? providers.find((p) => p.id === saved[0]) : undefined);
  const prov = $derived(providers.find((p) => p.id === pid));
  const usd = (n: number) => "$" + (n >= 0.01 ? n.toFixed(2) : n.toPrecision(2));
  const price = (c: Cost) => (c.kind === "usd" ? usd(c.usd) : c.kind === "free" ? "Free" : "Cost unknown");
  const desc = $derived(
    value === "none" ? "Not set: its provider was removed. New meetings wait until you pick a model again or choose Local model."
      : saved ? `${saved[1]} · ${savedProv?.name ?? "Unknown provider"}, after the meeting. What you say while muted stays on this Mac.`
      : endpoint ? "Still the Local model until a model passes Test."
      : "The Local model, after the meeting. Nothing leaves this Mac.",
  );

  // Read again whenever the providers change: removing one makes it Not set.
  $effect(() => {
    void ai;
    invoke<string>("meeting_stt_get").then((v) => {
      value = v;
      endpoint = v !== "local";
      const p = pick(v);
      if (p) [pid, model] = p;
    }, (e) => (error = String(e)));
  });
  // OpenRouter lists its transcription models; any other provider's model id is typed in.
  $effect(() => {
    const p = prov;
    list = [];
    if (p?.kind === "openrouter") invoke<Model[]>("meeting_stt_models", { id: p.id }).then((l) => { if (prov?.id === p.id) list = l.map((m) => m.id); }, () => {});
  });

  async function via(v: string) {
    error = "";
    test = null;
    if (v === "endpoint") {
      endpoint = true;
      pid ??= providers[0]?.id ?? null;
      return;
    }
    try {
      await invoke("meeting_stt_local");
      (value = "local"), (endpoint = false);
    } catch (e) {
      error = String(e);
    }
  }
  async function run() {
    if (!prov) return;
    test = { st: "run" };
    try {
      const c = await invoke<Check>("meeting_stt_test", { provider: prov.id, model: model.trim() });
      test = { st: "ok", c };
      value = `${prov.id}|${model.trim()}`;
    } catch (e) {
      test = { st: "err", msg: String(e) };
    }
  }
</script>

<!-- DESIGN.md › Field: an error line starts with an alert icon. -->
{#snippet alert()}<svg class="ic alert" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" /><path d="M12 8v5M12 16.5v.01" /></svg>{/snippet}

<li class="row wide">
  <span><label class="name" for="stt-via">Transcribe with</label><span class="desc" title={desc}>{desc}</span></span>
  <span class="end">
    {#if !endpoint}<HostPill off={false} />{:else if savedProv}<HostPill off={savedProv.off_device} what="audio" />{/if}
    <select id="stt-via" class="select" value={endpoint ? "endpoint" : "local"} onchange={(e) => via(e.currentTarget.value)}>
      <option value="local">Local model</option>
      <option value="endpoint" disabled={!providers.length}>Endpoint</option>
    </select>
  </span>
</li>
{#if endpoint}
  {@const msg = test?.st === "err" ? test.msg : ""}
  <li class="row wide ep">
    <span><span class="name">Endpoint</span>
      <span class="desc test" class:bad={msg} aria-live="polite" title={msg || undefined}>
        {#if !test}Test sends a 5 s clip. The model is used only if its reply has timestamps.
        {:else if test.st === "run"}Checking…
        {:else if test.st === "err"}{@render alert()}{msg}
        {:else}Works · {test.c.segments} segment{test.c.segments === 1 ? "" : "s"} · {(test.c.ms / 1000).toFixed(1)} s · {price(test.c.cost)}. New meetings use it.{/if}
      </span></span>
    <span class="end">
      <select class="select" aria-label="Provider" value={pid ?? ""} onchange={(e) => ((pid = +e.currentTarget.value), (model = ""), (test = null))}>
        {#if pid === null}<option value="" disabled>Choose a provider</option>{/if}
        {#each providers as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
      </select>
      {#if prov?.kind === "openrouter" && list.length}
        <select class="select" aria-label="Transcription model" bind:value={model} onchange={() => (test = null)}>
          {#if !model}<option value="" disabled>Choose a model</option>{/if}
          {#each [...new Set([...list, ...(model ? [model] : [])])] as m (m)}<option value={m}>{m}</option>{/each}
        </select>
      {:else}
        <input class="input" aria-label="Transcription model" placeholder="Model id" spellcheck="false" autocomplete="off" bind:value={model} oninput={() => (test = null)} />
      {/if}
      <button class="btn" disabled={!prov || !model.trim() || test?.st === "run"} onclick={run}>Test</button>
    </span>
  </li>
{/if}
{#if error}<li class="row"><span class="desc bad">{@render alert()}{error}</span></li>{/if}

<style>
  .desc.test { color: var(--text-3); font-variant-numeric: tabular-nums; }
  /* Provider, model and Test wrap instead of running off a narrow window. */
  .ep .end { flex-wrap: wrap; justify-content: flex-end; }
  .ep .end > :is(.select, .input) { min-width: 0; max-width: 100%; }
  /* A Test's reason is what the user acts on, so it wraps instead of truncating. */
  .desc.bad, .desc.test.bad { color: var(--danger-text); white-space: normal; }
  .alert { width: var(--ic-sm); height: var(--ic-sm); margin: 1px var(--sp-1) 0 0; vertical-align: top; }
</style>
