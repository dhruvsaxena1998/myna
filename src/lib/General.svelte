<!-- General (docs/settings.html, VIEWS.general): Appearance, Updates, Your data, the experimental switch, the license credit. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { saveLook, type Look } from "./look";
  import { upd, watchUpdates, type Prefs, type UpdateState } from "./updates.svelte";

  // `look` is shared with the Experimental page, so neither saves a stale copy of the other's choice.
  let { look = $bindable(), version, path, prefs: initial, status }: { look: Look; version: string; path: string; prefs: Prefs; status: UpdateState } = $props();
  let prefs = $derived(initial); // assignable: a pick shows at once, a failed save reverts it
  let errors = $state({ look: "", upd: "", data: "", exp: "" });
  onMount(() => watchUpdates(status));

  const THEMES = [["auto", "Auto"], ["light", "Light"], ["dark", "Dark"]];
  const ACCENTS = ["blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite"];
  const GLASSES = [["clear", "Clear"], ["tinted", "Tinted"]];
  const MODES: [Prefs["mode"], string, string][] = [
    ["auto", "Check and download", "Downloads in the background, then asks you to restart."],
    ["check", "Check only", "Tells you when there’s an update. You choose when to download."],
    ["off", "Off", "Only checks when you click Check for updates."],
  ];
  const EVERY: [Prefs["every"], string, string][] = [
    ["launch", "At launch and daily", "Checks each time Myna opens, and once a day while it’s open."],
    ["day", "Daily", "Checks once a day, even if you quit and reopen Myna."],
    ["week", "Weekly", "Checks once a week, even if you quit and reopen Myna."],
  ];
  const CHANNELS = [["stable", "Stable"], ["beta", "Beta"]];

  async function saveOne(change: Partial<Look>, what: string, slot: "look" | "exp") {
    const was = look;
    look = { ...look, ...change };
    errors[slot] = "";
    errors[slot] = await saveLook(was, look, what);
    if (errors[slot]) look = was;
  }

  async function savePrefs(change: Partial<Prefs>) {
    const was = prefs;
    prefs = { ...prefs, ...change };
    errors.upd = "";
    try {
      await invoke("updates_set", { prefs });
    } catch (e) {
      prefs = was;
      errors.upd = String(e);
    }
  }

  // The check reports through the update state, and opens the update pop-up with the result.
  const check = () => invoke("update_check").catch(() => {});
  const open = () => invoke("update_open");
  // A failed update says so once, on the section's error line.
  let updError = $derived(errors.upd || (upd.s.state === "error" ? upd.s.message : ""));
  let ready = $derived(upd.s.state === "available" || upd.s.state === "downloading" || upd.s.state === "downloaded" ? upd.s.version : "");
  const note = $derived(
    upd.s.state === "uptodate" ? " · Up to date"
    : upd.s.state === "downloaded" ? ` · ${ready} is ready`
    : ready ? ` · ${ready} is available` : "",
  );

  const modeDesc = $derived(MODES.find(([v]) => v === prefs.mode)?.[2]);
  const everyDesc = $derived(EVERY.find(([v]) => v === prefs.every)?.[2]);
  const expDesc = $derived(look.experimental ? "Features that are still changing. They’re on the Experimental page." : "Features that are still changing. Off turns them all off.");

  async function reveal() {
    errors.data = "";
    try {
      await invoke("storage_reveal");
    } catch (e) {
      errors.data = String(e);
    }
  }
</script>

