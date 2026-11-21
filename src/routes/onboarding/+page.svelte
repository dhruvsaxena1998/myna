<!-- First-run setup (story 02): what to help with, permissions, models, the voice typing key and a try, the browser add-on, then what to try.
     Opens at the first launch until Done, and from Settings › General › Set up Myna again. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { invalidateAll } from "$app/navigation";
  import { onMount } from "svelte";
  import Permissions from "$lib/Permissions.svelte";
  import BrowserAddon from "$lib/BrowserAddon.svelte";
  import ShortcutButton from "$lib/ShortcutButton.svelte";
  import { PRESS, Recorder } from "$lib/recorder.svelte";
  import { keycaps } from "$lib/shortcut";
  import ModelList from "$lib/ModelList.svelte";
  import { watchDownloads } from "$lib/downloads.svelte";

  let { data } = $props();
  let voice = $derived(data.voice); // assignable: saved picks show at once
  let notes = $derived(data.notes);
  let globe = $derived(data.globe);
  let connected = $derived(data.addon.connected);
  let code = $derived(data.addon.code);
  let perms = $derived(data.perms); // assignable: kept across steps, so Back shows what's already allowed
  // Both on by default; Settings can turn either off later.
  let useVoice = $state(true), useMeet = $state(true);
  let step = $state(0);
  let error = $state("");

  type Step = "what" | "perms" | "key" | "models" | "addon" | "done";
  const steps = $derived<Step[]>(["what", "perms", "models", ...(useVoice ? ["key" as const] : []), ...(useMeet ? ["addon" as const] : []), "done"]);
  const at = $derived(steps[step]);

  onMount(() => {
    watchDownloads();
    const offs = [
      // Settings › Set up Myna again: start over with what's saved now.
      listen("onboarding-start", async () => {
        await invalidateAll();
        step = 0;
        error = "";
        heard = false;
        tried = "";
      }),
      // A finished download can become the model in use; the lists show it.
      listen<{ state: string }>("model-state", (e) => e.payload.state === "done" && invalidateAll()),
      // The key test: the key started listening.
      listen<{ state: string }>("dictation", (e) => e.payload.state === "listening" && (heard = true)),
      listen<boolean>("addon-status", (e) => (connected = e.payload)),
      getCurrentWindow().onFocusChanged(async ({ payload }) => payload && (globe = await invoke<boolean>("globe_ok"))),
    ];
    return () => offs.forEach((o) => void o.then((f) => f()));
  });

  // Each step saves as it goes, so closing the window part way keeps what was picked.
  async function next() {
    error = "";
    try {
      if (at === "what") {
        await invoke("voice_set", { voice: (voice = { ...voice, on: useVoice }) });
        await invoke("meeting_set", { notes: (notes = { ...notes, on: useMeet }) });
        // The add-on's code exists once meeting notes are on.
        if (useMeet) ({ connected, code } = await invoke<{ connected: boolean; code: string }>("addon_get"));
      }
      if (at === "done") return void (await invoke("onboarding_done"));
      step++;
    } catch (e) {
      error = String(e);
    }
  }

  const rec = new Recorder<"key">({
    fn: true,
    save: async (_, key) => {
      try {
        await invoke("voice_set", { voice: { ...voice, key } });
        voice = { ...voice, key };
      } catch (e) {
        rec.error = String(e);
      }
    },
  });

  // Try it: the key starts listening (`heard`), and the words land in the box (`tried`) close to the phrase.
  const PHRASE = "Hello, this is my first dictation.";
  let heard = $state(false), tried = $state("");
  const words = (t: string) => t.toLowerCase().replace(/[^\p{L}\p{N}\s]/gu, " ").split(/\s+/).filter(Boolean);
  const close = $derived.by(() => {
    const got = words(tried), want = words(PHRASE);
    return want.filter((w) => got.includes(w)).length >= want.length * 0.8;
  });

  // Every model, the recommended one marked, as in Settings; the pick is saved as it's made.
  const stt = $derived(data.tasks.find((t) => t.id === "stt"));
  const diar = $derived(data.tasks.find((t) => t.id === "diar"));
  let sttActive = $derived(stt?.active ?? null); // assignable: ModelList binds it
  let diarActive = $derived(diar?.active ?? null);
  const apple = $derived(stt?.models.find((m) => m.system && !m.needs));
</script>

