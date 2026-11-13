<script lang="ts">
  let { data } = $props();
  import AiProviders from "$lib/AiProviders.svelte";
  import General from "$lib/General.svelte";
  import ErrorState from "$lib/ErrorState.svelte";
  import { glassOn, syncGlass } from "$lib/look";
  import ModelList from "$lib/ModelList.svelte";
  import UnloadAfter from "$lib/UnloadAfter.svelte";
  import Experimental from "$lib/Experimental.svelte";
  import PillLook from "$lib/PillLook.svelte";
  import Words, { type Rules } from "$lib/Words.svelte";
  import Fixes from "$lib/Fixes.svelte";
  import VoiceShortcut from "$lib/VoiceShortcut.svelte";
  import MeetingNotes from "$lib/MeetingNotes.svelte";
  import CleanupStyles, { type Cleanup } from "$lib/CleanupStyles.svelte";
  import Write from "$lib/Write.svelte";
  import StagePath from "$lib/StagePath.svelte";
  import type { Ai } from "$lib/AiProviders.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { dl, startDownload, watchDownloads } from "$lib/downloads.svelte";
  import { fmtSize, type Model, type Task } from "$lib/models";

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
  // Drill-ins: a sub-page under its parent, with its own toolbar title. Pages open them with `open`. `up`: a drill-in it sits under, for the breadcrumbs.
  const SUBS: Record<string, { parent: PageId; title: string; up?: string }> = {
    models: { parent: "understand", title: "Speech-to-text · Voice typing" },
    mmodels: { parent: "meetings", title: "Speech-to-text · Meeting notes" },
    diar: { parent: "meetings", title: "Speaker diarization" },
    remember: { parent: "understand", title: "Custom vocabulary" },
    keep: { parent: "understand", title: "Filler exceptions" },
    styles: { parent: "write", title: "Cleanup styles" },
    style: { parent: "write", title: "", up: "styles" }, // the style's own name
    add: { parent: "ai", title: "Add a provider" },
  };

  // Shared by the shortcut rows and the pill preview, which shows the same hint as the real pill.
  let voice = $derived(data.voice?.ok ? data.voice.voice : null); // assignable: saves update it
  let notes = $derived(data.notes?.ok ? data.notes.notes : null); // assignable: saves update it
  // Adding a fix changes it behind Words' back, so read it again.
  let rules = $derived(data.rules?.ok ? data.rules.rules : null);
  const reloadRules = async () => (rules = await invoke<Rules>("rules_get").catch(() => rules));
  // Shared by Write's rows, the styles list and the style editor, so they always agree.
  let cleanup = $derived(data.cleanup?.ok ? data.cleanup.cleanup : null); // assignable: saves return the styles as kept
  // What Myna adds about spelling, so it shows in the editor with no wait. Read again when the words change.
  let spell = $derived(data.spell); // assignable
  $effect(() => {
    void rules;
    invoke<string>("cleanup_spell").then((v) => (spell = v), () => {});
  });
  // Shared by Understand's model row and the model list's drill-in, so a pick shows on the way back.
  const stt = $derived(data.models?.ok ? data.models.tasks[0] : null);
  let sttActive = $derived(stt?.active ?? null); // assignable: the list changes it
  // Meeting notes' own pick, from the same catalog; voice typing's until one is saved, so a voice typing pick can move it.
  const meet = $derived(data.models?.ok ? data.models.tasks.find((t) => t.id === "meeting_stt") : null);
  let meetActive = $derived(meet?.active ?? null); // assignable
  // A pick or removal in one list can move the other's (a deleted model is forgotten by both); each list keeps its own.
  const reloadPicks = (from: "stt" | "meeting_stt") =>
    invoke<Task[]>("models_list").then((t) => {
      const other = t.find((x) => x.id === (from === "stt" ? "meeting_stt" : "stt"));
      if (other) from === "stt" ? (meetActive = other.active) : (sttActive = other.active);
    }, () => {});
  // The speaker diarization models, picked in their own drill-in.
  const diar = $derived(data.models?.ok ? data.models.tasks.find((t) => t.id === "diar") : null);
  let diarActive = $derived(diar?.active ?? null); // assignable
  // The browser add-on, kept here so it is current when Meeting notes is opened again; it can connect while another page shows.
  let addonOn = $derived(data.addon?.ok ? data.addon.addon.connected : false); // assignable
  let addonCode = $derived(data.addon?.ok ? data.addon.addon.code : ""); // assignable
  onMount(() => {
    const off = listen<boolean>("addon-status", (e) => (addonOn = e.payload));
    return () => void off.then((f) => f());
  });
  onMount(watchDownloads);
  const installed = (m: Model) => dl.installed[m.id] ?? m.installed;
  const modelState = (m: Model) => `${m.name} · ${installed(m) ? (m.system ? "Built in" : `Installed · ${fmtSize(m.size)}`) : m.id in dl.pct ? `Downloading ${dl.pct[m.id]}%` : `Not downloaded${m.system ? "" : ` · ${fmtSize(m.size)}`}`}`;
  let sttError = $state(""); // a download that couldn't start
  let unloadError = $state("");
  // Shared by General (look, the experimental switch) and Experimental (Liquid Glass): one look, saved whole.
  let look = $derived(data.appearance.ok ? data.appearance.look : null);
  // AI providers changes it; read it again on the way out, so the pages after it (Listen's and Write's cleanup rows) never show the old one.
  let ai = $derived(data.ai?.ok ? data.ai.ai : null);
  const nav = $derived(PAGES.filter((p) => p.id !== "experimental" || look?.experimental));

  // One history across pages and drill-ins, like System Settings' ‹ ›.
  type View = { page: PageId; sub: string | null; arg: string | null; title?: string };
  let hist = $state<{ list: View[]; i: number }>({ list: [{ page: "listen", sub: null, arg: null }], i: 0 });
  const view = $derived(hist.list[hist.i]);
  const title = $derived(
    view.sub === "style" ? (cleanup?.styles.find((x) => x.id === view.arg)?.name ?? "") : view.sub ? (view.title ?? SUBS[view.sub]?.title ?? "") : PAGES.find((p) => p.id === view.page)!.title,
  );
  let scroller: HTMLElement;
  // Listen, Understand and Write (not their drill-ins) are headed by the stage path instead of a title.
  const stage = $derived(!view.sub && (view.page === "listen" || view.page === "understand" || view.page === "write") ? view.page : null);
  const sttPick = $derived(stt?.models.find((m) => m.id === sttActive));
  const fastRoute = $derived(ai?.fast && !ai.fast.gone ? ai.fast : null);

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
  // Where a drill-in sits: its page, then the drill-in above it, if any. Each one goes there.
  const crumbs = $derived.by(() => {
    if (!view.sub) return [];
    const up = SUBS[view.sub]?.up;
    return [
      { title: PAGES.find((p) => p.id === view.page)!.title, to: () => go(view.page) },
      ...(up ? [{ title: SUBS[up].title, to: () => open(up) }] : []),
    ];
  });
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
      {#if stage && voice}
        <h1 class="sr-only">{title}</h1>
        <StagePath
          page={stage} on={voice.on} mode={voice.mode} accel={voice.key || null} paste={voice.paste} {go}
          model={sttPick?.name ?? null} installed={!!sttPick && installed(sttPick)} pct={sttPick ? dl.pct[sttPick.id] : undefined}
          fast={fastRoute ? { model: fastRoute.model, off: !!ai?.providers.find((p) => p.id === fastRoute.provider)?.off_device } : null} />
      {:else}
        <nav class="crumbs" aria-label="Breadcrumbs">
          {#each crumbs as c}<button class="crumb" onclick={c.to}>{c.title}</button><svg class="sep" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg>{/each}
          <h1>{title}</h1>
        </nav>
      {/if}
    </header>
    <div class="content" bind:this={scroller}>
      {#key hist.i + ":" + view.page + ":" + view.sub + ":" + view.arg}
        <div class="page prefs">

          <!-- Every setting lives in the database, so if it failed to open, every page shows why. -->
          {#if !data.storage.ok}
            <ErrorState title="Myna couldn't open its data folder" message={data.storage.error} />
          {:else if view.sub === "models"}
            {#if stt}<ModelList task={stt} label="Speech-to-text model" help="Used for voice typing. Downloads are shared with meeting notes." bind:active={sttActive} onchange={() => reloadPicks("stt")} />{/if}
          {:else if view.sub === "mmodels"}
            {#if meet}<ModelList task={meet} label="Meeting notes speech-to-text model" help="Used for meeting notes. Downloads are shared with voice typing." bind:active={meetActive} onchange={() => reloadPicks("meeting_stt")} />{/if}
          {:else if view.sub === "diar"}
            {#if diar}<ModelList task={diar} label="Speaker diarization model" help="Works out who said what. Runs on this Mac. Your own voice is always “Me”." search={false} bind:active={diarActive} />{/if}
          {:else if view.sub === "remember" || view.sub === "keep"}
            {#if rules}<Words bind:rules part={view.sub} />
            {:else}<ErrorState title="Myna couldn't read your words" message={data.rules?.ok === false ? data.rules.error : ""} />{/if}
          {:else if view.sub === "styles" || view.sub === "style"}
            {#if cleanup}<CleanupStyles bind:cleanup part={view.sub === "styles" ? "list" : "editor"} id={view.arg} {spell} {open} {drop} />
            {:else}<ErrorState title="Myna couldn't read the cleanup styles" message={data.cleanup?.ok === false ? data.cleanup.error : ""} />{/if}
          {:else if view.sub === "add"}
            {#if ai}<AiProviders bind:ai part="add" {back} />
            {:else}<ErrorState title="Myna couldn't read its AI providers" message={data.ai?.ok === false ? data.ai.error : ""} />{/if}
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
            {#if !stt || !data.models?.ok}
              <ErrorState title="Myna couldn't list its models" message={data.models?.ok === false ? data.models.error : ""} />
            {:else}
              {@const pick = stt.models.find((m) => m.id === sttActive)}
              {@const nDl = stt.models.filter((m) => m.id in dl.pct).length}
              {@const nFail = stt.models.filter((m) => dl.errors[m.id]).length}
              {@const other = [nDl && `${nDl} downloading`, nFail && `${nFail} failed`].filter(Boolean).join(", ")}
              {@const state = pick ? modelState(pick) : "Not set"}
              <section aria-labelledby="stt-h">
                <h2 id="stt-h">Speech-to-text</h2>
                {#if !voice?.on}<p class="help">Voice typing is off. Turn it on in Listen. Meeting notes use their own model.</p>{/if}
                <ul class="list">
                  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
                  <li class="row go" onclick={() => open("models")}>
                    <span><span class="name">Speech-to-text model</span><span class="desc" title={state}>{state}</span></span>
                    <span class="end">
                      {#if other}<span class="meta" class:bad={nFail}>{other}</span>{/if}
                      {#if pick && !installed(pick) && !(pick.id in dl.pct) && !pick.needs && (pick.files.length || pick.system || pick.sdk)}
                        <button class="btn" onclick={async (e) => { e.stopPropagation(); sttError = await startDownload(pick.id); }}>Download</button>
                      {/if}
                      <button class="btn" onclick={(e) => { e.stopPropagation(); open("models"); }}>All models</button>
                    </span>
                  </li>
                  <UnloadAfter initial={data.models.unload} bind:error={unloadError} />
                </ul>
                <p class="err" role="status">{unloadError || sttError}</p>
              </section>
              {#if voice?.on}
                {#if rules}<Words bind:rules part="rows" {open} />
                {:else}<ErrorState title="Myna couldn't read your words" message={data.rules?.ok === false ? data.rules.error : ""} />{/if}
                <Fixes initial={data.fixes} onadd={reloadRules} />
              {/if}
            {/if}
          {:else if view.page === "write"}
            {#if !voice}
              <ErrorState title="Myna couldn't read voice typing's settings" message={data.voice?.ok === false ? data.voice.error : ""} />
            {:else if !voice.on}
              <section><ul class="list"><li class="empty"><b>Voice typing is off</b><p>Turn it on in Listen to choose how your words are added.</p><button class="btn" onclick={() => go("listen")}>Go to Listen</button></li></ul></section>
            {:else}
              <Write bind:voice {ai} {cleanup} perms={data.perms} {go} {open} />
            {/if}
          {:else if view.page === "meetings"}
            {#if notes}<MeetingNotes bind:notes perms={data.perms} bind:connected={addonOn} bind:code={addonCode} loadError={data.addon?.ok === false ? data.addon.error : ""}>
              {#snippet models()}
                {#if meet || diar}
                  {@const pick = meet?.models.find((m) => m.id === meetActive)}
                  {@const state = pick ? modelState(pick) : "Not set"}
                  {@const dpick = diar?.models.find((m) => m.id === diarActive)}
                  {@const ddesc = dpick && !installed(dpick) ? (dpick.id in dl.pct ? `Downloading ${dl.pct[dpick.id]}%` : `Not downloaded · ${fmtSize(dpick.size)}`) : "Works out who said what. Runs on this Mac."}
                  <section aria-labelledby="mstt-h">
                    <h2 id="mstt-h">Models</h2>
                    <ul class="list">
                      {#if meet}
                        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
                        <li class="row go" onclick={() => open("mmodels")}>
                          <span><span class="name">Speech-to-text model</span><span class="desc" title={state}>{state}</span></span>
                          <span class="end"><button class="btn" onclick={(e) => { e.stopPropagation(); open("mmodels"); }}>All models</button></span>
                        </li>
                      {/if}
                      {#if diar}
                        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the button inside takes the keys) -->
                        <li class="row go" onclick={() => open("diar")}>
                          <span><span class="name withmeta"><span>Speaker diarization</span>{#if dpick}<span class="meta">{dpick.name}</span>{/if}</span><span class="desc" title={ddesc}>{ddesc}</span></span>
                          <span class="end"><button class="btn" onclick={(e) => { e.stopPropagation(); open("diar"); }}>All models</button></span>
                        </li>
                      {/if}
                    </ul>
                  </section>
                {/if}
              {/snippet}
            </MeetingNotes>
            {:else}<ErrorState title="Myna couldn't read meeting notes' settings" message={data.notes?.ok === false ? data.notes.error : ""} />{/if}
          {:else if view.page === "general"}
            {#if look && data.updates?.ok}<General bind:look version={data.version} path={data.storage.path} prefs={data.updates.prefs} status={data.updates.status} />
            {:else}<ErrorState title="Myna couldn't read its settings" message={!data.appearance.ok ? data.appearance.error : data.updates?.ok === false ? data.updates.error : ""} />{/if}
          {:else if view.page === "experimental"}
            {#if look}<Experimental bind:look />{/if}
          {:else if view.page === "ai"}
            {#if ai}<AiProviders bind:ai part="page" {open} />
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
  li[role="option"]:hover:not(.selected) { background: var(--fill-hover); }
  li .ic { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; color: var(--text-2); }
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
  /* A new view fades in, in place: switching pages never moves the content. */
  .page { animation: enter var(--dur-fast) var(--ease-out); }
  @keyframes enter { from { opacity: 0; } }
  header { position: relative; height: var(--h-toolbar); display: flex; align-items: center; gap: var(--sp-1); min-width: 0; padding: 0 var(--sp-5); }
  /* The stage path (about 44px) sits in the same 56px toolbar as every other page's title, so ‹ › and the content never move between pages. */
  .hnav { display: flex; gap: 2px; flex: none; margin: 0 var(--sp-2) 0 calc(-1 * var(--sp-2)); }
  .hnav .btn { width: 28px; height: 28px; color: var(--text); }
  /* Fainter than --text-disabled: next to an enabled arrow, a dimmed one has to read as off at a glance. */
  .hnav .btn:disabled { color: var(--text); opacity: .25; }
  /* Let drags on the title fall through to the header's drag region. */
  /* A drill-in's title follows the pages above it, each a quiet link back, all in the title's size so the toolbar never changes height. */
  .crumbs { flex: 1; display: flex; align-items: center; gap: var(--sp-1); min-width: 0; }
  .crumb { flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 0 var(--sp-1); margin: 0 calc(-1 * var(--sp-1)); border: 0; border-radius: var(--r-sm); background: none;
    font: inherit; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); color: var(--text-3); transition: background-color var(--dur-fast) var(--ease-in), color var(--dur-fast) var(--ease-in); }
  .crumb:hover { color: var(--text-2); background: var(--fill-hover); }
  .crumb:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  .sep { width: 14px; height: 14px; flex: none; fill: none; stroke: var(--text-3); stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  h1 { pointer-events: none; margin: 0; min-width: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* Empty state: a heading, one sentence, one action. */
  .empty { display: grid; justify-items: start; gap: var(--sp-1); padding: var(--sp-4); }
  .empty b { font-weight: var(--fw-semibold); }
  .empty p { margin: 0 0 var(--sp-2); color: var(--text-2); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  .meta { font-size: var(--fs-footnote); color: var(--text-3); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .withmeta { display: flex; align-items: center; gap: var(--sp-2); min-width: 0; }
  .meta.bad { color: var(--danger-text); }
  @container settings (max-width: 480px) { .content { padding: 0 var(--sp-4) var(--sp-6); } }
</style>
