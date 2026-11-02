<script lang="ts">
  let { data } = $props();
  import ErrorState from "$lib/ErrorState.svelte";
  import ModelTask from "$lib/ModelTask.svelte";
  import Permissions from "$lib/Permissions.svelte";
  import UnloadAfter from "$lib/UnloadAfter.svelte";
  import PillLook from "$lib/PillLook.svelte";

  // Every page is listed from day one; unbuilt ones say what will live there.
  // Same pages and names as the prototype (docs/index.html, SET_PAGES).
  const pages = [
    { id: "look", title: "Look & appearance", soon: "Light or dark, highlight colour, see-through panels." },
    { id: "dictation", title: "Voice typing", soon: "Turn voice typing on, pick its shortcut, and words to remember." },
    { id: "meetings", title: "Meeting notes", soon: "Turn meeting notes on, notice calls, pictures of the call, keep your voice when muted." },
    { id: "ai", title: "AI & privacy", soon: "Speech-to-text and voice models, AI providers for summaries and chat, and quick decisions." },
    { id: "experimental", title: "Experimental features", soon: "New things we're still working on. Off until you turn them on." },
    { id: "about", title: "About", soon: "" },
  ];
  let current = $state("look");
  const page = $derived(pages.find((p) => p.id === current)!);


  function onkeydown(e: KeyboardEvent) {
    const i = pages.findIndex((p) => p.id === current);
    if (e.key === "ArrowDown") current = pages[Math.min(i + 1, pages.length - 1)].id;
    else if (e.key === "ArrowUp") current = pages[Math.max(i - 1, 0)].id;
    else return;
    e.preventDefault();
  }
</script>

<div class="window">
  <nav class="sidebar" aria-label="Settings">
    <div class="titlebar" data-tauri-drag-region></div>
    <ul role="listbox" tabindex="0" aria-label="Settings pages" {onkeydown}>
      {#each pages as p}
        <li role="option" aria-selected={current === p.id} class:selected={current === p.id} onclick={() => (current = p.id)} onkeydown={() => {}}>
          {p.title}
        </li>
      {/each}
    </ul>
  </nav>

  <main>
    <header data-tauri-drag-region><h1>{page.title}</h1></header>
    <div class="content">
      <!-- Every setting lives in the database, so if it failed to open, every page shows why. -->
      {#if !data.storage.ok}
        <ErrorState title="Myna couldn't open its data folder" message={data.storage.error} />
      {:else if page.id === "ai"}
        {#if !data.models?.ok}
          <ErrorState title="Myna couldn't list its models" message={data.models?.error ?? ""} />
        {:else}
          {@const [stt, diar] = data.models.tasks}
          <ModelTask task={stt} title="Turning speech into text" help="Used for voice typing and meeting notes." />
          <UnloadAfter initial={data.models.unload} />
          <ModelTask task={diar} title="Telling voices apart" help="Works out who said what. Always runs on this Mac. Your own voice is always “Me”." />
          <p class="soon later">AI providers for summaries and chat, and quick decisions, will live here too.</p>
        {/if}
      {:else if page.id === "dictation"}
        <section class="group" aria-labelledby="hk-h">
          <h2 id="hk-h">Shortcut</h2>
          <!-- Changing it arrives with ticket 04.3. -->
          <p class="srow"><span><span class="label">Shortcut key</span><span class="hint">Hold it in any app while you talk. Let go to add your words.</span></span>
            <span class="keys" aria-label="Current shortcut: Option Space"><kbd>⌥</kbd><kbd>Space</kbd></span></p>
        </section>
        {#if data.look?.ok}<PillLook initial={data.look.look} />{:else}<ErrorState title="Myna couldn't read the pill's look" message={data.look?.error ?? ""} />{/if}
        <Permissions initial={data.perms} />
      {:else if page.id === "about"}
        <dl>
          <dt>Version</dt><dd>{data.version}</dd>
          <dt>Data folder</dt><dd class="mono">{data.storage.path}</dd>
        </dl>
        <!-- "Powered by Desert Ant Labs" appears here only while a Desert Ant model is enabled (story 03). -->
      {:else}
        <p class="soon">{page.soon}</p>
      {/if}
    </div>
  </main>
</div>

<style>
  .window { display: grid; grid-template-columns: var(--w-settings-nav) 1fr; gap: var(--win-inset);
    height: 100vh; box-sizing: border-box; padding: var(--win-inset); }
  /* Sidebar glass is the window's native vibrancy; the panel adds the token tint and edge. */
  .sidebar { border-radius: var(--r-xl); background: var(--mat-sidebar); box-shadow: inset 0 0 0 .5px var(--edge); }
  .titlebar { height: var(--h-titlebar); }
  ul { list-style: none; margin: 0; padding: 0 var(--sp-2); outline: none; }
  ul:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: var(--r-md); }
  li { height: var(--h-row); display: flex; align-items: center; padding: 0 var(--sp-2); border-radius: var(--r-md); }
  li.selected { background: var(--accent-soft-strong); }
  main { border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: auto; scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  main::-webkit-scrollbar { width: 8px; }
  main::-webkit-scrollbar-thumb { border-radius: 4px; background: var(--line-strong); border: 2px solid transparent; background-clip: content-box; }
  header { height: var(--h-toolbar); display: flex; align-items: center; padding: 0 var(--sp-5); }
  /* Let drags on the title fall through to the header's drag region. */
  h1 { pointer-events: none; margin: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  .content { padding: 0 var(--sp-5) var(--sp-5); }
  .soon { margin: 0; color: var(--text-2); }
  .later { margin-top: var(--sp-5); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: var(--sp-2) var(--sp-4); margin: 0; }
  dt { color: var(--text-3); }
  dd { margin: 0; user-select: text; }
  .group { margin-bottom: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-3); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .srow { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3); margin: 0; padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .label { display: block; font-weight: var(--fw-semibold); }
  .hint { display: block; margin-top: 2px; color: var(--text-2); }
  .keys { display: inline-flex; gap: var(--sp-1); }
  kbd { min-width: 24px; height: 24px; padding: 0 var(--sp-2); box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center;
    border-radius: var(--r-md); background: var(--fill-control); box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); font: inherit; }
  .mono { font: 12px/var(--lh-body) var(--font-mono); overflow-wrap: anywhere; }
</style>
