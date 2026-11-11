<!-- Listen's rows: the switch, Microphone, and Shortcuts (docs/settings.html, VIEWS.listen). -->
<script lang="ts" module>
  /** `key2` and `clean2`: an optional second key for each action, e.g. one on another keyboard. */
  export type Voice = { on: boolean; key: string; clean: string | null; key2?: string | null; clean2?: string | null; rate?: boolean; sounds?: boolean; mode: "hold" | "tap"; paste: "clipboard" | "type" };
  /** Which shortcut the recorder is changing. */
  type Which = "key" | "key2" | "clean" | "clean2";
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import Permissions, { type Perms } from "./Permissions.svelte";
  import type { Ai } from "./AiProviders.svelte";
  import { clash } from "./shortcut";
  import { PRESS, Recorder } from "./recorder.svelte";
  import ShortcutButton from "./ShortcutButton.svelte";

  // `globe`: the 🌐 key is set to Do Nothing, so fn only starts voice typing.
  // `ai`: the Fast model, which Dictate with cleanup needs; `setup` opens the AI providers page.
  // `meetKey`: Meeting notes' shortcut, which no voice typing key can match.
  let { voice = $bindable(), globe: initial, ai, perms, setup, meetKey }: { voice: Voice; globe: boolean; ai: Ai | null; perms: Perms; setup: () => void; meetKey: string } = $props();
  const fast = $derived(!!ai?.fast && !ai.fast.gone);
  const LABEL: Record<Which, string> = { key: "Dictate", key2: "Dictate", clean: "Dictate with cleanup", clean2: "Dictate with cleanup" };
  const WHICH: Which[] = ["key", "key2", "clean", "clean2"];
  let globe = $derived(initial); // assignable: refreshed on focus
  onMount(() => {
    // Changed in System Settings, then back to Myna.
    const offFocus = getCurrentWindow().onFocusChanged(async ({ payload }) => payload && (globe = await invoke<boolean>("globe_ok")));
    return () => void offFocus.then((f) => f());
  });
  const rec = new Recorder<Which>({
    fn: true,
    save: (which, key) => save({ ...voice, [which]: key }),
    check: (which, key) => clash(key, [...WHICH.filter((w) => w !== which).map((w): [string, string | null | undefined] => [LABEL[w], voice[w]]), ["Recording", meetKey]]),
  });
  const recording = $derived(rec.recording);

  async function save(next: Voice) {
    const was = voice;
    voice = next;
    rec.error = "";
    try {
      await invoke("voice_set", { voice: next });
    } catch (err) {
      voice = was;
      rec.error = String(err);
    }
  }

  // Removing the first cleanup key moves the second one up, so the first is always the one set.
  const remove = (which: Which) => save(which === "clean" ? { ...voice, clean: voice.clean2 ?? null, clean2: null } : { ...voice, [which]: null });
</script>

<!-- Leaving the window ends recording, so the shortcut never stays switched off. -->
<svelte:window onkeydown={rec.onkeydown} onblur={() => rec.record(null)} />

<section aria-labelledby="voice-h">
  <h2 id="voice-h" class="sr-only">Voice typing</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="voice-on">Use voice typing</label>{@render desc("Hold a key, talk, and let go. Your words appear wherever you’re typing.")}</span>
      <input id="voice-on" type="checkbox" role="switch" class="switch" checked={voice.on} onchange={(e) => save({ ...voice, on: e.currentTarget.checked })} />
    </li>
    {#if voice.on}<Permissions initial={perms} kinds={["mic"]} rows />{/if}
  </ul>
  {#if !voice.on}<p class="err" role="status">{rec.error}</p>{/if}
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
    <p class="err" role="status">{rec.error}</p>
  </section>
{/if}

{#snippet desc(text: string)}<span class="desc" title={text}>{text}</span>{/snippet}
{#snippet x()}<svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg>{/snippet}

<!-- The keycaps sit inside the button, so one control shows the key and changes it. -->
{#snippet recorder(which: Which)}
  <ShortcutButton id="rec-{which}" name="{which.endsWith("2") ? "second " : ""}{LABEL[which]} shortcut" key={voice[which]} recording={recording === which} onclick={() => rec.record(recording === which ? null : which)} />
{/snippet}

<!-- The optional second key: a quiet + until one is set, then its own recorder and a quiet ×. The slot at the right never changes size. -->
{#snippet second(which: "key2" | "clean2")}
  {#if voice[which] || recording === which}
    {@render recorder(which)}
    <button class="btn quiet icon" title="Remove" aria-label={voice[which] ? `Remove the second ${LABEL[which]} shortcut` : "Cancel"} onclick={() => (voice[which] ? remove(which) : rec.record(null))}>{@render x()}</button>
  {:else}
    <button class="btn quiet icon" title="Add another key" aria-label="Add another key" disabled={which === "clean2" && !voice.clean} onclick={() => rec.record(which)}>
      <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
    </button>
  {/if}
{/snippet}
