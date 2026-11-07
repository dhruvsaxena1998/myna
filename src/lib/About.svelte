<!-- About (prototype: case 'about' and "app updates" in docs/index.html): the version and its update state, Update mode, Channel, the data folder. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { upd, watchUpdates, type Prefs, type UpdateState } from "./updates.svelte";

  let { version, path, prefs: initial, status }: { version: string; path: string; prefs: Prefs; status: UpdateState } = $props();
  let prefs = $derived(initial); // assignable: a pick shows at once, a failed save reverts it
  let error = $state("");
  onMount(() => watchUpdates(status));

  const MODES: [Prefs["mode"], string, string][] = [
    ["auto", "Check and download", "Checks on its own, downloads in the background, then asks you to restart."],
    ["check", "Check only", "Checks on its own and tells you when there’s an update. You choose when to download."],
    ["off", "Off", "Never checks on its own. Use Check for updates when you want to."],
  ];

  const EVERY: [Prefs["every"], string, string][] = [
    ["launch", "At launch and daily", "Checks each time Myna opens, and once a day while it’s open."],
    ["day", "Daily", "Checks once a day, even if you quit and reopen Myna."],
    ["week", "Weekly", "Checks once a week, even if you quit and reopen Myna."],
  ];

  async function save(change: Partial<Prefs>) {
    const was = prefs;
    prefs = { ...prefs, ...change };
    error = "";
    try {
      await invoke("updates_set", { prefs });
    } catch (e) {
      prefs = was;
      error = String(e);
    }
  }

  // The check reports through the update state, and opens the update pop-up with the result.
  const check = () => invoke("update_check").catch(() => {});
  const open = () => invoke("update_open");
  let shown = $derived(error || (upd.s.state === "error" ? upd.s.message : ""));
</script>

<section aria-label="Myna">
  <ul class="list">
    <li class="row">
      <span><span class="name">Version</span><span class="desc">{version}{#if import.meta.env.VITE_BUILD} · {import.meta.env.VITE_BUILD}{/if}</span></span>
      <!-- Fixed size, so no state moves the row. -->
      <div class="slot">
        {#if upd.s.state === "checking"}<span class="pill" role="status">Checking…</span>
        {:else if upd.s.state === "available"}<button class="btn" onclick={open}>Update available · {upd.s.version}</button>
        {:else if upd.s.state === "downloading"}<span class="pill" role="status">Downloading {upd.s.pct}%</span>
        {:else if upd.s.state === "downloaded"}<button class="btn" onclick={open}>Restart to update</button>
        {:else if upd.s.state === "error"}<span class="pill alert">Update failed</span><button class="btn" onclick={check}>Try again</button>
        {:else}<button class="btn" onclick={check}>Check for updates</button>{/if}
      </div>
    </li>
    <li class="row">
      <span><label class="name" for="upd-mode">Update mode</label><span class="desc" id="upd-mode-d">{MODES.find(([v]) => v === prefs.mode)?.[2]}</span></span>
      <select id="upd-mode" class="select" aria-describedby="upd-mode-d" value={prefs.mode} onchange={(e) => save({ mode: e.currentTarget.value as Prefs["mode"] })}>
        {#each MODES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <li class="row">
      <span><label class="name" for="upd-every">How often to check</label><span class="desc" id="upd-every-d">{prefs.mode === "off" ? "Update mode is Off." : EVERY.find(([v]) => v === prefs.every)?.[2]}</span></span>
      <select id="upd-every" class="select" aria-describedby="upd-every-d" disabled={prefs.mode === "off"} value={prefs.every} onchange={(e) => save({ every: e.currentTarget.value as Prefs["every"] })}>
        {#each EVERY as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <li class="row">
      <span><span class="name" id="chan-l">Channel</span><span class="desc">Beta also gets newer stable releases. Switching to stable keeps your current version until stable passes it.</span></span>
      <fieldset class="seg" aria-labelledby="chan-l">
        {#each [["stable", "Stable"], ["beta", "Beta"]] as [v, label]}
          <label><input type="radio" name="channel" value={v} checked={prefs.channel === v} onchange={() => save({ channel: v as Prefs["channel"] })} /><span data-l={label}>{label}</span></label>
        {/each}
      </fieldset>
    </li>
    <li class="row">
      <span><span class="name">Where your data lives</span><span class="desc"><span class="mono">{path}</span><br />Your meetings, AI models and settings are all in one folder. Copy it to back everything up.</span></span>
    </li>
    <li class="row">
      <!-- Desert Ant's license asks for this credit; their SDK ships in every build, so it always shows. -->
      <span><span class="name">License</span><span class="desc">Free and open source (MIT license).<br /><span class="attrib">Voz speech-to-text <a href="https://desertant.com" onclick={(e) => { e.preventDefault(); invoke("open_link", { url: "https://desertant.com" }); }}>powered by Desert Ant Labs</a></span></span></span>
    </li>
  </ul>
  <p class="err" role="status">{shown}</p>
</section>

<style>
  .attrib { color: var(--text-3); }
  .attrib a { color: inherit; }
  .mono { font: 12px/var(--lh-body) var(--font-mono); overflow-wrap: anywhere; }
  .slot { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-2); width: 240px; height: var(--h-ctl); font-variant-numeric: tabular-nums; }
  /* Status pill (DESIGN.md); the alert kind uses the danger colours. */
  .pill { display: inline-flex; align-items: center; height: 20px; padding: 0 var(--sp-2); border-radius: var(--r-full); font-size: var(--fs-caption); font-weight: var(--fw-medium);
    background: var(--accent-soft); color: var(--accent-text); white-space: nowrap; }
  .pill.alert { background: var(--danger-soft); color: var(--danger-text); }
</style>
