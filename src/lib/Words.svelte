<!-- Voice typing › Your words: the language, the Tidy up switch and the words to remember (docs/index.html). -->
<script lang="ts" module>
  export type Word = { spelling: string; heard: string | null };
  export type Rules = { settings: { language: string; tidy: boolean; words: Word[]; keep: string[] }; languages: [string, string][] };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { initial }: { initial: Rules } = $props();
  let s = $derived(initial.settings); // assignable: a failed save reverts it
  let draft = $state("");
  let error = $state("");
  let input: HTMLInputElement;
  let keepDraft = $state("");
  let keepInput: HTMLInputElement;

  async function save(next: Rules["settings"]) {
    const was = s;
    s = next;
    error = "";
    try {
      await invoke("rules_set", { rules: next });
    } catch (err) {
      s = was;
      error = String(err);
    }
  }

  const label = (w: Word) => (w.heard ? `${w.heard} → ${w.spelling}` : w.spelling);
  const same = (a: Word, b: Word) => a.spelling.toLowerCase() === b.spelling.toLowerCase() && a.heard?.toLowerCase() === b.heard?.toLowerCase();

  // "Drove -> Dhruv" is heard-as then spelling. Comma-separated, so a pasted list adds every word.
  function add(text: string) {
    draft = "";
    const fresh: Word[] = [];
    for (const part of text.split(",")) {
      const [a, b] = part.split(/->|→/).map((x) => x.trim().slice(0, 40));
      const w: Word = b === undefined ? { spelling: a, heard: null } : { spelling: b, heard: a };
      if (w.spelling && w.heard !== "" && ![...s.words, ...fresh].some((x) => same(x, w))) fresh.push(w);
    }
    if (fresh.length) save({ ...s, words: [...s.words, ...fresh] });
  }

  function remove(w: Word) {
    save({ ...s, words: s.words.filter((x) => x !== w) });
    input.focus();
  }

  // Words Myna keeps even when a filler or repeat rule would take them out. Lowercase, no duplicates.
  function addKeep(text: string) {
    keepDraft = "";
    const fresh = text.split(",").map((x) => x.trim().toLowerCase().slice(0, 40)).filter((x, i, all) => x && !s.keep.includes(x) && all.indexOf(x) === i);
    if (fresh.length) save({ ...s, keep: [...s.keep, ...fresh] });
  }

  function removeKeep(w: string) {
    save({ ...s, keep: s.keep.filter((x) => x !== w) });
    keepInput.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Backspace" && !draft && s.words.length) save({ ...s, words: s.words.slice(0, -1) });
  }
</script>

<section aria-labelledby="your-h">
  <h2 id="your-h">Your words</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="lang">Language you speak</label><span class="desc" id="lang-d">Myna tidies up English. In other languages it keeps your words as said.</span></span>
      <select id="lang" class="select" aria-describedby="lang-d" value={s.language} onchange={(e) => save({ ...s, language: e.currentTarget.value })}>
        {#each initial.languages as [code, name]}<option value={code}>{name}</option>{/each}
      </select>
    </li>
    <li class="row">
      <span><label class="name" for="tidy">Tidy up as I talk</label><span class="desc" id="tidy-d">{s.tidy ? "Removes um and uh and repeated words, and writes numbers like 25 as digits." : "Pastes exactly what the model heard. Words to remember still apply."}</span></span>
      <input type="checkbox" role="switch" class="switch" id="tidy" aria-describedby="tidy-d" checked={s.tidy} onchange={(e) => save({ ...s, tidy: e.currentTarget.checked })} />
    </li>
  </ul>
</section>

<section aria-labelledby="dict-h">
  <h2 id="dict-h">Words to remember</h2>
  <p class="help">Names and special words Myna should spell your way.</p>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the input inside takes the keys) -->
  <div class="chipin" onclick={() => input.focus()}>
    {#each s.words as w (w)}
      <span class="tag">{label(w)}<button type="button" aria-label="Remove {label(w)}" onclick={(e) => { e.stopPropagation(); remove(w); }}>
        <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button></span>
    {/each}
    <form class="chipin-f" onsubmit={(e) => { e.preventDefault(); add(draft); }}>
      <label class="sr-only" for="dict-in">Add words for Myna to remember</label>
      <input id="dict-in" bind:this={input} bind:value={draft} maxlength="200" autocomplete="off" spellcheck="false" aria-describedby="dict-help"
        placeholder={s.words.length ? "Add a word" : "Type a word, then a comma"} oninput={() => draft.includes(",") && add(draft)} {onkeydown} />
    </form>
  </div>
  <p class="help after" id="dict-help">Return or comma adds a word. To fix a word Myna mishears, type it as Drove -&gt; Dhruv.</p>
  <p class="err" role="status">{error}</p>
</section>

<section aria-labelledby="keep-h">
  <h2 id="keep-h">Never remove</h2>
  <p class="help">Words Myna keeps even when they sound like a filler or a repeat, like oh or the.</p>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the input inside takes the keys) -->
  <div class="chipin" onclick={() => keepInput.focus()}>
    {#each s.keep as w (w)}
      <span class="tag">{w}<button type="button" aria-label="Remove {w} from Never remove" onclick={(e) => { e.stopPropagation(); removeKeep(w); }}>
        <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button></span>
    {/each}
    <form class="chipin-f" onsubmit={(e) => { e.preventDefault(); addKeep(keepDraft); }}>
      <label class="sr-only" for="keep-in">Add words for Myna to never remove</label>
      <input id="keep-in" bind:this={keepInput} bind:value={keepDraft} maxlength="200" autocomplete="off" spellcheck="false"
        placeholder={s.keep.length ? "Add a word" : "Type a word, then a comma"} oninput={() => keepDraft.includes(",") && addKeep(keepDraft)}
        onkeydown={(e) => { if (e.key === "Backspace" && !keepDraft && s.keep.length) save({ ...s, keep: s.keep.slice(0, -1) }); }} />
    </form>
  </div>
</section>

<style>
  .chipin-f { flex: 1 0 160px; margin: 0; }
  input:not(.switch) { width: 100%; box-sizing: border-box; padding: 4px 2px; border: 0; background: none; outline: none; color: var(--text); font: inherit; }
</style>
