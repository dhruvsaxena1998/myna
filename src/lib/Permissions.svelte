<!-- What macOS lets Myna use for voice typing (prototype: obPerms rows). Onboarding (story 02) reuses this. -->
<script lang="ts" module>
  export type Perms = { mic: "ask" | "allowed" | "denied"; ax: boolean };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  let { initial }: { initial: Perms } = $props();

  let perms = $derived(initial); // assignable: refreshed on focus and after asking
  const refresh = async () => (perms = await invoke<Perms>("permissions_state"));
  // Back from System Settings with Accessibility still off: the stale-entry hint shows, and only then.
  let opened = $state(false), back = $state(false);
  // The user switches things on in System Settings, then comes back.
  onMount(() => {
    const off = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (!payload) return;
      if (opened) back = true;
      refresh();
    });
    return () => void off.then((f) => f());
  });

  async function ask(kind: "mic" | "ax") {
    if (kind === "ax") opened = true;
    await invoke("permission_ask", { kind });
    await refresh();
  }

  const rows = $derived([
    { kind: "mic" as const, title: "Microphone", desc: "So Myna can hear you.", ok: perms.mic === "allowed", popup: perms.mic === "ask" },
    { kind: "ax" as const, title: "Accessibility", desc: "So Myna can put your words into the app you’re using.", ok: perms.ax, popup: false },
  ]);
</script>

<section aria-labelledby="perm-h">
  <h2 id="perm-h">Permissions</h2>
  <p class="help">Your Mac asks once for each. For Accessibility, switch Myna on in System Settings, then come back.</p>
  <ul class="list">
    {#each rows as r (r.kind)}
      <li class="row">
        <span><span class="name">{r.title}</span><span class="desc">{r.desc}</span>{#if r.kind === "ax" && back && !r.ok}<span class="desc">Still asking? Remove Myna from the list with −, then open System Settings again.</span>{/if}</span>
        <!-- Fixed width so Allowed and the buttons swap without moving the text. -->
        <span class="state">
          {#if r.ok}
            <span class="ok"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>Allowed</span>
          {:else if r.popup}
            <button class="btn" aria-label="Allow {r.title}" onclick={() => ask(r.kind)}>Allow</button>
          {:else}
            <button class="btn" aria-label="Open System Settings for {r.title}" onclick={() => ask(r.kind)}>Open System Settings</button>
          {/if}
        </span>
      </li>
    {/each}
  </ul>
</section>

<style>
  .state { display: flex; justify-content: flex-end; align-items: center; min-width: 190px; height: var(--h-ctl); }
  .ok { display: inline-flex; align-items: center; gap: var(--sp-1); color: var(--ok-text); font-weight: var(--fw-medium); }
  .ic { width: 14px; height: 14px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
</style>
