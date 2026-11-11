<!-- Voice typing's switch, its two shortcut recorders (Dictate, Dictate with cleanup), and how they start (prototype: the dictation page, `hotkeyRec` and `cleanRowHelp`). -->
<script lang="ts" module>
  /** `key2` and `clean2`: an optional second key for each action, e.g. one on another keyboard. */
  export type Voice = { on: boolean; key: string; clean: string | null; key2?: string | null; clean2?: string | null; rate?: boolean; sounds?: boolean; mode: "hold" | "tap"; paste: "clipboard" | "type" };
  /** Which shortcut the recorder is changing. */
  type Which = "key" | "key2" | "clean" | "clean2";
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";
  import HostPill from "./HostPill.svelte";
  import type { Ai } from "./AiProviders.svelte";
  import { fromKey, keycaps, spoken } from "./shortcut";

  // `globe`: the 🌐 key is set to Do Nothing, so fn only starts voice typing.
  // `ai`: the providers and the Fast model, which Dictate with cleanup needs; `setup` opens the AI provider page.
  let { voice = $bindable(), globe: initial, ai, setup }: { voice: Voice; globe: boolean; ai: Ai | null; setup: () => void } = $props();
  const fast = $derived(ai?.fast && !ai.fast.gone ? { route: ai.fast, provider: ai.providers.find((p) => p.id === ai.fast!.provider) } : null);
  const LABEL: Record<Which, string> = { key: "Dictate", key2: "Dictate", clean: "Dictate with cleanup", clean2: "Dictate with cleanup" };
  const WHICH: Which[] = ["key", "key2", "clean", "clean2"];
  let globe = $derived(initial); // assignable: refreshed on focus
  onMount(() => {
    // The browser never sees fn; the backend tells us when it’s let go while recording.
    const offFn = listen("voice-fn", () => recording && pick("Fn"));
    // Changed in System Settings, then back to Myna.
    const offFocus = getCurrentWindow().onFocusChanged(async ({ payload }) => payload && (globe = await invoke<boolean>("globe_ok")));
    return () => [offFn, offFocus].forEach((off) => void off.then((f) => f()));
  });
  let recording = $state<Which | null>(null);
  let error = $state("");
  const btns = $state<Partial<Record<Which, HTMLButtonElement>>>({});

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

  // Removing the first cleanup key moves the second one up, so the first is always the one set.
  const remove = (which: Which) => save(which === "clean" ? { ...voice, clean: voice.clean2 ?? null, clean2: null } : { ...voice, [which]: null });

  async function record(which: Which | null) {
    if (recording === which) return;
    const was = recording;
    recording = which;
    error = "";
    await tick();
    btns[which ?? was!]?.focus();
    // While recording, the old shortcuts must reach this page instead of starting voice typing.
    if ((was === null) !== (which === null)) await invoke("voice_pause", { paused: which !== null }).catch((err) => (error = String(err)));
  }

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    if (e.key === "Escape") return void record(null);
    const key = fromKey(e);
    if (key === null) return;
    if (typeof key === "object") return void (error = key.error);
    pick(key);
  }

  function pick(key: string) {
    const which = recording!;
    recording = null;
    const other = WHICH.find((w) => w !== which && voice[w] === key);
    if (other) {
      error = `${spoken(key)} is already the ${LABEL[other]} shortcut. Pick another key.`;
      invoke("voice_pause", { paused: false }).catch(() => {});
      return void tick().then(() => btns[which]?.focus());
    }
    save({ ...voice, [which]: key }).then(() => btns[which]?.focus());
  }
</script>

<!-- Leaving the window ends recording, so the shortcut never stays switched off. -->
<svelte:window {onkeydown} onblur={() => record(null)} />

