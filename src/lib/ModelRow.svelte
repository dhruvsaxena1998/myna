<!-- A feature's own model row in Meeting notes (docs/settings.html `feature()`): Uses Main or Fast, any provider's model, or Not set. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Ai, Model } from "./AiProviders.svelte";
  import HostPill from "./HostPill.svelte";

  // `value`: `base` (Uses Main or Fast), "none" (Not set: its provider was removed) or "<provider id>|<model>"; `gone`: that model left its provider's list.
  // `rev`: bumped by the owner to re-draw the menu with what was kept.
  let { id, name, base, value, gone, ai, rev, onpick }: {
    id: string; name: string; base: "main" | "fast"; value: string; gone: boolean; ai: Ai | null; rev: number; onpick: (value: string) => void;
  } = $props();
  let lists = $state<Record<number, string[]>>({});

  const label = $derived(base === "main" ? "Main" : "Fast");
  const providers = $derived(ai?.providers ?? []);
  const split = (v: string) => [+v.slice(0, v.indexOf("|")), v.slice(v.indexOf("|") + 1)] as const;
  // What the feature runs on: the base model's route, or the user's own pick.
  const route = $derived.by(() => {
    if (value === "none") return null;
    if (value === base) return ai?.[base] && !ai[base].gone ? ai[base] : null;
    const [provider, model] = split(value);
    return { provider, model, gone };
  });
  const provider = $derived(route ? providers.find((p) => p.id === route.provider) : undefined);
  const uses = $derived(value === base ? `Uses the ${label} model: ` : "");
  const desc = $derived(
    !providers.length ? "Add a provider on AI providers first."
      : value === "none" ? "Not set: its provider was removed."
      : !route ? `Uses the ${label} model, which is Not set.`
      : `${uses}${route.model}${provider ? ` · ${provider.name}` : ""}`,
  );
  // The cached lists, plus the current pick in case its list was never fetched or no longer has it.
  const ids = (id: number) => [...new Set([...(lists[id] ?? []), ...(route && value !== base && route.provider === id ? [route.model] : [])])];
  $effect(() => {
    for (const p of providers) invoke<Model[]>("ai_models", { id: p.id, fresh: false }).then((l) => { lists[p.id] = l.map((m) => m.id); }, () => {});
  });
</script>

<li class="row">
  <span><label class="name" for={id}>{name}</label><span class="desc" title={desc}>
    {#if route && providers.length}{uses}<span class="mono">{route.model}</span>{provider ? ` · ${provider.name}` : ""}{:else}{desc}{/if}</span></span>
  <span class="end">
    {#if route?.gone}<span class="pill">No longer offered</span>{:else if route && provider}<HostPill off={provider.off_device} />{/if}
    {#key rev}
      <select {id} class="select" {value} disabled={!providers.length} onchange={(e) => onpick(e.currentTarget.value)}>
        {#if value === "none"}<option value="none">Not set</option>{/if}
        <option value={base}>Use the {label} model</option>
        {#each providers as p (p.id)}<optgroup label={p.name}>{#each ids(p.id) as m (m)}<option value="{p.id}|{m}">{m}</option>{/each}</optgroup>{/each}
      </select>
    {/key}
  </span>
</li>
