<!-- The Write page: how your words land, the cleanup that runs on them, and feedback (docs/settings.html, VIEWS.write). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Ai } from "./AiProviders.svelte";
  import type { Cleanup } from "./CleanupStyles.svelte";
  import HostPill from "./HostPill.svelte";
  import Permissions, { type Perms } from "./Permissions.svelte";
  import type { Voice } from "./VoiceShortcut.svelte";

  let { voice = $bindable(), ai, cleanup, perms, go, open }: {
    voice: Voice; ai: Ai | null; cleanup: Cleanup | null; perms: Perms; go: (page: "ai") => void; open: (sub: string) => void;
  } = $props();

  // A Fast route whose model is gone counts as none.
  const fast = $derived(ai?.fast && !ai.fast.gone ? ai.fast : null);
  const provider = $derived(fast ? ai!.providers.find((p) => p.id === fast.provider) : undefined);
  const names = $derived(cleanup?.styles.map((s) => s.name).join(", ") ?? "");
  const anyNewer = $derived(cleanup?.styles.some((s) => s.newer));

  let error = $state(""); // the rating prompt
  let insertError = $state("");
  let exported = $state(false);
  let exportError = $state("");

  async function save(next: Voice, fail: (m: string) => void) {
    const was = voice;
    voice = next;
    fail("");
    try {
      await invoke("voice_set", { voice: next });
    } catch (err) {
      voice = was;
      fail(String(err));
    }
  }
  async function exportRatings() {
    exported = false;
    exportError = "";
    try {
      await invoke("feedback_export");
      exported = true;
    } catch (err) {
      exportError = String(err);
    }
  }
  const pasteDesc = $derived(voice.paste === "clipboard" ? "Fastest. Myna puts back whatever you had copied." : "Types each letter. Slower, but works where pasting is blocked.");
  const feedbackLine = $derived(error || exportError);
</script>

<section aria-labelledby="ins-h">
  <h2 id="ins-h">Text insertion</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name" id="paste-l">Insert method</span><span class="desc" title={pasteDesc}>{pasteDesc}</span></span>
      <fieldset class="seg" aria-labelledby="paste-l">
        {#each [["clipboard", "Paste"], ["type", "Type"]] as [v, label]}
          <label><input type="radio" name="paste" value={v} checked={voice.paste === v} onchange={() => save({ ...voice, paste: v as Voice["paste"] }, (m) => (insertError = m))} /><span>{label}</span></label>
        {/each}
      </fieldset>
    </li>
    <Permissions initial={perms} kinds={["ax"]} rows />
  </ul>
  <p class="err" role="status">{insertError}</p>
</section>

<section aria-labelledby="cl-h">
  <h2 id="cl-h">Cleanup</h2>
  <ul class="list">
    <li class="row">
      {#if fast}
        {@const desc = `${fast.model}${provider ? ` · ${provider.name}` : ""}`}
        <span><span class="name">Cleanup model (Fast)</span><span class="desc" title={desc}><span class="mono">{fast.model}</span>{provider ? ` · ${provider.name}` : ""}</span></span>
        <span class="end">{#if provider}<HostPill off={provider.off_device} />{/if}<button class="btn" onclick={() => go("ai")}>Change</button></span>
      {:else}
        <span><span class="name">Cleanup model (Fast)</span><span class="desc">Dictate with cleanup needs a Fast model on AI providers.</span></span>
        <span class="end"><button class="btn" onclick={() => go("ai")}>Set up</button></span>
      {/if}
    </li>
    {#if fast && cleanup}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
      <li class="row go" onclick={() => open("styles")}>
        <span><span class="name">Cleanup styles</span><span class="desc" title={names}>{names}</span></span>
        <span class="end">
          {#if anyNewer}<span class="pill status">Newer default</span>{/if}
          <span class="count">{cleanup.styles.length}</span>
          <button class="chev" aria-label="Cleanup styles" onclick={(e) => { e.stopPropagation(); open("styles"); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
        </span>
      </li>
    {:else}
      <li class="row">
        <span><span class="name">Cleanup styles</span><span class="desc">{fast ? "Myna couldn’t read the cleanup styles." : "Available once a Fast model is set."}</span></span>
        <span class="end"><span class="chev dim" aria-hidden="true"><svg class="ic" viewBox="0 0 24 24"><path d="M9 6l6 6-6 6" /></svg></span></span>
      </li>
    {/if}
  </ul>
</section>

<section aria-labelledby="fb-h">
  <h2 id="fb-h">Feedback</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="fb-ask">Rating prompt</label><span class="desc" title="The pill shows a thumbs up and down for a few seconds. Clicking keeps you in your app.">The pill shows a thumbs up and down for a few seconds. Clicking keeps you in your app.</span></span>
      <input id="fb-ask" type="checkbox" role="switch" class="switch" checked={voice.rate ?? false} onchange={(e) => save({ ...voice, rate: e.currentTarget.checked }, (m) => (error = m))} />
    </li>
    <li class="row">
      <span><span class="name">Export ratings</span><span class="desc" title="Ratings stay on this Mac. Export saves a file you can check and send us.">Ratings stay on this Mac. Export saves a file you can check and send us.</span></span>
      <button type="button" class="btn" onclick={exportRatings}>Export…</button>
    </li>
  </ul>
  <p class="err" class:done={exported && !feedbackLine} role="status">{feedbackLine || (exported ? "Saved to Downloads" : "")}</p>
</section>

<style>
  .chev.dim { opacity: .45; }
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .err.done { color: var(--text-2); }
</style>
