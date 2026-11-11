<!-- What macOS lets Myna use (prototype: obPerms rows): voice typing's, or meeting notes'. Onboarding (story 02) reuses this. -->
<script lang="ts" module>
  /** `audio` is "unknown" when macOS gives no way to check; it asks the first time a meeting records. */
  export type Perms = { mic: "ask" | "allowed" | "denied"; ax: boolean; audio: "ask" | "allowed" | "denied" | "unknown" };
  type Kind = "mic" | "ax" | "audio";
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  // `rows`: just the `<li>` rows, for a list that already exists (Listen's, under Use voice typing).
  let { initial, kinds = ["mic", "ax"], rows: bare = false }: { initial: Perms; kinds?: Kind[]; rows?: boolean } = $props();

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

  async function ask(kind: Kind) {
    if (kind === "ax") opened = true;
    await invoke("permission_ask", { kind });
    await refresh();
  }

  const items = $derived(
    [
      { kind: "mic" as const, title: "Microphone", desc: "So Myna can hear you.", ok: perms.mic === "allowed", popup: perms.mic === "ask" },
      { kind: "ax" as const, title: "Accessibility", desc: "So Myna can put your words into the app you’re using.", ok: perms.ax, popup: false },
      { kind: "audio" as const, title: "Screen & System Audio Recording", desc: "So Myna can hear the other people on a call.", ok: perms.audio === "allowed", popup: perms.audio === "ask" },
    ].filter((r) => kinds.includes(r.kind)),
  );
</script>

{#snippet list()}
  {#each items as r (r.kind)}
    <li class="row">
      <span><span class="name">{r.title}</span><span class="desc" title={r.desc}>{r.desc}</span>{#if r.kind === "ax" && back && !r.ok}<span class="desc">Still asking? Remove Myna from the list with −, then open System Settings again.</span>{/if}{#if r.kind === "audio" && perms.audio === "unknown"}<span class="desc">Myna can’t check this one. Your Mac asks the first time you record a meeting.</span>{/if}</span>
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
{/snippet}

{#if bare}
  {@render list()}
{:else}
  <section aria-labelledby="perm-h">
    <h2 id="perm-h">Permissions</h2>
    <p class="help">Your Mac asks once for each.{#if kinds.includes("ax")} For Accessibility, switch Myna on in System Settings, then come back.{/if}</p>
    <ul class="list">{@render list()}</ul>
  </section>
{/if}

<style>
  .state { display: flex; justify-content: flex-end; align-items: center; min-width: 164px; height: var(--h-ctl); }
</style>
