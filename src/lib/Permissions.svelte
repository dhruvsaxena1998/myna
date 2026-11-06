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
  // The user switches things on in System Settings, then comes back.
  onMount(() => {
    const off = getCurrentWindow().onFocusChanged(({ payload }) => payload && refresh());
    return () => void off.then((f) => f());
  });

  async function ask(kind: "mic" | "ax") {
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
        <span><span class="name">{r.title}</span><span class="desc">{r.desc}</span></span>
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
  <p class="help fix"><strong>Accessibility already on, but Myna still asks?</strong> After a reinstall, macOS can keep the old entry. In System Settings, select Myna and click −, then come back and click Open System Settings again.</p>
</section>

<style>
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .help { margin: var(--sp-1) 0 var(--sp-3); color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .fix { margin: var(--sp-2) 0 0; }
  .fix strong { color: var(--text-2); font-weight: var(--fw-semibold); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .list > li + li { border-top: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  .state { display: flex; justify-content: flex-end; align-items: center; min-width: 190px; height: var(--h-ctl); }
  .ok { display: inline-flex; align-items: center; gap: var(--sp-1); color: var(--ok-text); font-weight: var(--fw-medium); }
  .ic { width: 14px; height: 14px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); color: var(--text); font: inherit; box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); white-space: nowrap; }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
</style>
