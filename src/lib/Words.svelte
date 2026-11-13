<!-- Understand › Your words (`part="rows"`) and its two drill-ins, Custom vocabulary (`"remember"`) and Filler exceptions (`"keep"`). One `rules`, held by the shell, so a drill-in's change shows in the rows' counts. -->
<script lang="ts" module>
  export type Word = { spelling: string; heard: string | null };
  export type Rules = { settings: { language: string; tidy: boolean; words: Word[]; keep: string[] }; languages: [string, string][] };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { rules = $bindable(), part, open = () => {} }: { rules: Rules; part: "rows" | "remember" | "keep"; open?: (sub: string) => void } = $props();
  const s = $derived(rules.settings);
  let draft = $state("");
  let error = $state("");
  let input = $state<HTMLInputElement>();

  async function save(next: Rules["settings"]) {
    const was = rules;
    rules = { ...rules, settings: next };
    error = "";
    try {
      await invoke("rules_set", { rules: next });
    } catch (err) {
      rules = was;
      error = String(err);
    }
  }

  const label = (w: Word) => (w.heard ? `${w.heard} → ${w.spelling}` : w.spelling);
  const same = (a: Word, b: Word) => a.spelling.toLowerCase() === b.spelling.toLowerCase() && a.heard?.toLowerCase() === b.heard?.toLowerCase();
  // "the first three, and N more", or the placeholder.
  const first3 = (a: string[], empty: string) => (a.length ? `${a.slice(0, 3).join(", ")}${a.length > 3 ? ` and ${a.length - 3} more` : ""}` : empty);

  // "Drove -> Dhruv" is heard-as then spelling. Comma-separated, so a pasted list adds every word.
  function addWords(text: string) {
    const fresh: Word[] = [];
    for (const part of text.split(",")) {
      const [a, b] = part.split(/->|→/).map((x) => x.trim().slice(0, 40));
      const w: Word = b === undefined ? { spelling: a, heard: null } : { spelling: b, heard: a };
      if (w.spelling && w.heard !== "" && ![...s.words, ...fresh].some((x) => same(x, w))) fresh.push(w);
    }
    if (fresh.length) save({ ...s, words: [...s.words, ...fresh] });
  }
  // Words Myna keeps even when a filler or repeat rule would take them out. Lowercase, no duplicates.
  function addKeep(text: string) {
    const fresh = text.split(",").map((x) => x.trim().toLowerCase().slice(0, 40)).filter((x, i, all) => x && !s.keep.includes(x) && all.indexOf(x) === i);
    if (fresh.length) save({ ...s, keep: [...s.keep, ...fresh] });
  }
  const isKeep = $derived(part === "keep");
  function add(text: string) {
    draft = "";
    if (isKeep) addKeep(text);
    else addWords(text);
  }
  function remove(i: number) {
    save(isKeep ? { ...s, keep: s.keep.filter((_, k) => k !== i) } : { ...s, words: s.words.filter((_, k) => k !== i) });
    input?.focus();
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Backspace" || draft) return;
    if (isKeep ? s.keep.length : s.words.length) remove((isKeep ? s.keep.length : s.words.length) - 1);
  }
  const chips = $derived(isKeep ? s.keep.map((w) => ({ text: w, aria: `Remove ${w} from Filler exceptions` })) : s.words.map((w) => ({ text: label(w), aria: `Remove ${label(w)}` })));
  const help = $derived(isKeep ? "Words Myna keeps even when a filler rule would take them out, like “like”." : "Names and words Myna should spell your way. For one it mishears, type Drove -> Dhruv.");
  const tidyDesc = $derived(s.tidy ? "Removes um, uh and repeats, and writes numbers as digits." : "Pastes exactly what the model heard.");
  const lists = $derived([
    { sub: "remember", name: "Custom vocabulary", items: s.words.map(label), empty: "Names and words Myna should spell your way." },
    { sub: "keep", name: "Filler exceptions", items: s.keep, empty: "Words Myna keeps even when they sound like filler." },
  ].map((l) => ({ sub: l.sub, name: l.name, n: l.items.length, desc: first3(l.items, l.empty) })));
  const field = $derived(isKeep ? "Add words for Myna to never remove" : "Add words for Myna to remember");
</script>

{#if part === "rows"}
  <section aria-labelledby="your-h">
    <h2 id="your-h">Your words</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="lang">Spoken language</label><span class="desc" id="lang-d" title="Myna tidies up English. Other languages stay as said.">Myna tidies up English. Other languages stay as said.</span></span>
        <select id="lang" class="select" aria-describedby="lang-d" value={s.language} onchange={(e) => save({ ...s, language: e.currentTarget.value })}>
          {#each rules.languages as [code, name]}<option value={code}>{name}</option>{/each}
        </select>
      </li>
      <li class="row">
        <span><label class="name" for="tidy">Filler and repeat removal</label><span class="desc" id="tidy-d" title={tidyDesc}>{tidyDesc}</span></span>
        <input type="checkbox" role="switch" class="switch" id="tidy" aria-describedby="tidy-d" checked={s.tidy} onchange={(e) => save({ ...s, tidy: e.currentTarget.checked })} />
      </li>
      {#each lists as l (l.sub)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button takes the keys) -->
        <li class="row go" onclick={() => open(l.sub)}>
          <span><span class="name">{l.name}</span><span class="desc" title={l.desc}>{l.desc}</span></span>
          <span class="end">
            <span class="count">{l.n || ""}</span>
            <button class="chev" aria-label={l.name} onclick={(e) => { e.stopPropagation(); open(l.sub); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
          </span>
        </li>
      {/each}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{:else}
  <section>
    <p class="help" id="chips-help">{help}</p>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions (the input inside takes the keys) -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul class="list"><li class="chips" onclick={() => input?.focus()}>
      {#each chips as c, i (c.text)}
        <span class="tag">{c.text}<button type="button" aria-label={c.aria} onclick={(e) => { e.stopPropagation(); remove(i); }}>
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button></span>
      {/each}
      <input bind:this={input} bind:value={draft} maxlength="200" autocomplete="off" spellcheck="false" aria-label={field} aria-describedby="chips-help"
        placeholder="Add a word" oninput={() => draft.includes(",") && add(draft)}
        onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); add(draft); } else onkeydown(e); }} />
    </li></ul>
    <p class="err" role="status">{error}</p>
  </section>
  <p class="help">Return or comma adds a word. Backspace in an empty field removes the last one.</p>
{/if}

<style>
  /* The chevron is the row's keyboard stop; it looks like the bare icon. */
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; color: inherit; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
</style>
