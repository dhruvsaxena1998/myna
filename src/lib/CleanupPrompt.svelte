<!-- Voice typing › Cleanup instructions: what Dictate with cleanup tells the Fast model (docs/index.html, the Default style's editor). -->
<script lang="ts" module>
  export type Cleanup = { text: string; default: string };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { initial }: { initial: Cleanup } = $props();
  let saved = $derived(initial.text); // assignable: a failed save reverts the box
  let draft = $derived(saved);
  let error = $state("");
  const custom = $derived(draft.trim() !== initial.default);

  // On leaving the box, not on every key: a half-written instruction never reaches a dictation.
  async function save(text: string) {
    const was = saved;
    error = "";
    try {
      await invoke("cleanup_set", { text });
      saved = draft = text.trim() || initial.default;
    } catch (err) {
      draft = was;
      error = String(err);
    }
  }
</script>

<section class="group" aria-labelledby="cln-h">
  <h2 id="cln-h">Cleanup instructions</h2>
  <p class="help">What Dictate with cleanup tells your Fast model to do with what you said.</p>
  <label class="sr-only" for="cln">Cleanup instructions</label>
  <textarea id="cln" rows="5" bind:value={draft} onchange={() => save(draft)} spellcheck="true" aria-describedby="cln-d"></textarea>
  <p class="help after" id="cln-d">Myna always adds your Words to remember (“Spell these exactly as written: …”). What you said goes in as text to clean up, never as instructions. With your own instructions, Myna stops rejecting a reply for being much longer or shorter.</p>
  <div class="acts"><button class="btn" disabled={!custom} onclick={() => save(initial.default)}>Reset to default</button></div>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .group { margin-bottom: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-1); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .help { margin: 0 0 var(--sp-3); color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .help.after { margin: 6px 0 var(--sp-2); }
  textarea { display: block; width: 100%; box-sizing: border-box; resize: vertical; padding: 8px 12px; border: 0; border-radius: var(--r-lg); background: var(--fill-field); box-shadow: inset 0 0 0 .5px var(--line-strong);
    color: var(--text); font: inherit; line-height: 1.5; }
  textarea:focus-visible { outline: none; box-shadow: inset 0 0 0 1.5px var(--accent); }
  .acts { display: flex; justify-content: flex-end; }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); background: var(--fill-control); color: var(--text); font: inherit; box-shadow: inset 0 0 0 .5px var(--line-strong); }
  .btn:hover:not(:disabled) { background: var(--fill-hover); }
  .btn:disabled { opacity: .4; }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