{#snippet seg(name: string, choices: string[][], value: string, labelledby: string, change: (v: string) => void)}
  <fieldset class="seg" aria-labelledby={labelledby}>
    {#each choices as [v, label]}
      <label><input type="radio" {name} value={v} checked={value === v} onchange={() => change(v)} /><span>{label}</span></label>
    {/each}
  </fieldset>
{/snippet}

<section aria-labelledby="appearance-h">
  <h2 id="appearance-h">Appearance</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name" id="theme-l">Theme</span><span class="desc">Auto follows your Mac.</span></span>
      {@render seg("theme", THEMES, look.theme, "theme-l", (v) => saveOne({ theme: v as Look["theme"] }, "the theme", "look"))}
    </li>
    <li class="row">
      <span><span class="name" id="accent-l">Accent colour</span><span class="desc">Buttons, switches and selection.</span></span>
      <fieldset class="swatches" aria-labelledby="accent-l">
        {#each ACCENTS as v}
          {@const label = v[0].toUpperCase() + v.slice(1)}
          <label class="sw" title={label} style="--c: var(--sw-{v})"><input type="radio" name="accent" value={v} checked={look.accent === v} onchange={() => saveOne({ accent: v }, "the accent colour", "look")} /><span class="dot" aria-hidden="true"><span class="pip"></span></span><span class="sr-only">{label}</span></label>
        {/each}
      </fieldset>
    </li>
    <li class="row">
      <span><span class="name" id="glass-l">Window tint</span><span class="desc">{look.glass === "tinted" ? "The sidebar takes a hint of your highlight colour." : "The sidebar shows your desktop through it."}</span></span>
      {@render seg("glass", GLASSES, look.glass, "glass-l", (v) => saveOne({ glass: v as Look["glass"] }, "the window tint", "look"))}
    </li>
  </ul>
  <p class="err" role="status">{errors.look}</p>
</section>

<section aria-labelledby="updates-h">
  <h2 id="updates-h">Updates</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name">Myna</span><span class="desc">Version {version}{#if import.meta.env.VITE_BUILD}{" · "}{import.meta.env.VITE_BUILD}{/if}{note}</span></span>
      <!-- One button of one width in every state, so nothing moves the row. -->
      <div class="end">
        {#if upd.s.state === "checking"}<button class="btn" style="min-width: 148px" disabled>Checking…</button>
        {:else if upd.s.state === "available"}<button class="btn primary" style="min-width: 148px" onclick={open}>View update</button>
        {:else if upd.s.state === "downloading"}<button class="btn" style="min-width: 148px" disabled>Downloading {upd.s.pct}%</button>
        {:else if upd.s.state === "downloaded"}<button class="btn primary" style="min-width: 148px" onclick={open}>Restart to update</button>
        {:else if upd.s.state === "error"}<button class="btn" style="min-width: 148px" onclick={check}>Try again</button>
        {:else}<button class="btn" style="min-width: 148px" onclick={check}>Check for updates</button>{/if}
      </div>
    </li>
    <li class="row">
      <span><label class="name" for="upd-mode">Update mode</label><span class="desc" id="upd-mode-d" title={modeDesc}>{modeDesc}</span></span>
      <select id="upd-mode" class="select" aria-describedby="upd-mode-d" value={prefs.mode} onchange={(e) => savePrefs({ mode: e.currentTarget.value as Prefs["mode"] })}>
        {#each MODES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    {#if prefs.mode !== "off"}
      <li class="row">
        <span><label class="name" for="upd-every">Check frequency</label><span class="desc" id="upd-every-d" title={everyDesc}>{everyDesc}</span></span>
        <select id="upd-every" class="select" aria-describedby="upd-every-d" value={prefs.every} onchange={(e) => savePrefs({ every: e.currentTarget.value as Prefs["every"] })}>
          {#each EVERY as [v, label]}<option value={v}>{label}</option>{/each}
        </select>
      </li>
    {/if}
    <li class="row">
      <span><span class="name" id="chan-l">Update channel</span><span class="desc">{prefs.channel === "beta" ? "Gets pre-releases as well as stable releases." : "Stable releases only. A newer beta stays until stable passes it."}</span></span>
      {@render seg("channel", CHANNELS, prefs.channel, "chan-l", (v) => savePrefs({ channel: v as Prefs["channel"] }))}
    </li>
  </ul>
  <p class="err" role="status">{updError}</p>
</section>

<section aria-labelledby="data-h">
  <h2 id="data-h">Your data</h2>
  <p class="help">Meetings, models and settings. Copy this folder to back them up.</p>
  <ul class="list">
    <li class="row">
      <span><span class="name">Data folder</span><span class="desc" title={path}><span class="mono">{path}</span></span></span>
      <button class="btn" onclick={reveal}>Show in Finder</button>
    </li>
  </ul>
  <p class="err" role="status">{errors.data}</p>
</section>

<section aria-labelledby="exp-h">
  <h2 id="exp-h">Experimental</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="exp-on">Use experimental features</label><span class="desc" id="exp-d" title={expDesc}>{expDesc}</span></span>
      <input type="checkbox" role="switch" class="switch" id="exp-on" aria-describedby="exp-d" checked={look.experimental} onchange={(e) => saveOne({ experimental: e.currentTarget.checked }, "experimental features", "exp")} />
    </li>
  </ul>
  <p class="err" role="status">{errors.exp}</p>
</section>

<!-- Desert Ant's license asks for this credit; their SDK ships in every build, so it always shows. -->
<p class="foot">Free and open source, MIT license. Voz speech-to-text and Who speaker diarization <a href="https://desertant.com" onclick={(e) => { e.preventDefault(); invoke("open_link", { url: "https://desertant.com" }); }}>powered by Desert Ant Labs</a>.</p>
