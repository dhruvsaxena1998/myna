<!-- Meeting notes › Summary (`part="rows"`), its templates list (`"list"`) and template editor (`"editor"`) (docs/settings.html, VIEWS.meetings, VIEWS.templates and VIEWS.tpl). One `summary`, held by the shell, so the rows, the list and the editor agree. -->
<script lang="ts" module>
  /** `model`: "main" (Uses Main), "none" (Not set: its provider was removed) or "<provider id>|<model>"; `gone`: that model left its provider's list. */
  export type Prefs = { auto: boolean; model: string; gone: boolean; template: string };
  /** `shipped` is the text Reset goes back to, none for the user's own templates. */
  export type Template = { id: string; name: string; text: string; shipped: string | null; newer: boolean };
  export type Summary = { prefs: Prefs; templates: Template[] };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import type { Ai } from "./AiProviders.svelte";
  import ModelRow from "./ModelRow.svelte";

  // `open`: a drill-in under Meeting notes. `drop`: takes a deleted template's editor out of history.
  let { summary = $bindable(), part, ai = null, id = null, open = async () => {}, drop = () => {} }: {
    summary: Summary; part: "rows" | "list" | "editor"; ai?: Ai | null; id?: string | null;
    open?: (sub: string, arg?: string | null) => Promise<void> | void; drop?: (sub: string, arg: string | null) => void;
  } = $props();
  const prefs = $derived(summary.prefs);
  const templates = $derived(summary.templates);
  const t = $derived(templates.find((x) => x.id === id));
  let error = $state("");
  let rev = $state(0); // re-draws the fields so they show what was kept, not what was typed

  const focus = async (el: string) => { await tick(); document.getElementById(el)?.focus(); };

  async function savePrefs(next: Prefs) {
    error = "";
    const was = summary.prefs;
    summary = { ...summary, prefs: { ...next, gone: next.model === was.model && was.gone } };
    try {
      await invoke("summary_prefs_set", { prefs: next });
    } catch (err) {
      summary = { ...summary, prefs: was };
      error = String(err);
    }
    rev++;
  }

  async function save(next: Template[]) {
    error = "";
    try {
      const kept = await invoke<Template[]>("summary_templates_set", { templates: next.map(({ id, name, text }) => ({ id, name, text })) });
      summary = { ...summary, templates: kept }; // as the backend kept them
    } catch (err) {
      error = String(err);
    }
    rev++;
  }
  const patch = (x: Template, p: Partial<Template>) => save(templates.map((o) => (o === x ? { ...o, ...p } : o)));
  async function add() {
    const nid = `t${Date.now()}`;
    await save([...templates, { id: nid, name: "New template", text: templates[0].shipped!, shipped: null, newer: false }]);
    if (!templates.some((x) => x.id === nid)) return;
    await open("template", nid);
    (document.getElementById("tn") as HTMLInputElement | null)?.select();
  }
  async function remove(x: Template) {
    await save(templates.filter((o) => o !== x));
    if (templates.some((o) => o.id === x.id)) return; // not deleted: the error line says why
    drop("template", x.id);
    focus("tpl-add");
  }
  async function reset(x: Template) {
    await patch(x, { text: x.shipped! });
    focus("tt");
  }
  const names = $derived(templates.map((x) => x.name).join(", "));
</script>

{#if part === "rows"}
  <section aria-labelledby="sum-h">
    <h2 id="sum-h">Summary</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="sum-auto">Summarize when a meeting finishes</label><span class="desc">Writes a summary as soon as the notes are ready.</span></span>
        <input id="sum-auto" type="checkbox" role="switch" class="switch" checked={prefs.auto} onchange={(e) => savePrefs({ ...prefs, auto: e.currentTarget.checked })} />
      </li>
      <ModelRow id="sum-model" name="Summary model" base="main" value={prefs.model} gone={prefs.gone} {ai} {rev} onpick={(model) => savePrefs({ ...prefs, model })} />
      <li class="row">
        <span><label class="name" for="sum-tpl">Template for new summaries</label><span class="desc">You can pick another one when you regenerate.</span></span>
        {#key rev}
          <!-- A deleted template's summaries use Recap, as the backend does. -->
          <select id="sum-tpl" class="select" value={templates.some((x) => x.id === prefs.template) ? prefs.template : templates[0]?.id} onchange={(e) => savePrefs({ ...prefs, template: e.currentTarget.value })}>
            {#each templates as x (x.id)}<option value={x.id}>{x.name}</option>{/each}
          </select>
        {/key}
      </li>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
      <li class="row go" onclick={() => open("templates")}>
        <span><span class="name">Summary templates</span><span class="desc" title={names}>{names}</span></span>
        <span class="end">
          {#if templates.some((x) => x.newer)}<span class="pill status">Newer default</span>{/if}
          <span class="count">{templates.length}</span>
          <button class="chev" aria-label="Summary templates" onclick={(e) => { e.stopPropagation(); open("templates"); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
        </span>
      </li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{:else if part === "list"}
  <section>
    <p class="help">What a summary should say. Myna always asks for Markdown, with timestamps that open the transcript, from the lines you kept.</p>
    <ul class="list">
      {#each templates as x (x.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
        <li class="row go" onclick={() => open("template", x.id)}>
          <span><span class="name">{x.name}</span><span class="desc" title={x.text}>{x.text}</span></span>
          <span class="end">
            {#if x.newer}<span class="pill status">Newer default</span>{/if}
            <button class="chev" aria-label="Edit {x.name}" onclick={(e) => { e.stopPropagation(); open("template", x.id); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
          </span>
        </li>
      {/each}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <div class="actions"><button class="btn" id="tpl-add" onclick={add}>Add template</button></div>
{:else if t}
  {#if t.newer}
    <section>
      <ul class="list">
        <li class="row">
          <span><span class="name">A newer default is available</span><span class="desc">Your edit is kept until you choose.</span></span>
          <button class="btn" onclick={() => reset(t)}>Use the new default</button>
        </li>
      </ul>
    </section>
  {/if}
  <section>
    <ul class="list">
      <li class="field one"><label for="tn">Name</label>
        {#key rev}<input class="input" id="tn" value={t.name} maxlength="40" autocomplete="off" disabled={t.shipped !== null} onchange={(e) => patch(t, { name: e.currentTarget.value })} />{/key}</li>
      <li class="field"><label for="tt">Instructions</label>
        {#key rev}<textarea class="input" id="tt" spellcheck="true" onchange={(e) => patch(t, { text: e.currentTarget.value })}>{t.text}</textarea>{/key}
        <span class="help">What the Summary model is told after it has read the meeting’s notes.</span></li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <div class="actions end">
    {#if t.shipped !== null}<button class="btn" disabled={t.text === t.shipped} onclick={() => reset(t)}>Reset to default</button>
    {:else}<button class="btn danger" onclick={() => remove(t)}>Delete template</button>{/if}
  </div>
{/if}

<style>
  /* The chevron is the row's keyboard stop; it looks like the bare icon. */
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .actions.end { justify-content: flex-end; }
  textarea.input { display: block; width: 100%; height: 96px; resize: vertical; }
  .field .input:not(textarea) { width: 100%; }
</style>
