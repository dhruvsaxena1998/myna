<!-- Write › Cleanup styles (`part="list"`) and its style editor (`"editor"`): named instructions for Dictate with cleanup, picked by the app in front (docs/settings.html, VIEWS.styles and VIEWS.style). One `cleanup`, held by the shell, so Write's row, the list and the editor agree. -->
<script lang="ts" module>
  /** `shipped` is the text Reset goes back to, none for the user's own styles. */
  export type Style = { id: string; name: string; apps: string[]; text: string; shipped: string | null; newer: boolean };
  export type Cleanup = { styles: Style[]; apps: string[] };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";

  // `open`: a drill-in under Write. `drop`: takes a deleted style's editor out of history.
  let { cleanup = $bindable(), part, id = null, spell = "", open = async () => {}, drop = () => {} }: {
    cleanup: Cleanup; part: "list" | "editor"; id?: string | null; spell?: string;
    open?: (sub: string, arg?: string | null) => Promise<void> | void; drop?: (sub: string, arg: string | null) => void;
  } = $props();
  const styles = $derived(cleanup.styles);
  const s = $derived(styles.find((x) => x.id === id));
  let error = $state("");
  let rev = $state(0); // re-draws the fields so they show what was kept, not what was typed

  const apps = (x: Style) => (x.id === "default" ? "Every app that doesn’t have its own style." : x.apps.length ? x.apps.join(", ") : "No apps yet");
  const owner = (app: string, of: Style) => styles.find((o) => o !== of && o.apps.includes(app));
  const focus = async (el: string) => { await tick(); document.getElementById(el)?.focus(); };
  // The installed apps, and any a style already holds that aren't here, so it can be moved.
  const known = $derived([...new Set([...cleanup.apps, ...styles.flatMap((x) => x.apps)])].sort((a, b) => a.localeCompare(b)));

  async function save(next: Style[]) {
    error = "";
    try {
      const kept = await invoke<Style[]>("cleanup_set", { styles: next.map(({ id, name, apps, text }) => ({ id, name, apps, text })) });
      cleanup = { ...cleanup, styles: kept }; // as the backend kept them
    } catch (err) {
      error = String(err);
    }
    rev++;
  }
  const patch = (x: Style, p: Partial<Style>) => save(styles.map((o) => (o === x ? { ...o, ...p } : o)));

  // An app belongs to one style, so adding it moves it.
  async function addApp(x: Style, app: string) {
    if (!app) return;
    await save(styles.map((o) => ({ ...o, apps: o === x ? [...o.apps.filter((a) => a !== app), app] : o.apps.filter((a) => a !== app) })));
    focus("sa");
  }
  async function removeApp(x: Style, app: string) {
    await patch(x, { apps: x.apps.filter((a) => a !== app) });
    focus("sa");
  }
  async function add() {
    const nid = `s${Date.now()}`;
    await save([...styles, { id: nid, name: "New style", apps: [], text: styles[0].shipped!, shipped: null, newer: false }]);
    if (!styles.some((x) => x.id === nid)) return;
    await open("style", nid);
    (document.getElementById("sn") as HTMLInputElement | null)?.select();
  }
  async function remove(x: Style) {
    await save(styles.filter((o) => o !== x));
    if (styles.some((o) => o.id === x.id)) return; // not deleted: the error line says why
    drop("style", x.id);
    focus("style-add");
  }
  async function reset(x: Style) {
    await patch(x, { text: x.shipped! });
    focus("st");
  }
</script>

{#if part === "list"}
  <section>
    <p class="help">Dictate with cleanup picks a style by the app you’re in. Only Myna sees the app; its name is never sent.</p>
    <ul class="list">
      {#each styles as x (x.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
        <li class="row go" onclick={() => open("style", x.id)}>
          <span><span class="name">{x.name}</span><span class="desc" title={apps(x)}>{apps(x)}</span></span>
          <span class="end">
            {#if x.newer}<span class="pill status">Newer default</span>{/if}
            <button class="chev" aria-label="Edit {x.name}" onclick={(e) => { e.stopPropagation(); open("style", x.id); }}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
          </span>
        </li>
      {/each}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <div class="actions"><button class="btn" id="style-add" onclick={add}>Add style</button></div>
{:else if s}
  {#if s.newer}
    <section>
      <ul class="list">
        <li class="row">
          <span><span class="name">A newer default is available</span><span class="desc">Your edit is kept until you choose.</span></span>
          <button class="btn" onclick={() => reset(s)}>Use the new default</button>
        </li>
      </ul>
    </section>
  {/if}
  <section>
    <ul class="list">
      <li class="field one"><label for="sn">Name</label>
        {#key rev}<input class="input" id="sn" value={s.name} maxlength="32" autocomplete="off" disabled={s.id === "default"} onchange={(e) => patch(s, { name: e.currentTarget.value })} />{/key}</li>
      <li class="field one"><span class="name" id="sa-l">Apps</span>
        {#if s.id === "default"}<span class="help">Every app that doesn’t have its own style.</span>
        {:else}
          <div class="apps" role="group" aria-labelledby="sa-l">
            {#each s.apps as a (a)}
              <span class="tag">{a}<button type="button" aria-label="Remove {a} from {s.name}" onclick={() => removeApp(s, a)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button></span>
            {/each}
            {#key rev}
              <select class="select" id="sa" aria-label="Add an app to {s.name}" onchange={(e) => addApp(s, e.currentTarget.value)}>
                <option value="">Add an app…</option>
                {#each known.filter((a) => !s.apps.includes(a)) as a}
                  <option value={a}>{a}{owner(a, s) ? ` (now ${owner(a, s)!.name})` : ""}</option>
                {/each}
              </select>
            {/key}
          </div>
        {/if}
      </li>
      <li class="field"><label for="st">Instructions</label>
        {#key rev}<textarea class="input" id="st" spellcheck="true" onchange={(e) => patch(s, { text: e.currentTarget.value })}>{s.text}</textarea>{/key}
        <span class="help">What the Fast model is told before it sees your words.{spell ? ` Myna always adds: “Spell these exactly as written: ${spell}.”` : ""}</span></li>
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
  <div class="actions end">
    {#if s.shipped !== null}<button class="btn" id="reset" disabled={s.text === s.shipped} onclick={() => reset(s)}>Reset to default</button>
    {:else}<button class="btn danger" onclick={() => remove(s)}>Delete style</button>{/if}
  </div>
{/if}

<style>
  /* The chevron is the row's keyboard stop; it looks like the bare icon. */
  button.chev { padding: 0; border: 0; border-radius: var(--r-xs); background: none; }
  button.chev:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .actions.end { justify-content: flex-end; }
  .apps { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2); min-height: var(--h-ctl); }
  textarea.input { display: block; width: 100%; height: 96px; resize: vertical; }
  .field .input:not(textarea) { width: 100%; }
</style>
