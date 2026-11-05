<script lang="ts">
  let { data } = $props();
  import { invoke } from "@tauri-apps/api/core";
  import About from "$lib/About.svelte";
  import ErrorState from "$lib/ErrorState.svelte";
  import { glassOn } from "$lib/look";
  import ModelTask from "$lib/ModelTask.svelte";
  import Permissions from "$lib/Permissions.svelte";
  import PasteMethod from "$lib/PasteMethod.svelte";
  import UnloadAfter from "$lib/UnloadAfter.svelte";
  import Experimental from "$lib/Experimental.svelte";
  import Look from "$lib/Look.svelte";
  import Traces from "$lib/Traces.svelte";
  import PillLook from "$lib/PillLook.svelte";
  import Words from "$lib/Words.svelte";
  import VoiceShortcut from "$lib/VoiceShortcut.svelte";

  // Every page is listed from day one; unbuilt ones say what will live there.
  // Same pages and names as the prototype (docs/index.html, SET_PAGES).
  const all = [
    { id: "look", title: "Look & appearance", soon: "Light or dark, highlight colour, see-through panels." },
    { id: "dictation", title: "Voice typing", soon: "Turn voice typing on, pick its shortcut, and words to remember." },
    { id: "meetings", title: "Meeting notes", soon: "Turn meeting notes on, notice calls, pictures of the call, keep your voice when muted." },
    { id: "ai", title: "AI & privacy", soon: "Speech-to-text and voice models, AI providers for summaries and chat, and quick decisions." },
    { id: "experimental", title: "Experimental features", soon: "New things we're still working on. Off until you turn them on." },
    { id: "nerds", title: "Stats for nerds", soon: "" },
    { id: "about", title: "About", soon: "" },
  ];
  // Stats for nerds is an experimental feature: its entry shows only while Experimental features is on.
  const pages = $derived(all.filter((p) => p.id !== "nerds" || look?.experimental));
  let current = $state("look");
  const page = $derived(pages.find((p) => p.id === current)!);
  // Shared by the shortcut rows and the pill preview, which shows the same hint as the real pill.
  let voice = $derived(data.voice?.ok ? data.voice.voice : null); // assignable: saves update it
  // Shared by Look & appearance and Experimental features (Liquid Glass): one look, saved whole.
  let look = $derived(data.appearance.ok ? data.appearance.look : null);
  // Liquid Glass: one sheet of macOS's glass for the whole window in place of its vibrancy; the panels
  // turn into light frosted fills on it (tokens.css). The backend pins it to the window, so a live resize follows.
  function syncGlass() {
    const on = look && glassOn(look);
    invoke("glass_set", { glass: on ? [{ x: 0, y: 0, w: innerWidth, h: innerHeight, radius: 0, shown: true, grow: true }] : [] });
  }
  $effect(() => {
    void [look && glassOn(look), look?.glass, look?.accent]; // the backend picks the tint
    syncGlass();
  });


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
      {:else if page.id === "look"}
        {#if look}<Look bind:look />
        {:else}<ErrorState title="Myna couldn't read its look" message={data.appearance.ok ? "" : data.appearance.error} />{/if}
      {:else if page.id === "experimental"}
        {#if look}<Experimental bind:look />
        {:else}<ErrorState title="Myna couldn't read experimental features" message={data.appearance.ok ? "" : data.appearance.error} />{/if}
      {:else if page.id === "nerds"}
        {#if data.traces?.ok}<Traces initial={data.traces.rows} />
        {:else}<ErrorState title="Myna couldn't read your dictations" message={data.traces?.ok === false ? data.traces.error : ""} />{/if}
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
        {#if !voice}
          <ErrorState title="Myna couldn't read voice typing's settings" message={data.voice?.ok === false ? data.voice.error : ""} />
        {:else}
          <VoiceShortcut bind:voice globe={data.globe} />
          {#if voice.on}
            {#if data.look?.ok}<PillLook initial={data.look.look} listening={{ state: "listening", hold: voice.mode === "hold", key: voice.key }} />
            {:else}<ErrorState title="Myna couldn't read the pill's look" message={data.look?.error ?? ""} />{/if}
            <PasteMethod bind:voice />
            {#if data.rules?.ok}<Words initial={data.rules.rules} />
            {:else}<ErrorState title="Myna couldn't read your words" message={data.rules?.error ?? ""} />{/if}
            <Permissions initial={data.perms} />
          {/if}
        {/if}
      {:else if page.id === "about"}
        {#if data.updates?.ok}<About version={data.version} path={data.storage.path} prefs={data.updates.prefs} status={data.updates.status} />
        {:else}<ErrorState title="Myna couldn't read the update settings" message={data.updates?.ok === false ? data.updates.error : ""} />{/if}
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
  /* Liquid Glass: the panels are frosted fills on the window's glass (tokens.css thins them). */
  :global(html[data-liquid]) main { box-shadow: inset 0 0 0 .5px var(--edge); }
  .titlebar { height: var(--h-titlebar); }
  ul { list-style: none; margin: 0; padding: 0 var(--sp-2); outline: none; }
  ul:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: var(--r-md); }
  li { height: var(--h-row); display: flex; align-items: center; padding: 0 var(--sp-2); border-radius: var(--r-md); }
  /* Stronger than --accent-soft-strong: the sidebar sits on see-through material, which thins a pale wash. */
  li.selected { background: color-mix(in srgb, var(--accent) 28%, transparent); }
  /* On Liquid Glass, tinted or not, a wash of the accent vanishes into the glass: a solid fill, like a focused Finder sidebar. */
  :global(html[data-liquid]) li.selected { background: var(--accent); color: var(--on-accent); }
  main { border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: auto; scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  main::-webkit-scrollbar { width: 8px; }
  main::-webkit-scrollbar-thumb { border-radius: 4px; background: var(--line-strong); border: 2px solid transparent; background-clip: content-box; }
  header { height: var(--h-toolbar); display: flex; align-items: center; padding: 0 var(--sp-5); }
  /* Let drags on the title fall through to the header's drag region. */
  h1 { pointer-events: none; margin: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  .content { padding: 0 var(--sp-5) var(--sp-5); }
  .soon { margin: 0; color: var(--text-2); }
  .later { margin-top: var(--sp-5); }
</style>
