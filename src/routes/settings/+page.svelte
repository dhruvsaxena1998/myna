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
  import MeetingNotes from "$lib/MeetingNotes.svelte";
  import CleanupStyles from "$lib/CleanupStyles.svelte";
  import type { Ai } from "$lib/AiProviders.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";

  // Sidebar, in the order your voice goes through Myna (docs/settings.html, PAGES). Experimental is listed only while its switch in General is on.
  const ICON = {
    mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
    wave: '<path d="M4 10v4M8 7v10M12 4v16M16 8v8M20 11v2"/>',
    write: '<path d="M4 20h4L19 9a2.1 2.1 0 0 0-4-4L4 16z"/><path d="M13.5 6.5l4 4"/>',
    people: '<circle cx="9" cy="8" r="3"/><path d="M3.5 19a5.5 5.5 0 0 1 11 0"/><circle cx="16.5" cy="9" r="2.5"/><path d="M16.5 14.5a4 4 0 0 1 4 4.5"/>',
    gear: '<circle cx="12" cy="12" r="3"/><path d="M12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M5.6 18.4l1.8-1.8M16.6 7.4l1.8-1.8"/>',
    server: '<rect x="4" y="4" width="16" height="7" rx="2"/><rect x="4" y="13" width="16" height="7" rx="2"/><path d="M8 7.5h.01M8 16.5h.01"/>',
    flask: '<path d="M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.7 3h10.6a2 2 0 0 0 1.7-3l-5-9V3M7.5 15h9"/>',
  };
  type PageId = "listen" | "understand" | "write" | "meetings" | "general" | "ai" | "experimental";
  const PAGES: { id: PageId; title: string; icon: string; gap?: boolean }[] = [
    { id: "listen", title: "Listen", icon: ICON.mic },
    { id: "understand", title: "Understand", icon: ICON.wave },
    { id: "write", title: "Write", icon: ICON.write },
    { id: "meetings", title: "Meeting notes", icon: ICON.people, gap: true },
    { id: "general", title: "General", icon: ICON.gear, gap: true },
    { id: "ai", title: "AI providers", icon: ICON.server },
    { id: "experimental", title: "Experimental", icon: ICON.flask },
  ];
  // Drill-ins: a sub-page under its parent, with its own toolbar title. Pages open them with `open`.
  const SUBS: Record<string, { parent: PageId; title: string }> = {
    models: { parent: "understand", title: "Speech-to-text · Voice typing" },
  };

  // Shared by the shortcut rows and the pill preview, which shows the same hint as the real pill.
  let voice = $derived(data.voice?.ok ? data.voice.voice : null); // assignable: saves update it
  let notes = $derived(data.notes?.ok ? data.notes.notes : null); // assignable: saves update it
  // Adding a fix changes it behind Words' back, so read it again.
  let rules = $derived(data.rules?.ok ? data.rules.rules : null);
  const reloadRules = async () => (rules = await invoke<Rules>("rules_get").catch(() => rules));
  // Shared by General (look, the experimental switch) and Experimental (Liquid Glass): one look, saved whole.
  let look = $derived(data.appearance.ok ? data.appearance.look : null);
  // AI providers changes it; read it again on the way out, so the pages after it (Listen's and Write's cleanup rows) never show the old one.
  let ai = $derived(data.ai?.ok ? data.ai.ai : null);
  const nav = $derived(PAGES.filter((p) => p.id !== "experimental" || look?.experimental));

  // One history across pages and drill-ins, like System Settings' ‹ ›.
  type View = { page: PageId; sub: string | null; arg: string | null; title?: string };
  let hist = $state<{ list: View[]; i: number }>({ list: [{ page: "listen", sub: null, arg: null }], i: 0 });
  const view = $derived(hist.list[hist.i]);
  const title = $derived(view.sub ? (view.title ?? SUBS[view.sub]?.title ?? "") : PAGES.find((p) => p.id === view.page)!.title);
  let scroller: HTMLElement;

  async function show(v: View, push = true) {
    if (v.page === "experimental" && !look?.experimental) v = { page: "general", sub: null, arg: null };
    if (view.page === "ai" && v.page !== "ai") ai = await invoke<Ai>("ai_get").catch(() => ai);
    if (push) hist = { list: [...hist.list.slice(0, hist.i + 1), v], i: hist.i + 1 };
    else hist = { ...hist, list: hist.list.with(hist.i, v) };
    await tick();
    scroller?.scrollTo(0, 0);
  }
  const go = (page: PageId) => (page === view.page && !view.sub ? undefined : show({ page, sub: null, arg: null }));
  /** Opens a drill-in under the current page. `title` overrides the drill-in's own, e.g. a style's name. */
  const open = (sub: string, arg: string | null = null, title?: string) => show({ page: view.page, sub, arg, title });
  async function step(d: number) {
    const j = hist.i + d;
    if (j < 0 || j >= hist.list.length) return;
    if (view.page === "ai" && hist.list[j].page !== "ai") ai = await invoke<Ai>("ai_get").catch(() => ai);
    hist = { ...hist, i: j };
    await tick();
    scroller?.scrollTo(0, 0);
  }
  const back = () => step(-1);
  const same = (a: View, b: View) => a.page === b.page && a.sub === b.sub && a.arg === b.arg;
  /** Takes the views that are `gone` out of history (and the repeats that leaves). Showing one goes back to the view before it, or to `fallback`. */
  function prune(gone: (v: View) => boolean, fallback?: View) {
    const showing = gone(view);
    const kept: View[] = [];
    let at = 0;
    hist.list.forEach((v, k) => {
      if (!gone(v) && !(kept.length && same(kept[kept.length - 1], v))) kept.push(v);
      if (k === hist.i) at = kept.length - 1;
    });
    if (showing && fallback) kept.splice(++at, Infinity, fallback);
    if (!kept.length) kept.push(fallback ?? { page: "listen", sub: null, arg: null });
    hist = { list: kept, i: Math.max(0, Math.min(at, kept.length - 1)) };
  }
  /** A drill-in whose subject is deleted (a cleanup style) leaves the stack. */
  const drop = (sub: string, arg: string | null) => prune((v) => v.sub === sub && v.arg === arg);
  // The Experimental page goes when its switch does, from history too.
  $effect(() => {
    if (look && !look.experimental && hist.list.some((v) => v.page === "experimental")) prune((v) => v.page === "experimental", { page: "general", sub: null, arg: null });
  });
  $effect(() => {
    void [look && glassOn(look), look?.glass, look?.accent]; // the backend picks the tint
    syncGlass(look);
  });

  function navKeys(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const i = nav.findIndex((p) => p.id === view.page);
    go(nav[Math.max(0, Math.min(nav.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].id);
  }
  function winKeys(e: KeyboardEvent) {
    if (e.metaKey && (e.key === "[" || e.key === "]")) {
      e.preventDefault();
      step(e.key === "[" ? -1 : 1);
    } else if (e.key === "Escape" && view.sub && !e.defaultPrevented && !(e.target as Element).closest("input, textarea, select, [role=dialog]")) back();
  }
</script>

<svelte:window onkeydown={winKeys} />

<div class="window">
  <nav class="sidebar" aria-label="Settings">
    <div class="titlebar" data-tauri-drag-region></div>
    <ul role="listbox" tabindex="0" aria-label="Settings pages" onkeydown={navKeys}>
      <li class="cap" role="presentation">Voice typing</li>
      {#each nav as p (p.id)}
        {#if p.gap}<li class="gap" role="presentation"></li>{/if}
        <li role="option" aria-selected={view.page === p.id} class:selected={view.page === p.id} onclick={() => go(p.id)} onkeydown={() => {}}>
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true">{@html p.icon}</svg>{p.title}
        </li>
      {/each}
    </ul>
  </nav>

  <main>
    <header data-tauri-drag-region>
      <span class="hnav">
        <button class="btn quiet icon" aria-label="Back" title="Back (⌘[)" disabled={hist.i === 0} onclick={() => step(-1)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 6l-6 6 6 6" /></svg></button>
        <button class="btn quiet icon" aria-label="Forward" title="Forward (⌘])" disabled={hist.i === hist.list.length - 1} onclick={() => step(1)}><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
      </span>
      <h1>{title}</h1>
    </header>
    <div class="content" bind:this={scroller}>
      {#key hist.i + ":" + view.page + ":" + view.sub + ":" + view.arg}
        <div class="page prefs">
          <!-- Every setting lives in the database, so if it failed to open, every page shows why. -->
          {#if !data.storage.ok}
            <ErrorState title="Myna couldn't open its data folder" message={data.storage.error} />
          {:else if view.sub === "models"}
            <!-- Interim: the old Speech models page, until 18.4 and 18.8 split it. -->
            {#if data.models?.ok}
              <ModelTask task={data.models.tasks[0]} title="Turning speech into text" help="Used for voice typing and meeting notes." tools />
              <ModelTask task={data.models.tasks[1]} title="Telling voices apart" help="Works out who said what. Always runs on this Mac. Your own voice is always “Me”." />
            {/if}
          {:else if view.page === "listen"}
            {#if !voice}
              <ErrorState title="Myna couldn't read voice typing's settings" message={data.voice?.ok === false ? data.voice.error : ""} />
            {:else}
              <VoiceShortcut bind:voice globe={data.globe} {ai} perms={data.perms} setup={() => go("ai")} />
              {#if voice.on}
                {#if data.look?.ok}<PillLook initial={data.look.look} listening={{ state: "listening", hold: voice.mode === "hold", key: voice.key }} />
                {:else}<ErrorState title="Myna couldn't read the pill's look" message={data.look?.error ?? ""} />{/if}
              {/if}
            {/if}
          {:else if view.page === "understand"}
            {#if !data.models?.ok}
              <ErrorState title="Myna couldn't list its models" message={data.models?.error ?? ""} />
            {:else}
              {@const stt = data.models.tasks[0]}
              <section aria-labelledby="stt-h">
                <h2 id="stt-h">Speech-to-text</h2>
                {#if !voice?.on}<p class="help">Voice typing is off. Turn it on in Listen.</p>{/if}
                <ul class="list">
                  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
                  <li class="row go" onclick={() => open("models")}>
                    <span><span class="name">Speech-to-text model</span><span class="desc">{stt.models.find((m) => m.id === stt.active)?.name ?? "Not set"}</span></span>
                    <span class="end"><button class="btn" onclick={(e) => { e.stopPropagation(); open("models"); }}>All models</button></span>
                  </li>
                </ul>
              </section>
              <UnloadAfter initial={data.models.unload} />
              <!-- Interim: Fixes and Words stay as they were until 18.4 rebuilds them. -->
              {#if voice?.on}
                <Fixes initial={data.fixes} onadd={reloadRules} />
                {#if rules}<Words initial={rules} />
                {:else}<ErrorState title="Myna couldn't read your words" message={data.rules?.ok === false ? data.rules.error : ""} />{/if}
              {/if}
            {/if}
          {:else if view.page === "write"}
            {#if !voice}
              <ErrorState title="Myna couldn't read voice typing's settings" message={data.voice?.ok === false ? data.voice.error : ""} />
            {:else if !voice.on}
              <section><ul class="list"><li class="empty"><b>Voice typing is off</b><p>Turn it on in Listen to choose how your words are added.</p><button class="btn" onclick={() => go("listen")}>Go to Listen</button></li></ul></section>
            {:else}
              <!-- Interim: these rows move as 18.5 rebuilds Write. -->
              <PasteMethod bind:voice />
              <Feedback bind:voice />
              <!-- Styles only matter once there's a Fast model to run them. -->
              {#if ai?.fast && !ai.fast.gone}
                {#if data.cleanup?.ok}<CleanupStyles initial={data.cleanup.cleanup} />
                {:else}<ErrorState title="Myna couldn't read the cleanup styles" message={data.cleanup?.error ?? ""} />{/if}
              {/if}
              <Permissions initial={data.perms} kinds={["ax"]} />
            {/if}
          {:else if view.page === "meetings"}
            {#if notes}<MeetingNotes bind:notes perms={data.perms} />
            {:else}<ErrorState title="Myna couldn't read meeting notes' settings" message={data.notes?.ok === false ? data.notes.error : ""} />{/if}
          {:else if view.page === "general"}
            {#if look}<Look bind:look />
            {:else}<ErrorState title="Myna couldn't read its look" message={data.appearance.ok ? "" : data.appearance.error} />{/if}
            {#if data.updates?.ok}<About version={data.version} path={data.storage.path} prefs={data.updates.prefs} status={data.updates.status} />
            {:else}<ErrorState title="Myna couldn't read the update settings" message={data.updates?.ok === false ? data.updates.error : ""} />{/if}
            {#if look}<Experimental bind:look part="switch" />{/if}
          {:else if view.page === "experimental"}
            {#if look}<Experimental bind:look part="features" />{/if}
          {:else if view.page === "ai"}
            {#if ai}<AiProviders initial={ai} />
            {:else}<ErrorState title="Myna couldn't read its AI providers" message={data.ai?.ok === false ? data.ai.error : ""} />{/if}
          {/if}
        </div>
      {/key}
    </div>
  </main>
</div>

<style>
  .window { display: grid; grid-template-columns: var(--w-settings-nav) minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); gap: var(--win-inset);
    height: 100vh; box-sizing: border-box; padding: var(--win-inset); }
  /* Sidebar glass is the window's native vibrancy; the panel adds the token tint and edge. */
  .sidebar { display: flex; flex-direction: column; min-height: 0; border-radius: var(--r-xl); background: var(--mat-sidebar); box-shadow: inset 0 0 0 .5px var(--edge); }
  /* Liquid Glass: the panels are frosted fills on the window's glass (tokens.css thins them). */
  :global(html[data-liquid]) main { box-shadow: inset 0 0 0 .5px var(--edge); }
  .titlebar { height: var(--h-titlebar); flex: none; }
  ul { list-style: none; margin: 0; padding: 0 var(--sp-2) var(--sp-2); outline: none; overflow: auto; }
  ul:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; border-radius: var(--r-md); }
  .cap { margin: 0 0 var(--sp-1); padding: 0 10px; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-3); }
  .gap { height: var(--sp-4); }
  /* Same nav row as the main window's sidebar: 16px icon in --text-2, the selected one in --accent-text. */
  li[role="option"] { height: var(--h-row); display: flex; align-items: center; gap: var(--sp-2); padding: 0 10px; border-radius: var(--r-md); cursor: default;
    transition: background-color var(--dur-fast) var(--ease-in); }
  li:hover:not(.selected) { background: var(--fill-hover); }
  .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; color: var(--text-2); }
  li.selected .ic { color: var(--accent-text); }
  :global(html[data-liquid]) li.selected .ic { color: var(--on-accent); }
  /* Stronger than --accent-soft-strong: the sidebar sits on see-through material, which thins a pale wash. */
  li.selected { font-weight: var(--fw-medium); background: color-mix(in srgb, var(--accent) 28%, transparent); }
  /* On Liquid Glass, tinted or not, a wash of the accent vanishes into the glass: a solid fill, like a focused Finder sidebar. */
  :global(html[data-liquid]) li.selected { background: var(--accent); color: var(--on-accent); }
  main { container: settings / inline-size; min-width: 0; border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: hidden;
    display: grid; grid-template-rows: auto minmax(0, 1fr); }
  /* Only the page scrolls: the toolbar stays put, like the main window. */
  /* relative: screen-reader-only text (position: absolute) stays inside the scroller instead of stretching the window. */
  .content { position: relative; padding: 0 var(--sp-5) var(--sp-6); overflow: auto; scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  .content::-webkit-scrollbar { width: 8px; }
  .content::-webkit-scrollbar-thumb { border-radius: 4px; background: var(--line-strong); border: 2px solid transparent; background-clip: content-box; }
  /* A new view rises in. */
  .page { animation: enter var(--dur) var(--ease-out); }
  @keyframes enter { from { opacity: 0; transform: translateY(4px); } }
  header { height: var(--h-toolbar); display: flex; align-items: center; gap: var(--sp-1); min-width: 0; padding: 0 var(--sp-5); }
  .hnav { display: flex; gap: 2px; flex: none; margin: 0 var(--sp-2) 0 calc(-1 * var(--sp-2)); }
  .hnav .btn { width: 28px; height: 28px; color: var(--text); }
  .hnav .btn:disabled { color: var(--text-disabled); }
  /* Let drags on the title fall through to the header's drag region. */
  h1 { pointer-events: none; margin: 0; min-width: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* Empty state: a heading, one sentence, one action. */
  .empty { display: grid; justify-items: start; gap: var(--sp-1); padding: var(--sp-4); }
  .empty b { font-weight: var(--fw-semibold); }
  .empty p { margin: 0 0 var(--sp-2); color: var(--text-2); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  @container settings (max-width: 480px) { .content { padding: 0 var(--sp-4) var(--sp-6); } }
</style>