{#snippet mark(ok: boolean)}{#if ok}<span class="ok"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>Works</span>{/if}{/snippet}

<svelte:window onkeydown={rec.onkeydown} onblur={() => rec.record(null)} />

<div class="window" role="dialog" aria-labelledby="ob-h">
  <div class="titlebar" data-tauri-drag-region>Set up Myna</div>
  <div class="panel">
    <div class="body prefs">
      {#if at === "what"}
        <header><h1 id="ob-h">Welcome to Myna</h1><p class="lead">What should Myna help you with? You can change this later in Settings.</p></header>
        <ul class="list">
          <li class="row">
            <span><label class="name" for="ob-voice">Voice typing</label><span class="desc">Hold a key, talk, and let go. Your words appear wherever you’re typing.</span></span>
            <input id="ob-voice" type="checkbox" role="switch" class="switch" bind:checked={useVoice} />
          </li>
          <li class="row">
            <span><label class="name" for="ob-meet">Meeting notes</label><span class="desc">Records your calls, then writes the transcript and a summary.</span></span>
            <input id="ob-meet" type="checkbox" role="switch" class="switch" bind:checked={useMeet} />
          </li>
        </ul>
      {:else if at === "perms"}
        <header><h1 id="ob-h">Allow access</h1><p class="lead">Your Mac asks once for each. For Accessibility, switch Myna on in System Settings, then come back.</p></header>
        <ul class="list"><Permissions bind:initial={perms} kinds={[...(useVoice || useMeet ? ["mic" as const] : []), ...(useVoice ? ["ax" as const] : []), ...(useMeet ? ["audio" as const] : [])]} rows /></ul>
      {:else if at === "key"}
        <header><h1 id="ob-h">Your voice typing key</h1><p class="lead">Hold it, talk, and let go. Click the key to pick another one; <kbd>fn</kbd> works too.</p></header>
        <ul class="list">
          <li class="row">
            <span><span class="name">Dictate</span><span class="desc">{rec.recording ? PRESS : "Adds what you said. Works in any app."}</span></span>
            <ShortcutButton id="rec-key" name="Dictate shortcut" key={voice.key} recording={rec.recording === "key"} onclick={() => rec.record(rec.recording ? null : "key")} />
          </li>
          {#if voice.key === "Fn" && !globe}
            <li class="row">
              <span><span class="name">Set <kbd>fn</kbd> to Do Nothing</span><span class="desc">In Keyboard settings, set Press 🌐 key to Do Nothing.</span></span>
              <button class="btn" onclick={() => invoke("globe_open")}>Open Keyboard Settings</button>
            </li>
          {/if}
        </ul>
        <p class="err" role="status">{rec.error}</p>
        <section aria-labelledby="try-h">
          <h2 id="try-h">Try it</h2>
          <p class="help">Click the box, {voice.mode === "hold" ? "hold" : "press"} your key, say “{PHRASE}”, then {voice.mode === "hold" ? "let go" : "press it again"}.</p>
          <textarea class="input try" rows="2" bind:value={tried} placeholder="Your words appear here" aria-label="Try voice typing here"></textarea>
          <ul class="list">
            <li class="row"><span><span class="name">Key</span><span class="desc">{heard ? "Myna heard your key." : "Waiting for your key…"}</span></span>{@render mark(heard)}</li>
            <li class="row"><span><span class="name">Words</span><span class="desc" title={tried}>{!tried.trim() ? "Waiting for your words…" : close ? "That’s what you said." : `Got “${tried.trim()}”. Try again, or go Back to pick another model.`}</span></span>{@render mark(close)}</li>
          </ul>
        </section>
      {:else if at === "models"}
        <header><h1 id="ob-h">Speech models</h1><p class="lead">They run on this Mac. Download one and pick it; you can carry on while it downloads.{#if apple && useVoice}{" "}Until then, voice typing uses Apple Speech.{/if}</p></header>
        {#if stt}<ModelList task={stt} label="Speech-to-text model" help="Turns speech into text, for voice typing and meeting notes." bind:active={sttActive} search={false} advanced={false} />{/if}
        {#if diar && useMeet}<ModelList task={diar} label="Speaker diarization model" help="Works out who said what in a meeting." bind:active={diarActive} search={false} advanced={false} />{/if}
      {:else if at === "addon"}
        <header><h1 id="ob-h">Calls in Chrome</h1><p class="lead">Optional. With the add-on, Myna sees when you’re muted in Teams or Meet and who is talking. You can connect it later in Settings.</p></header>
        <BrowserAddon bind:connected bind:code loadError="" />
      {:else}
        <header><h1 id="ob-h">You’re set</h1><p class="lead">Try it now.</p></header>
        <ul class="list">
          {#if useVoice}
            <li class="row"><span><span class="name">Voice typing</span><span class="desc">Click into any text field, {voice.mode === "hold" ? "hold" : "press"} <span class="keys">{#each keycaps(voice.key) as k}<kbd>{k}</kbd>{/each}</span> and talk.</span></span></li>
          {/if}
          {#if useMeet}
            <li class="row"><span><span class="name">Meeting notes</span><span class="desc">Start from the Myna icon in the menu bar{#if notes.key}, or press <span class="keys">{#each keycaps(notes.key) as k}<kbd>{k}</kbd>{/each}</span>{/if}. Myna also offers to record when a call starts.</span></span></li>
          {/if}
          <li class="row"><span><span class="name">Settings</span><span class="desc">Everything here, and more, is in Settings from the menu bar icon.</span></span></li>
        </ul>
      {/if}
      <p class="err" role="status">{error}</p>
    </div>
    <footer>
      <span class="help fill">Step {step + 1} of {steps.length}</span>
      {#if step > 0 && at !== "done"}<button class="btn" onclick={() => (step--, (error = ""))}>Back</button>{/if}
      <button class="btn primary" disabled={at === "what" && !useVoice && !useMeet} onclick={next}>{at === "done" ? "Done" : "Continue"}</button>
    </footer>
  </div>
</div>

<style>
  .window { display: grid; grid-template-rows: var(--h-titlebar) minmax(0, 1fr); height: 100vh; box-sizing: border-box; padding: 0 var(--win-inset) var(--win-inset); }
  /* Left of the title: the traffic lights. */
  .titlebar { display: flex; align-items: center; padding-left: 76px; font-weight: var(--fw-semibold); color: var(--text-2); }
  .panel { display: grid; grid-template-rows: minmax(0, 1fr) 56px; min-height: 0; border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: hidden; }
  .body { min-height: 0; overflow-y: auto; padding: var(--sp-5); gap: var(--sp-4); }
  h1 { margin: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  .lead { margin: var(--sp-1) 0 0; color: var(--text-2); }
  .body > .err:last-child { margin-top: auto; }
  footer { display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-5); border-top: 1px solid var(--line); }
  .fill { flex: 1 1 0; min-width: 0; }
  .help { color: var(--text-3); font-size: var(--fs-footnote); }
  .keys { display: inline-flex; gap: 2px; }
  .try { width: 100%; box-sizing: border-box; margin-bottom: var(--sp-2); resize: none; user-select: text; }
</style>
