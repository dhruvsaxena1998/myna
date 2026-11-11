<!-- One task's model list as a drill-in: search, a radio per model, and its download state on the right (docs/settings.html, modelList).
     Takes the task, so Voice typing and Meeting notes (18.7) share it, each with its own selection. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { fmtSize, type Task } from "$lib/models";
  import { cancelDownload, dl, startDownload, watchDownloads } from "$lib/downloads.svelte";
  import { fuzzy } from "$lib/fuzzy";

  // `search`: false for a short list, like the diarization models. `onchange`: after a pick or a removal, which can change the other speech-to-text task's pick too.
  let { task, label, help, active = $bindable(), onchange, search = true }: { task: Task; label: string; help: string; active: string | null; onchange?: () => void; search?: boolean } = $props();

  let error = $state("");
  let q = $state("");
  const installed = (m: { id: string; installed: boolean }) => dl.installed[m.id] ?? m.installed;
  onMount(watchDownloads);

  // The chosen model never searches out of sight.
  let hits = $derived(task.models.filter((m) => fuzzy(q, m.name)));
  let visible = $derived(task.models.filter((m) => m.id === active || hits.includes(m)));

  async function start(id: string) {
    error = await startDownload(id);
  }
  async function cancel(id: string) {
    error = (await cancelDownload(id)) || error;
  }
  async function remove(id: string) {
    error = "";
    try {
      active = await invoke<string | null>("model_delete", { id, task: task.id }); // the default takes over from a deleted pick
      onchange?.();
      dl.installed[id] = false;
      delete dl.errors[id];
      delete dl.retry[id]; // a load failure set it; a removed model offers Download, not Retry
    } catch (e) {
      error = String(e);
    }
  }
  async function pick(id: string) {
    const was = active;
    active = id;
    error = "";
    try {
      await invoke("model_select", { task: task.id, id });
      onchange?.();
    } catch (e) {
      active = was;
      error = String(e);
    }
  }
</script>

{#if search}<label class="search">
  <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
  <input bind:value={q} type="search" placeholder="Search models" aria-label="Search models" autocomplete="off" spellcheck="false"
    onkeydown={(e) => { if (e.key === "Escape" && q) { e.preventDefault(); q = ""; } }} />
</label>{/if}
<section>
  <p class="help">{help}</p>
  <div role="radiogroup" aria-label={label}>
    <ul class="list radios">
      {#each visible as m (m.id)}
        {@const blocked = !!m.needs}
        {@const failed = dl.errors[m.id]}
        <li class="row mrow" class:blocked>
          <input type="radio" name="model-{task.id}" id="m-{task.id}-{m.id}" value={m.id} aria-label="Use {m.name}" checked={active === m.id}
            disabled={blocked || !installed(m)} onchange={() => pick(m.id)} />
          <span>
            <span class="name"><span class="title">{m.name}</span>{" "}<span class="meta">{m.vendor}{m.default ? " · Recommended" : ""}{m.size ? ` · ${fmtSize(m.size)}` : ""}</span></span>
            <span class="desc" class:bad={failed} title={failed || m.desc}>{failed || m.desc}</span>
          </span>
          <!-- Fixed width, so a state change (Download, progress, Installed) never moves the row. Apple's download can't stop, and SDK models can't cancel either. -->
          <span class="end mstate">
            {#if blocked}
              <span class="pill">{m.needs}</span>
            {:else if m.id in dl.pct}
              <span class="bar" role="progressbar" aria-label="Downloading {m.name}" aria-valuenow={dl.pct[m.id]} aria-valuemin="0" aria-valuemax="100"><i style:transform="scaleX({dl.pct[m.id] / 100})"></i></span>
              <span class="pct">{dl.pct[m.id]}%</span>
              {#if !m.system && !m.sdk}
                <button class="btn quiet icon sm" aria-label="Stop downloading {m.name}" onclick={() => cancel(m.id)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17" /></svg></button>
              {/if}
            {:else if installed(m)}
              <span class="ok"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{m.system ? "Built in" : "Installed"}</span>
              {#if !m.system}<button class="btn quiet sm" aria-label="Remove {m.name}" onclick={() => remove(m.id)}>Remove</button>{/if}
            {:else if m.files.length === 0 && !m.system && !m.sdk}
              <span class="pill">Coming later</span>
            {:else}
              <button class="btn sm" aria-label="{dl.retry[m.id] ? 'Retry' : 'Download'} {m.name}" onclick={() => start(m.id)}>{dl.retry[m.id] ? "Retry" : "Download"}</button>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
  </div>
  <p class="help none" role="status">{q.trim() && !hits.length ? "No models match." : ""}</p>
  <p class="err" role="status">{error}</p>
</section>
<p class="help">Sizes are download sizes. A model downloads only when you ask.</p>

<style>
  /* The shared row (60px, separator, one-line description), with the radio column and the status column on top. */
  .row.mrow { grid-template-columns: 20px minmax(0, 1fr) auto; }
  .mrow input { margin: 0; width: 16px; height: 16px; accent-color: var(--accent); }
  .mrow input:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  .mrow.blocked .title, .mrow.blocked .meta, .mrow.blocked .desc { opacity: .55; }
  .mrow .name { display: flex; align-items: center; gap: var(--sp-2); min-width: 0; }
  /* In a narrow window the vendor line gives way first, so a name is never lost. */
  .title { flex: 0 1 auto; min-width: 6ch; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { flex: 0 100 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; font-size: var(--fs-footnote); font-weight: var(--fw-regular); color: var(--text-3); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .mrow .desc.bad { color: var(--danger-text); }
  .mstate { width: 152px; }
  .bar { width: 72px; height: 4px; border-radius: 2px; background: var(--fill-track); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); transform-origin: left; }
  .pct { min-width: 32px; text-align: right; font-size: var(--fs-footnote); color: var(--text-2); font-variant-numeric: tabular-nums; }
  /* Reserved, so a search with no match never moves the page. */
  .none { margin-top: var(--sp-2); min-height: var(--lh-footnote); }
</style>
