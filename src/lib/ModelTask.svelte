<!-- One task's on-device models (prototype: taskBlock/DIAR on the Speech models page). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { fmtSize, type Task } from "$lib/models";
  import { dl, watchDownloads } from "$lib/downloads.svelte";

  let { task, title, help }: { task: Task; title: string; help: string } = $props();

  let active = $derived(task.active); // assignable: a pick shows at once, a failed save reverts it
  let error = $state("");
  const installed = (m: { id: string; installed: boolean }) => dl.installed[m.id] ?? m.installed;
  let shown = $derived(error || task.models.map((m) => dl.errors[m.id]).find(Boolean) || "");
  onMount(watchDownloads);

  // The buttons update at once; the backend confirms (or the catch puts it back).
  async function start(id: string) {
    error = "";
    delete dl.errors[id];
    dl.pct[id] = 0;
    try {
      await invoke("model_download", { id });
    } catch (e) {
      delete dl.pct[id];
      error = String(e);
    }
  }
  async function cancel(id: string) {
    delete dl.pct[id];
    dl.retry[id] = true;
    invoke("model_cancel", { id }).catch((e) => (error = String(e)));
  }
  async function remove(id: string) {
    error = "";
    try {
      await invoke("model_delete", { id });
      dl.installed[id] = false;
    } catch (e) {
      error = String(e);
    }
  }

  const ACC = ["", "Basic", "Fair", "Good", "Great", "Best"];
  const SPD = ["", "Slow", "Steady", "Quick", "Fast", "Fastest"];

  async function pick(id: string) {
    const was = active;
    active = id;
    error = "";
    try {
      await invoke("model_select", { task: task.id, id });
    } catch (e) {
      active = was;
      error = String(e);
    }
  }
</script>

{#snippet rating(label: string, n: number, words: string[])}
  <span class="rate" role="img" aria-label="{label}: {words[n]}, {n} out of 5">
    <span class="rate-l">{label}</span>
    <span class="pips" aria-hidden="true">{#each [1, 2, 3, 4, 5] as i}<i class:on={i <= n}></i>{/each}</span>
    <span class="rate-w">{words[n]}</span>
  </span>
{/snippet}

<section class="task" aria-labelledby="task-{task.id}">
  <h2 id="task-{task.id}">{title}</h2>
  <p class="help">{help}</p>
  <ul class="list" role="radiogroup" aria-label="{title} model">
    {#each task.models as m (m.id)}
      {@const blocked = !!m.needs}
      <li class="row" class:blocked>
        <input type="radio" name="model-{task.id}" id="m-{m.id}" value={m.id} checked={active === m.id}
          disabled={blocked} onchange={() => pick(m.id)} />
        <label for="m-{m.id}">
          <span class="name">{m.name} <span class="vendor">{m.vendor}</span></span>
          <span class="desc">{m.desc}</span>
        </label>
        <span class="rates">{@render rating("Accuracy", m.acc, ACC)}{@render rating("Speed", m.spd, SPD)}</span>
        <!-- Fixed width so the state can change (Download → progress → Installed) without moving the row. -->
        <div class="dl">
          <!-- Apple Speech: macOS owns the model, so no size, no Cancel (Apple's download can't stop), no Remove. -->
          {#if blocked}
            {#if !m.system}<span class="size">{fmtSize(m.size)}</span>{/if}<span class="pill">{m.needs}</span>
          {:else if m.id in dl.pct}
            <span class="bar" role="progressbar" aria-label="Downloading {m.name}" aria-valuenow={dl.pct[m.id]} aria-valuemin="0" aria-valuemax="100"><b style:transform="scaleX({dl.pct[m.id] / 100})"></b></span>
            <span class="size">{dl.pct[m.id]}%</span>
            {#if !m.system}<button class="btn" onclick={() => cancel(m.id)}>Cancel</button>{/if}
          {:else if installed(m)}
            <span class="ok"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>Installed</span>
            {#if !m.system}
              <span class="size">{fmtSize(m.size)}</span>
              <button class="btn quiet" aria-label="Remove {m.name}" onclick={() => remove(m.id)}>Remove</button>
            {/if}
          {:else if m.files.length === 0 && !m.system}
            <span class="size">{fmtSize(m.size)}</span><span class="pill">Coming soon</span>
          {:else}
            {#if !m.system}<span class="size">{fmtSize(m.size)}</span>{/if}
            <button class="btn" aria-label="Download {m.name}" onclick={() => start(m.id)}>
              <svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{#if dl.retry[m.id]}<path d="M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6" />{:else}<path d="M12 4v11M7 10l5 5 5-5M5 20h14" />{/if}</svg>{dl.retry[m.id] ? "Try again" : "Download"}
            </button>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
  <p class="err" role="status">{shown}</p>
</section>

<style>
  .task + :global(.task) { margin-top: var(--sp-5); }
  h2 { margin: 0; font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .help { margin: var(--sp-1) 0 var(--sp-3); color: var(--text-3); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .list { container-type: inline-size; list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .list > li + li { border-top: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: auto minmax(0, 1fr) auto auto; gap: var(--sp-3); align-items: start; padding: var(--sp-3) var(--sp-4); }
  .row input { margin: 3px 0 0; accent-color: var(--accent); }
  .row.blocked label, .row.blocked .rates { opacity: .55; }
  label { display: block; }
  .name { font-weight: var(--fw-semibold); }
  .vendor { color: var(--text-3); font-weight: var(--fw-regular); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  .rates { display: flex; flex-direction: column; gap: var(--sp-1); padding-top: 2px; }
  .rate { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-footnote); color: var(--text-2); white-space: nowrap; }
  .rate-l { width: 56px; color: var(--text-3); }
  .pips { display: inline-flex; gap: 3px; }
  .pips i { width: 6px; height: 6px; border-radius: 50%; background: var(--line-strong); }
  .pips i.on { background: var(--accent); }
  .rate-w { min-width: 48px; font-weight: var(--fw-medium); }
  .dl { display: flex; align-items: center; justify-content: flex-end; gap: var(--sp-2); min-width: 240px; height: var(--h-ctl); }
  .size { color: var(--text-3); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .ok { display: inline-flex; align-items: center; gap: var(--sp-1); color: var(--ok-text); font-weight: var(--fw-medium); }
  .ic { width: 14px; height: 14px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .pill { display: inline-flex; align-items: center; height: 20px; padding: 0 var(--sp-2); border-radius: var(--r-full); font-size: var(--fs-caption); font-weight: var(--fw-medium); background: var(--fill-hover); color: var(--text-2); white-space: nowrap; }
  .btn { display: inline-flex; align-items: center; gap: var(--sp-1h); height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full);
    background: var(--fill-control); color: var(--text); font: inherit; box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); white-space: nowrap; }
  .btn.quiet { background: none; box-shadow: none; color: var(--text-2); padding: 0 var(--sp-2); }
  .bar { width: 72px; height: 4px; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .bar b { display: block; height: 100%; background: var(--accent); transform-origin: left; }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:focus-visible, .row input:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Narrow (the default Settings width): ratings and download sit under the description, as in the prototype. */
  @container (width < 640px) {
    .row { grid-template-columns: auto minmax(0, 1fr); }
    .rates, .dl { grid-column: 2; justify-content: flex-start; }
    .rates { flex-direction: row; gap: var(--sp-3); }
  }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
