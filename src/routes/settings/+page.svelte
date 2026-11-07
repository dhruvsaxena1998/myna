<script lang="ts">
  let { data } = $props();
  import AiProviders from "$lib/AiProviders.svelte";
  import About from "$lib/About.svelte";
  import ErrorState from "$lib/ErrorState.svelte";
  import { glassOn, syncGlass } from "$lib/look";
  import ModelTask from "$lib/ModelTask.svelte";
  import Permissions from "$lib/Permissions.svelte";
  import PasteMethod from "$lib/PasteMethod.svelte";
  import UnloadAfter from "$lib/UnloadAfter.svelte";
  import Experimental from "$lib/Experimental.svelte";
  import Look from "$lib/Look.svelte";
  import PillLook from "$lib/PillLook.svelte";
  import Words, { type Rules } from "$lib/Words.svelte";
  import Fixes from "$lib/Fixes.svelte";
  import Feedback from "$lib/Feedback.svelte";
  import VoiceShortcut from "$lib/VoiceShortcut.svelte";
  import CleanupStyles from "$lib/CleanupStyles.svelte";

  // Every page is listed from day one; unbuilt ones say what will live there.
  // Same pages and names as the prototype (docs/index.html, SET_PAGES).
  const pages = [
    { id: "look", icon: '<circle cx="12" cy="12" r="8"/><path d="M12 4a8 8 0 0 0 0 16z" fill="currentColor"/>', title: "Look & appearance", soon: "Light or dark, highlight colour, see-through panels." },
    { id: "dictation", icon: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>', title: "Voice typing", soon: "Turn voice typing on, pick its shortcut, and words to remember." },
    { id: "meetings", icon: '<circle cx="9" cy="8" r="3"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><circle cx="16.5" cy="9" r="2.5"/><path d="M16.5 14.5a4 4 0 0 1 4 4.5"/>', title: "Meeting notes", soon: "Notes from your calls: noticing a call, taking notes, and keeping your voice when you’re muted." },
    { id: "models", icon: '<path d="M4 10v4M8 7v10M12 4v16M16 8v8M20 11v2"/>', title: "Speech models", soon: "Speech-to-text and voice models." },
    { id: "ai", icon: '<rect x="4" y="4" width="16" height="7" rx="2"/><rect x="4" y="13" width="16" height="7" rx="2"/><path d="M8 7.5h.01M8 16.5h.01"/>', title: "AI provider", soon: "Providers for summaries and chat, and which model does what." },
    { id: "decide", icon: '<path d="M12 20v-6L6 8M12 14l6-6M6 4v4h4M18 4v4h-4"/>', title: "Decision provider", soon: "Which model makes Myna’s small choices, like the folder a meeting goes in." },
    { id: "experimental", icon: '<path d="M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.7 3h10.6a2 2 0 0 0 1.7-3l-5-9V3M7.5 15h9"/>', title: "Experimental features", soon: "New things we're still working on. Off until you turn them on." },
    { id: "about", icon: '<circle cx="12" cy="12" r="8"/><path d="M12 11v5M12 8h.01"/>', title: "About", soon: "" },
  ];
  import type { Ai } from "$lib/AiProviders.svelte";
  import { invoke } from "@tauri-apps/api/core";
  let current = $state("look");
  const page = $derived(pages.find((p) => p.id === current)!);
  // Shared by the shortcut rows and the pill preview, which shows the same hint as the real pill.
  let voice = $derived(data.voice?.ok ? data.voice.voice : null); // assignable: saves update it
  // Shared by Look & appearance and Experimental features (Liquid Glass): one look, saved whole.
  // Adding a fix changes it behind Words' back, so read it again.
  let rules = $derived(data.rules?.ok ? data.rules.rules : null);
  const reloadRules = async () => (rules = await invoke<Rules>("rules_get").catch(() => rules));
  let look = $derived(data.appearance.ok ? data.appearance.look : null);
  // The AI provider page changes it; read it again on the way out, so the pages after it (Voice typing's cleanup row) never show the old one.
  let ai = $derived(data.ai?.ok ? data.ai.ai : null);
  async function show(id: string) {
    if (current === "ai" && id !== "ai") ai = await invoke<Ai>("ai_get").catch(() => ai);
    current = id;
  }
  $effect(() => {
    void [look && glassOn(look), look?.glass, look?.accent]; // the backend picks the tint
    syncGlass(look);
  });


  function onkeydown(e: KeyboardEvent) {
    const i = pages.findIndex((p) => p.id === current);
    if (e.key === "ArrowDown") void show(pages[Math.min(i + 1, pages.length - 1)].id);
    else if (e.key === "ArrowUp") void show(pages[Math.max(i - 1, 0)].id);
    else return;
    e.preventDefault();
  }
</script>

<div class="window">
  <nav class="sidebar" aria-label="Settings">
    <div class="titlebar" data-tauri-drag-region></div>
    <ul role="listbox" tabindex="0" aria-label="Settings pages" {onkeydown}>
      {#each pages as p}
        <li role="option" aria-selected={current === p.id} class:selected={current === p.id} onclick={() => show(p.id)} onkeydown={() => {}}>
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html p.icon}</svg>{p.title}
        </li>
      {/each}
    </ul>
  </nav>

  <main>
    <header data-tauri-drag-region><h1>{page.title}</h1></header>
    <div class="content prefs">
      <!-- Every setting lives in the database, so if it failed to open, every page shows why. -->
      {#if !data.storage.ok}
        <ErrorState title="Myna couldn't open its data folder" message={data.storage.error} />
      {:else if page.id === "look"}
        {#if look}<Look bind:look />
        {:else}<ErrorState title="Myna couldn't read its look" message={data.appearance.ok ? "" : data.appearance.error} />{/if}
      {:else if page.id === "experimental"}
        {#if look}<Experimental bind:look />
        {:else}<ErrorState title="Myna couldn't read experimental features" message={data.appearance.ok ? "" : data.appearance.error} />{/if}
      {:else if page.id === "ai"}
        {#if ai}<AiProviders initial={ai} />
        {:else}<ErrorState title="Myna couldn't read its AI providers" message={data.ai?.ok === false ? data.ai.error : ""} />{/if}
      {:else if page.id === "models"}
        {#if !data.models?.ok}
          <ErrorState title="Myna couldn't list its models" message={data.models?.error ?? ""} />
        {:else}
          {@const [stt, diar] = data.models.tasks}
          <ModelTask task={stt} title="Turning speech into text" help="Used for voice typing and meeting notes." tools />
          <UnloadAfter initial={data.models.unload} />
          <ModelTask task={diar} title="Telling voices apart" help="Works out who said what. Always runs on this Mac. Your own voice is always “Me”." />
        {/if}
      {:else if page.id === "dictation"}
        {#if !voice}
          <ErrorState title="Myna couldn't read voice typing's settings" message={data.voice?.ok === false ? data.voice.error : ""} />
        {:else}
          <VoiceShortcut bind:voice globe={data.globe} {ai} setup={() => show("ai")} />
          {#if voice.on}
            {#if data.look?.ok}<PillLook initial={data.look.look} listening={{ state: "listening", hold: voice.mode === "hold", key: voice.key }} />
            {:else}<ErrorState title="Myna couldn't read the pill's look" message={data.look?.error ?? ""} />{/if}
            <PasteMethod bind:voice />
            <Fixes initial={data.fixes} onadd={reloadRules} />
            {#if rules}<Words initial={rules} />
            {:else}<ErrorState title="Myna couldn't read your words" message={data.rules?.ok === false ? data.rules.error : ""} />{/if}
            <Feedback />
            <!-- Styles only matter once there's a Fast model to run them. -->
            {#if ai?.fast && !ai.fast.gone}
              {#if data.cleanup?.ok}<CleanupStyles initial={data.cleanup.cleanup} />
              {:else}<ErrorState title="Myna couldn't read the cleanup styles" message={data.cleanup?.error ?? ""} />{/if}
            {/if}
            <Permissions initial={data.perms} />
          {/if}
        {/if}
      {:else if page.id === "about"}
        {#if data.updates?.ok}<About version={data.version} path={data.storage.path} prefs={data.updates.prefs} status={data.updates.status} />
        {:else}<ErrorState title="Myna couldn't read the update settings" message={data.updates?.ok === false ? data.updates.error : ""} />{/if}
      {:else}
        <!-- An unbuilt page is an empty state, not a blank one (DESIGN.md › Empty and error states). -->
        <div class="soon">
          <h2>Not in this version yet</h2>
          <p>{page.soon}</p>
          <span class="tag">Coming later</span>
        </div>
      {/if}
    </div>
  </main>
</div>

<style>
  .window { display: grid; grid-template-columns: var(--w-settings-nav) minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); gap: var(--win-inset);
    height: 100vh; box-sizing: border-box; padding: var(--win-inset); }
  /* Sidebar glass is the window's native vibrancy; the panel adds the token tint and edge. */
  .sidebar { border-radius: var(--r-xl); background: var(--mat-sidebar); box-shadow: inset 0 0 0 .5px var(--edge); }
  /* Liquid Glass: the panels are frosted fills on the window's glass (tokens.css thins them). */
  :global(html[data-liquid]) main { box-shadow: inset 0 0 0 .5px var(--edge); }
  .titlebar { height: var(--h-titlebar); }
  ul { display: grid; gap: 2px; list-style: none; margin: 0; padding: 0 var(--sp-2); outline: none; }
  ul:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: var(--r-md); }
  /* Same nav row as the main window's sidebar: 16px icon in --text-2, the selected one in --accent-text. */
  li { height: var(--h-row); display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-2); border-radius: var(--r-md); }
  li:hover:not(.selected) { background: var(--fill-hover); }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; color: var(--text-2); }
  li.selected .ic { color: var(--accent-text); }
  :global(html[data-liquid]) li.selected .ic { color: var(--on-accent); }
  /* Stronger than --accent-soft-strong: the sidebar sits on see-through material, which thins a pale wash. */
  li.selected { background: color-mix(in srgb, var(--accent) 28%, transparent); }
  /* On Liquid Glass, tinted or not, a wash of the accent vanishes into the glass: a solid fill, like a focused Finder sidebar. */
  :global(html[data-liquid]) li.selected { background: var(--accent); color: var(--on-accent); }
  main { border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: hidden;
    display: grid; grid-template-rows: auto minmax(0, 1fr); }
  /* Only the page scrolls: the title stays put, like the main window. */
  /* relative: screen-reader-only text (position: absolute) stays inside the scroller instead of stretching the window. */
  .content { position: relative; padding: 0 var(--sp-5) var(--sp-5); overflow: auto; scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  .content::-webkit-scrollbar { width: 8px; }
  .content::-webkit-scrollbar-thumb { border-radius: 4px; background: var(--line-strong); border: 2px solid transparent; background-clip: content-box; }
  header { height: var(--h-toolbar); display: flex; align-items: center; padding: 0 var(--sp-5); }
  /* Let drags on the title fall through to the header's drag region. */
  h1 { pointer-events: none; margin: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  .soon { margin: auto; max-width: 360px; padding-bottom: var(--h-toolbar); text-align: center; } /* centred in the panel, not the scroller */
  .soon h2 { margin-bottom: var(--sp-1); }
  .soon p { margin: 0 0 var(--sp-3); color: var(--text-2); }
  .tag { display: inline-flex; align-items: center; height: 20px; padding: 0 var(--sp-2); border-radius: var(--r-full); background: var(--fill-hover); color: var(--text-2);
    font-size: var(--fs-caption); font-weight: var(--fw-medium); }
</style>
