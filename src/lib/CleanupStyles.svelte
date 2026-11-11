<!-- Voice typing › Cleanup styles: named instructions for Dictate with cleanup, picked by the app in front (docs/index.html, stylesGroup). -->
<script lang="ts" module>
  /** `shipped` is the text Reset goes back to, none for the user's own styles. */
  export type Style = { id: string; name: string; apps: string[]; text: string; shipped: string | null; newer: boolean };
  export type Cleanup = { styles: Style[]; apps: string[] };
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";

  let { initial }: { initial: Cleanup } = $props();
  let styles = $derived(initial.styles); // assignable: a save returns them as the backend kept them
  let open = $state<string | null>(null);
  let error = $state("");
  let spell = $state("");
  let rev = $state(0); // re-draws the fields so they show what was kept, not what was typed

  const meta = (s: Style) => (s.id === "default" ? "Every other app" : s.apps.length ? s.apps.join(", ") : "No apps yet") + (s.shipped !== null && s.text !== s.shipped ? " · edited" : "");
  const owner = (app: string, of: Style) => styles.find((o) => o !== of && o.apps.includes(app));
  const focus = async (id: string) => { await tick(); document.getElementById(id)?.focus(); };

  async function save(next: Style[]) {
    error = "";
    try {
      styles = await invoke<Style[]>("cleanup_set", { styles: next.map(({ id, name, apps, text }) => ({ id, name, apps, text })) });
    } catch (err) {
      error = String(err);
    }
    rev++;
  }
  const patch = (s: Style, p: Partial<Style>) => save(styles.map((x) => (x === s ? { ...x, ...p } : x)));

  async function toggle(s: Style) {
    open = open === s.id ? null : s.id;
    if (open) spell = await invoke<string>("cleanup_spell").catch(() => "");
    focus(`se-${s.id}`);
  }
  // An app belongs to one style, so adding it moves it.
  async function addApp(s: Style, app: string) {
    if (!app) return;
    await save(styles.map((x) => ({ ...x, apps: x === s ? [...x.apps.filter((a) => a !== app), app] : x.apps.filter((a) => a !== app) })));
    focus(`sa-${s.id}`);
  }
  async function removeApp(s: Style, app: string) {
    await patch(s, { apps: s.apps.filter((a) => a !== app) });
    focus(`sa-${s.id}`);
  }
  async function add() {
    const id = `s${Date.now()}`;
    await save([...styles, { id, name: "New style", apps: [], text: styles[0].shipped!, shipped: null, newer: false }]);
    if (styles.some((s) => s.id === id)) { open = id; await tick(); (document.getElementById(`sn-${id}`) as HTMLInputElement | null)?.select(); }
  }
  async function remove(s: Style) {
    await save(styles.filter((x) => x !== s));
    open = null;
    focus("style-add");
  }
  async function reset(s: Style) {
    await patch(s, { text: s.shipped! });
    focus(`st-${s.id}`);
  }
  // The installed apps, and any a style already holds that aren't here, so it can be moved.
  const apps = $derived([...new Set([...initial.apps, ...styles.flatMap((s) => s.apps)])].sort((a, b) => a.localeCompare(b)));
</script>

<section aria-labelledby="sty-h">
  <h2 id="sty-h">Cleanup styles</h2>
  <p class="help">Dictate with cleanup picks a style by the app you’re in. Only Myna sees the app; its name is never sent.</p>
  <ul class="list">
    {#each styles as s (s.id)}
      <li class="row">
        <span class="od"><span class="name">{s.name}{#if s.newer} <span class="pill status">Newer default</span>{/if}</span><span class="desc">{meta(s)}</span></span>
        <button class="btn" id="se-{s.id}" aria-expanded={open === s.id} aria-controls="ed-{s.id}" onclick={() => toggle(s)}>{open === s.id ? "Done" : "Edit"}</button>
      </li>
      {#if open === s.id}
        <li class="style-li">
          <div class="style-ed" id="ed-{s.id}">
            {#if s.id !== "default"}
              <div class="field"><label class="name" for="sn-{s.id}">Name</label>
                {#key rev}<input class="input" id="sn-{s.id}" value={s.name} maxlength="32" autocomplete="off" onchange={(e) => patch(s, { name: e.currentTarget.value })} />{/key}</div>
            {/if}
            <div class="field"><span class="name" id="sa-l-{s.id}">Apps</span>
              {#if s.id === "default"}<span class="desc">Every app that doesn’t have its own style.</span>
              {:else}
                <div class="apps" role="group" aria-labelledby="sa-l-{s.id}">
                  {#each s.apps as a (a)}
                    <span class="tag">{a}<button type="button" aria-label="Remove {a} from {s.name}" onclick={() => removeApp(s, a)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button></span>
                  {/each}
                  {#key rev}
                    <select class="select" id="sa-{s.id}" aria-label="Add an app to {s.name}" onchange={(e) => addApp(s, e.currentTarget.value)}>
                      <option value="">Add an app…</option>
                      {#each apps.filter((a) => !s.apps.includes(a)) as a}
                        <option value={a}>{a}{owner(a, s) ? ` (now ${owner(a, s)!.name})` : ""}</option>
                      {/each}
                    </select>
                  {/key}
                </div>
              {/if}
            </div>
            <div class="field"><label class="name" for="st-{s.id}">Instructions</label>
              {#key rev}<textarea class="input" id="st-{s.id}" rows="5" spellcheck="true" onchange={(e) => patch(s, { text: e.currentTarget.value })}>{s.text}</textarea>{/key}
              <span class="help">{spell ? `Myna always adds: “Spell these exactly as written: ${spell}.” ` : ""}What you said goes in as text to clean up, never as instructions.</span></div>
            {#if s.newer}
              <p class="newer" role="status"><span>A newer default is available. Your edit is kept until you choose.</span><button class="btn" onclick={() => reset(s)}>Use the new default</button></p>
            {/if}
            <div class="acts">
              {#if s.shipped !== null}<button class="btn" id="reset-{s.id}" disabled={s.text === s.shipped} onclick={() => reset(s)}>Reset to default</button>
              {:else}<button class="btn danger" onclick={() => remove(s)}>Delete style</button>{/if}
            </div>
          </div>
        </li>
      {/if}
    {/each}
  </ul>
  <div class="add"><button class="btn" id="style-add" onclick={add}>Add style</button></div>
  <p class="err" role="status">{error}</p>
</section>

<style>
  /* The editor belongs to the row above it: no separator between them. */
  .list > li.style-li { padding: 0 var(--sp-4) var(--sp-4); background: none; }
  .od { min-width: 0; }
  .style-ed { display: grid; gap: var(--sp-3); }
  .field { display: grid; gap: var(--sp-1); align-content: start; }
  .apps { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .select { width: auto; max-width: 220px; }
  .field .input { display: block; width: 100%; }
  textarea { resize: vertical; }
  .newer { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-2); margin: 0; padding: 8px 12px; border-radius: 12px; background: var(--accent-soft); color: var(--accent-text); font-size: var(--fs-footnote); }
  .acts { display: flex; gap: var(--sp-2); }
</style>
