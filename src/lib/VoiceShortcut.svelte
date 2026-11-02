<!-- Voice typing's switch, its shortcut recorder, and how it starts (prototype: the dictation page and `hotkeyRec`). -->
<script lang="ts" module>
  export type Voice = { on: boolean; key: string; mode: "hold" | "tap" };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { fromKey, keycaps, spoken } from "./shortcut";

  let { voice = $bindable() }: { voice: Voice } = $props();
  let recording = $state(false);
  let error = $state("");
  let btn = $state<HTMLButtonElement>();

  async function save(next: Voice) {
    const was = voice;
    voice = next;
    error = "";
    try {
      await invoke("voice_set", { voice: next });
    } catch (err) {
      voice = was;
      error = String(err);
    }
  }

  async function record(on: boolean) {
    if (recording === on) return;
    recording = on;
    error = "";
    await tick();
    btn?.focus();
    // While recording, the old shortcut must reach this page instead of starting voice typing.
    await invoke("voice_pause", { paused: on }).catch((err) => (error = String(err)));
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    if (e.key === "Escape") return void record(false);
    const key = fromKey(e);
    if (key === null) return;
    if (typeof key === "object") return void (error = key.error);
    recording = false;
    save({ ...voice, key }).then(() => btn?.focus());
  }
</script>

<!-- Leaving the window ends recording, so the shortcut never stays switched off. -->
<svelte:window {onkeydown} onblur={() => record(false)} />

<section class="group" aria-labelledby="voice-h">
  <h2 id="voice-h" class="sr">Voice typing</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="voice-on">Use voice typing</label><span class="desc">Hold a key, talk, and let go. Your words appear wherever you’re typing.</span></span>
      <input id="voice-on" type="checkbox" role="switch" class="switch" checked={voice.on} onchange={(e) => save({ ...voice, on: e.currentTarget.checked })} />
    </li>
  </ul>
  {#if !voice.on}<p class="err" role="status">{error}</p>{/if}
</section>

{#if voice.on}
  <section class="group" aria-labelledby="hk-h">
    <h2 id="hk-h">Shortcut</h2>
    <ul class="list">
      <li class="row">
        <span><span class="name">Shortcut key</span><span class="desc">{recording ? "Press the key or keys you want to use. Esc cancels." : "Works in any app."}</span></span>
        <!-- Same width either way, so the row doesn't move when recording starts. -->
        <span class="hk">
          {#if recording}
            <button class="btn live" bind:this={btn} aria-live="polite" onclick={() => record(false)}>Press a key…</button>
          {:else}
            <span class="keys" aria-label="Current shortcut: {spoken(voice.key)}">{#each keycaps(voice.key) as k}<kbd>{k}</kbd>{/each}</span>
            <button class="btn" bind:this={btn} onclick={() => record(true)}>Change</button>
          {/if}
        </span>
      </li>
      <li class="row">
        <span><span class="name" id="mode-l">How it starts</span><span class="desc">{voice.mode === "hold" ? "Hold the key while you talk. Let go to add your words." : "Press once to start and again to stop. Stops by itself after 5 minutes."}</span></span>
        <fieldset class="seg" aria-labelledby="mode-l">
          {#each [["hold", "Hold to talk"], ["tap", "Tap to start and stop"]] as [v, label]}
            <label><input type="radio" name="mode" value={v} checked={voice.mode === v} onchange={() => save({ ...voice, mode: v as Voice["mode"] })} /><span>{label}</span></label>
          {/each}
        </fieldset>
      </li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{/if}

<style>
  .group { margin-bottom: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-3); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .list > li + li { border-top: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  .hk { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-2); min-width: 180px; }
  .keys { display: inline-flex; gap: var(--sp-1); }
  kbd { min-width: 26px; height: 26px; padding: 0 var(--sp-2); box-sizing: border-box; display: inline-grid; place-items: center;
    border-radius: var(--r-md); background: var(--fill-field); box-shadow: inset 0 0 0 .5px var(--line-strong); font: inherit; }
  .btn { display: inline-flex; align-items: center; justify-content: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); color: var(--text); font: inherit; box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); white-space: nowrap; }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn.live { min-width: 150px; color: var(--accent-text); box-shadow: inset 0 0 0 2px var(--accent); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Switch, 38×22 (DESIGN.md). */
  .switch { appearance: none; width: 38px; height: 22px; margin: 0; border-radius: 11px; position: relative; flex: none; background: var(--fill-track);
    transition: background var(--dur) var(--ease-out); }
  .switch::after { content: ""; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, .3); transition: transform var(--dur) var(--ease-out); }
  .switch:checked { background: var(--accent); }
  .switch:checked::after { transform: translateX(16px); }
  .switch:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Segmented control: a capsule track with a raised thumb. */
  .seg { display: inline-flex; gap: 2px; margin: 0; padding: 3px; border: 0; border-radius: var(--r-full); background: var(--fill-field); }
  .seg label { position: relative; }
  .seg input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .seg span { display: flex; align-items: center; height: 26px; padding: 0 14px; border-radius: var(--r-full); color: var(--text-2); white-space: nowrap; }
  .seg input:checked + span { background: var(--fill-control-on); color: var(--text); font-weight: var(--fw-medium); box-shadow: 0 0 0 .5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, .1); }
  .seg input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
