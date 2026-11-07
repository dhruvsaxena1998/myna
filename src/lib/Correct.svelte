<!-- After a 👎: "What did you mean?" Edit the pasted text to the right words; Myna diffs it and may suggest fixes. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { Fix } from "./feedback";
  import FixRow from "./FixRow.svelte";

  let { text, save, onclose }: { text: string; save: (corrected: string | null) => Promise<Fix[]>; onclose: () => void } = $props();
  let draft = $state(""), done = $state(false), fixes = $state<Fix[]>([]), error = $state("");
  let box = $state<HTMLTextAreaElement>();
  // What was already suggested, so only fixes this answer brought show here.
  let known: Promise<Set<string>>;

  onMount(() => {
    draft = text;
    box?.focus();
    known = invoke<Fix[]>("fixes_get").then((l) => new Set(l.map((f) => f.key)), () => new Set<string>());
  });

  async function submit() {
    error = "";
    try {
      const [now, seen] = [await save(draft === text ? null : draft), await known];
      fixes = now.filter((f) => !seen.has(f.key));
      done = true;
    } catch (err) {
      error = String(err);
    }
  }

  async function act(cmd: "fix_add" | "fix_dismiss", key: string) {
    error = "";
    try {
      const left = await invoke<Fix[]>(cmd, { key });
      fixes = fixes.filter((f) => left.some((l) => l.key === f.key));
    } catch (err) {
      error = String(err);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (Escape closes from anywhere inside) -->
<div class="panel" onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); onclose(); } }}>
  {#if !done}
    <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
      <label for="mean">What did you mean?</label>
      <textarea id="mean" bind:this={box} bind:value={draft} rows="3" spellcheck="false" aria-describedby="mean-d"></textarea>
      <p class="help" id="mean-d">Fix the words Myna got wrong. Myna suggests a fix once the same mistake comes up twice.</p>
      <div class="btns"><button class="btn" type="submit">Save</button><button class="btn" type="button" onclick={onclose}>Skip</button></div>
    </form>
  {:else}
    {#if fixes.length}
      <ul>{#each fixes as f (f.key)}<FixRow fix={f} onadd={() => act("fix_add", f.key)} onskip={() => act("fix_dismiss", f.key)} />{/each}</ul>
    {:else}
      <p class="help">Thanks. Myna will suggest a fix if this happens again.</p>
    {/if}
    <div class="btns"><button class="btn" type="button" onclick={onclose}>Done</button></div>
  {/if}
  <p class="err" role="status">{error}</p>
</div>

<style>
  .panel { flex: 1 0 100%; box-sizing: border-box; padding: var(--sp-2) 0 var(--sp-1); }
  form { display: grid; gap: var(--sp-2); }
  label { font-weight: var(--fw-semibold); }
  textarea { box-sizing: border-box; width: 100%; padding: var(--sp-2); border: 0; border-radius: var(--r-lg); background: var(--fill-field); box-shadow: inset 0 0 0 .5px var(--line-strong);
    color: var(--text); font: inherit; resize: vertical; outline: none; }
  textarea:focus-visible { box-shadow: inset 0 0 0 1.5px var(--accent); }
  .help { margin: 0; color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  ul { list-style: none; margin: 0 0 var(--sp-2); padding: 0; border-radius: var(--r-lg); background: var(--fill-field); }
  .btns { display: flex; gap: var(--sp-2); }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); background: var(--fill-control); color: var(--text); font: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