<section aria-labelledby="voice-h">
  <h2 id="voice-h" class="sr-only">Voice typing</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="voice-on">Use voice typing</label><span class="desc">Hold a key, talk, and let go. Your words appear wherever you’re typing.</span></span>
      <input id="voice-on" type="checkbox" role="switch" class="switch" checked={voice.on} onchange={(e) => save({ ...voice, on: e.currentTarget.checked })} />
    </li>
  </ul>
  {#if !voice.on}<p class="err" role="status">{error}</p>{/if}
</section>

{#if voice.on}
  <section aria-labelledby="hk-h">
    <h2 id="hk-h">Shortcuts</h2>
    <ul class="list">
      <li class="row">
        <span><span class="name">Dictate</span><span class="desc">{recording === "key" || recording === "key2" ? "Press the key or keys you want to use. Esc cancels." : "Adds what you said. Works in any app."}</span></span>
        <span class="two">{@render recorder("key")}{@render second("key2")}</span>
      </li>
      <li class="row">
        <span><span class="name">Dictate with cleanup</span>
          <span class="desc">
            {#if recording === "clean" || recording === "clean2"}Press the key or keys you want to use. Esc cancels.
            {:else if !fast}Fixes punctuation, grammar slips and self-corrections with AI before adding your words.<br /><span class="pill need">Needs an Endpoint</span> Set a Fast model on AI provider first.
            {:else}Fixes punctuation, grammar slips and self-corrections before adding your words. If that takes over 3 s, they go in as they are.
              <span class="fast"><span class="model">{fast.provider?.name ?? "Unknown provider"} · <code>{fast.route.model}</code></span><HostPill off={fast.provider?.off_device ?? true} /></span>{/if}
          </span></span>
        {#if fast}<span class="two">{@render recorder("clean")}{#if voice.clean}{@render second("clean2")}{/if}</span>{:else}<button class="btn" onclick={setup}>Set up</button>{/if}
      </li>
      {#if WHICH.some((w) => voice[w] === "Fn") && !globe}
        <li class="row">
          <span><span class="name">Set the 🌐 key to Do Nothing</span><span class="desc">Otherwise fn also does what your Mac set it to, like showing emoji. In Keyboard settings, choose Do Nothing for “Press 🌐 key to” (on some keyboards, “Press fn key to”).</span></span>
          <button class="btn" onclick={() => invoke("globe_open")}>Open Keyboard Settings</button>
        </li>
      {/if}
      <li class="row">
        <span><span class="name" id="mode-l">How it starts</span><span class="desc">{voice.mode === "hold" ? "Hold the key while you talk. Let go to add your words." : "Press once to start and again to stop. Stops by itself after 5 minutes."} Same for both shortcuts. Esc stops without adding your words.</span></span>
        <fieldset class="seg" aria-labelledby="mode-l">
          {#each [["hold", "Hold to talk"], ["tap", "Tap to start and stop"]] as [v, label]}
            <label><input type="radio" name="mode" value={v} checked={voice.mode === v} onchange={() => save({ ...voice, mode: v as Voice["mode"] })} /><span>{label}</span></label>
          {/each}
        </fieldset>
      </li>
      <li class="row">
        <span><label class="name" for="voice-sounds">Sounds</label><span class="desc">Plays a sound when Myna starts and stops listening.</span></span>
        <input id="voice-sounds" type="checkbox" role="switch" class="switch" checked={voice.sounds ?? true} onchange={(e) => save({ ...voice, sounds: e.currentTarget.checked })} />
      </li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{/if}

<!-- Same width either way, so the row doesn't move when recording starts. -->
{#snippet recorder(which: Which)}
  {@const key = voice[which]}
  <span class="hk">
    {#if recording === which}
      <button class="btn live" bind:this={btns[which]} aria-live="polite" onclick={() => record(null)}>Press a key…</button>
    {:else}
      {#if key}<span class="keys" aria-label="Current shortcut: {spoken(key)}">{#each keycaps(key) as k}<kbd>{k}</kbd>{/each}</span>
      {:else}<span class="keys" aria-label="No shortcut"><kbd class="empty">—</kbd></span>{/if}
      <button class="btn" bind:this={btns[which]} onclick={() => record(which)}>{key ? "Change" : "Set shortcut"}</button>
      {#if key && which !== "key"}<button class="btn quiet icon" title="Remove" onclick={() => remove(which)} aria-label="Remove the {which.endsWith('2') ? 'second ' : ''}{LABEL[which]} shortcut"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button>{/if}
    {/if}
  </span>
{/snippet}

<!-- The optional second key: "Add another key" until one is set. -->
{#snippet second(which: "key2" | "clean2")}
  {#if voice[which] || recording === which}{@render recorder(which)}
  {:else}<span class="hk"><button class="btn quiet" bind:this={btns[which]} onclick={() => record(which)}>Add another key</button></span>{/if}
{/snippet}

<style>
  .two { display: flex; flex-direction: column; align-items: flex-end; gap: var(--sp-2); }
  .hk { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-2); min-width: 180px; }
  .keys { display: inline-flex; gap: var(--sp-1); }
  kbd.empty { color: var(--text-3); }
  /* The Fast model on its own line, beside where its text goes. */
  .fast { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2); margin-top: var(--sp-1); }
  .model { font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-3); }
  .model code { font: 12px var(--font-mono); }
  .fast :global(.pill) { margin: 0; }
  .need { margin-top: var(--sp-1); }
  .desc :global(.pill) { margin-top: var(--sp-1); }
  kbd { min-width: 26px; height: 26px; padding: 0 var(--sp-2); box-sizing: border-box; display: inline-grid; place-items: center;
    border-radius: var(--r-md); background: var(--fill-field); box-shadow: inset 0 0 0 .5px var(--line-strong); font: inherit; }
  .ic { width: 14px; height: 14px; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; }
  .btn.live { min-width: 150px; color: var(--accent-text); box-shadow: inset 0 0 0 2px var(--accent); }
</style>
