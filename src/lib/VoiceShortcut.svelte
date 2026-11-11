<!-- Listen's rows: the switch, Microphone, and Shortcuts (docs/settings.html, VIEWS.listen). -->
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
  import Permissions, { type Perms } from "./Permissions.svelte";
  import type { Ai } from "./AiProviders.svelte";
  import { fromKey, keycaps, spoken } from "./shortcut";

  // `globe`: the 🌐 key is set to Do Nothing, so fn only starts voice typing.
  // `ai`: the Fast model, which Dictate with cleanup needs; `setup` opens the AI providers page.
  let { voice = $bindable(), globe: initial, ai, perms, setup }: { voice: Voice; globe: boolean; ai: Ai | null; perms: Perms; setup: () => void } = $props();
  const fast = $derived(!!ai?.fast && !ai.fast.gone);
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
  const PRESS = "Press the key or keys you want to use. Esc cancels.";

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
      <span><label class="name" for="voice-on">Use voice typing</label>{@render desc("Hold a key, talk, and let go. Your words appear wherever you’re typing.")}</span>
      <input id="voice-on" type="checkbox" role="switch" class="switch" checked={voice.on} onchange={(e) => save({ ...voice, on: e.currentTarget.checked })} />
    </li>
    {#if voice.on}<Permissions initial={perms} kinds={["mic"]} rows />{/if}
  </ul>
  {#if !voice.on}<p class="err" role="status">{error}</p>{/if}
</section>

{#if voice.on}
  <section aria-labelledby="hk-h">
    <h2 id="hk-h">Shortcuts</h2>
    <ul class="list">
      <li class="row wide">
        <span><span class="name">Dictate</span>{@render desc(recording === "key" || recording === "key2" ? PRESS : "Adds what you said. Works in any app.")}</span>
        <span class="end">{@render recorder("key")}{@render second("key2")}</span>
      </li>
      <li class="row" class:wide={fast}>
        <span><span class="name">Dictate with cleanup</span>
          {@render desc(recording === "clean" || recording === "clean2" ? PRESS : fast ? "Fixes punctuation, grammar slips and self-corrections." : "Fixes punctuation, grammar slips and self-corrections. Needs a Fast model.")}</span>
        {#if fast}
          <span class="end">
            {#if voice.clean}<button class="btn quiet icon" title="Remove" onclick={() => remove("clean")} aria-label="Remove the Dictate with cleanup shortcut">{@render x()}</button>{/if}
            {@render recorder("clean")}{@render second("clean2")}
          </span>
        {:else}<span class="end"><button class="btn" onclick={setup}>Set up</button></span>{/if}
      </li>
      {#if WHICH.some((w) => voice[w] === "Fn") && !globe}
        <li class="row">
          <span><span class="name">Set <kbd>fn</kbd> to Do Nothing</span>{@render desc("In Keyboard settings, set Press 🌐 key to Do Nothing.")}</span>
          <span class="end"><button class="btn" onclick={() => invoke("globe_open")}>Open Keyboard Settings</button></span>
        </li>
      {/if}
      <li class="row">
        <span><span class="name" id="mode-l">Activation</span>{@render desc(voice.mode === "hold" ? "Hold while you talk, let go to add. Esc cancels." : "Press to start, again to stop. Stops after 5 minutes. Esc cancels.")}</span>
        <fieldset class="seg" aria-labelledby="mode-l">
          {#each [["hold", "Hold"], ["tap", "Tap"]] as [v, label]}
            <label><input type="radio" name="mode" value={v} checked={voice.mode === v} onchange={() => save({ ...voice, mode: v as Voice["mode"] })} /><span>{label}</span></label>
          {/each}
        </fieldset>
      </li>
      <li class="row">
        <span><label class="name" for="voice-sounds">Start and stop sounds</label>{@render desc("Plays a sound when Myna starts and stops listening.")}</span>
        <input id="voice-sounds" type="checkbox" role="switch" class="switch" checked={voice.sounds ?? true} onchange={(e) => save({ ...voice, sounds: e.currentTarget.checked })} />
      </li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{/if}

{#snippet desc(text: string)}<span class="desc" title={text}>{text}</span>{/snippet}
{#snippet x()}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg>{/snippet}

<!-- The keycaps sit inside the button, so one control shows the key and changes it. -->
{#snippet recorder(which: Which)}
  {@const key = voice[which]}
  {@const nth = which.endsWith("2") ? "second " : ""}
  <button class="btn" class:rec={recording === which} bind:this={btns[which]} aria-live={recording === which ? "polite" : undefined}
    aria-label={recording === which ? undefined : key ? `${nth}${LABEL[which]} shortcut: ${spoken(key)}` : `Set the ${nth}${LABEL[which]} shortcut`}
    onclick={() => record(recording === which ? null : which)}>
    {#if recording === which}Press keys…
    {:else if key}<span class="keys">{#each keycaps(key) as k}<kbd>{k}</kbd>{/each}</span>
    {:else}Set key{/if}
  </button>
{/snippet}

<!-- The optional second key: a quiet + until one is set, then its own recorder and a quiet ×. The slot at the right never changes size. -->
{#snippet second(which: "key2" | "clean2")}
  {#if voice[which] || recording === which}
    {@render recorder(which)}
    <button class="btn quiet icon" title="Remove" aria-label={voice[which] ? `Remove the second ${LABEL[which]} shortcut` : "Cancel"} onclick={() => (voice[which] ? remove(which) : record(null))}>{@render x()}</button>
  {:else}
    <button class="btn quiet icon" title="Add another key" aria-label="Add another key" disabled={which === "clean2" && !voice.clean} onclick={() => record(which)}>
      <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
    </button>
  {/if}
{/snippet}

<style>
  /* Recording: the focus ring sits inside the button, so it never changes its size. */
  .btn.rec { box-shadow: inset 0 0 0 2px var(--focus-ring); color: var(--text-2); }
</style>
