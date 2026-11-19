<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { fade } from "svelte/transition";
  import Pill, { WAVE, type Look, type Shown } from "$lib/Pill.svelte";
  import { glassOn, type Look as Appearance } from "$lib/look";

  let { data } = $props();
  const THUMB = '<path d="M4 11h3v9H4zM7 11l3.5-7a2 2 0 0 1 2 2v3.5h5.2a2 2 0 0 1 2 2.3l-1.1 6a2 2 0 0 1-2 1.7H7"/>'; // as Thumbs.svelte

  let look = $state<Look>({ style: "capsule", place: "bottom" });
  // Liquid Glass and what tints it; the layout already loaded them before first paint.
  let appearance = $derived<Appearance | null>(data.appearance.ok ? data.appearance.look : null); // assignable: the "look" event updates it
  let spot: HTMLElement;
  let meetSpot: HTMLElement;
  // What the pill last said; kept while it fades out so the text doesn't vanish first.
  let pill = $state<Shown>({ state: "listening", hold: true, key: "Alt+Space" });
  let on = $state(false);
  let wave = $state<number[]>(Array(WAVE).fill(0));
  let meetLevel = $state(0); // the recording's mic loudness, 0 to 1, about 8 times a second
  let started = 0;
  let secs = $state(0);
  // A meeting recording: its start time, or null. Its pill shares the dictation pill's spot, so it steps aside while dictation shows.
  let meetStarted = $state<number | null>(null);
  let meetSecs = $state(0);
  let meetWords = $state(""); // how the mic is, in words, while a meeting records
  let mini = $state(false); // the recording pill hidden down to its level meter; a click brings it back
  let paused = $state(false); // the recording is paused (Pause on the pill)
  // A finished meeting's notes: being written up, saved, or failed. Shown in the same spot once the recording is over.
  let notes = $state<{ id: number; state: "transcribing" | "done" | "failed"; progress: number; error: string | null } | null>(null);
  let notesHide: ReturnType<typeof setTimeout> | undefined;
  // A detected call (calls.rs): the offer to record it, or the countdown to a stop after it ended.
  let call = $state<{ state: "offer"; title: string; label: string } | { state: "ending"; at: number } | null>(null);
  let now = $state(Date.now());
  let showPill = $state(true); // Settings › Meeting notes › Show the recording pill
  const shown = $derived<Shown | null>(
    meetStarted !== null && call?.state === "ending" ? { state: "ending", left: Math.max(0, Math.ceil((call.at - now) / 1000)) }
    : meetStarted !== null ? (showPill ? { state: "recording", words: meetWords, mini, paused } : null)
    : call?.state === "offer" ? { state: "detected", title: call.title, label: call.label }
    : !notes ? null
    : notes.state === "transcribing" ? { state: "notes", progress: notes.progress }
    : notes.state === "done" ? { state: "notes-saved" }
    : { state: "notes-failed", reason: notes.error ?? "Something went wrong." },
  );
  const meeting = $derived(shown !== null && !on);
  // Dragging the recording pill (or the notes being written, a detected call) moves the whole pill window, until the next dictation or recording puts it back in its place.
  let press: { x: number; y: number } | null = null;
  function down(e: PointerEvent) {
    if (meeting && !(e.target as Element).closest("button")) press = { x: e.screenX, y: e.screenY };
  }
  function drag(e: PointerEvent) {
    if (!press || e.buttons !== 1 || Math.hypot(e.screenX - press.x, e.screenY - press.y) < 4) return;
    press = null;
    getCurrentWindow().startDragging().catch(() => {});
  }
  function up() {
    if (press && mini) mini = false; // a click, not a drag: the dot opens back up
    press = null;
  }
  // Pause and Resume: the pill changes at once; the backend's "meeting" event confirms it.
  function pause() {
    paused = !paused;
    invoke("meeting_pause", { paused }).catch(() => (paused = !paused));
  }
  // Stop asks once: a stray click on a floating pill would end the meeting, and a stopped recording can't be resumed.
  let stopAsk = $state(false);
  let stopTimer: ReturnType<typeof setTimeout> | undefined;
  function stop() {
    clearTimeout(stopTimer);
    if (stopAsk) return void ((stopAsk = false), invoke("call_stop").catch(() => {}));
    stopAsk = true;
    stopTimer = setTimeout(() => (stopAsk = false), 3000);
  }
  const dismiss = () => (clearTimeout(notesHide), (notes = null));
  // The call's buttons: what the backend does, then the pill drops it at once.
  const callDo = (cmd: "call_start" | "call_dismiss" | "call_keep" | "call_stop") => ((call = null), invoke(cmd).catch(() => {}));
  function openNotes() {
    if (notes) invoke("meeting_open", { id: notes.id }); // the main window shows it (05.4)
    dismiss();
  }
  async function retry() {
    if (!notes) return;
    const id = notes.id;
    await invoke("meeting_retry", { id }).catch((e) => (notes = { id, state: "failed", progress: 0, error: String(e) }));
  }
  let hide: ReturnType<typeof setTimeout> | undefined;
  // Ask how it went: 👍/👎 for the dictation just added, then "Fix it" after a 👎.
  let ask = $state<{ trace: number; rated: "up" | "down" | null } | null>(null);
  let askEl = $state<HTMLElement>();
  const close = (after: number) => (clearTimeout(hide), (hide = setTimeout(() => ((on = false), (ask = null)), after)));
  // The window takes the mouse only over the 👍/👎 buttons, or the whole meeting pill (to drag it); the rest stays click-through (pill.rs `pill_hit`).
  // Measured again once the slide-in ends: measured mid-slide, the area sat 8px off the buttons and missed clicks on them.
  function hit() {
    const el = ask ? askEl : meeting ? meetSpot.querySelector<HTMLElement>(".hud") : null;
    if (!el) return void invoke("pill_hit", { rect: null });
    const r = el.getBoundingClientRect();
    invoke("pill_hit", { rect: [r.x, r.y, r.width, r.height] });
  }
  const slid = (e: TransitionEvent) => e.target === e.currentTarget && e.propertyName === "translate" && hit(); // not the level bars' transitions
  $effect(() => {
    void [shown?.state, mini, meetWords, stopAsk]; // the buttons move when the pill's words change
    hit();
  });
  async function rate(rating: "up" | "down") {
    if (!ask) return;
    const trace = ask.trace;
    ask = { trace, rated: rating };
    await invoke("feedback_set", { trace, rating, corrected: null }).catch(() => {});
    close(rating === "up" ? 1200 : 4000);
  }
  function fix() {
    if (ask) invoke("dictation_fix", { trace: ask.trace });
    close(0);
  }
  // Tap mode counts down its last 30 s; the backend stops it at the limit.
  const left = $derived(pill.state === "listening" && pill.limit && !pill.hold && pill.limit - secs <= 30 ? Math.max(0, pill.limit - secs) : null);

  $effect(() => {
    invoke<Look>("pill_look_get").then((l) => (look = l), () => {}); // the default look until it answers
    invoke<{ show_pill?: boolean } | null>("meeting_get").then((n) => (showPill = n?.show_pill !== false), () => {}); // missing: shown
    const tick = setInterval(() => {
      if (pill.state === "listening") secs = Math.floor((Date.now() - started) / 1000);
      if (call?.state === "ending") now = Date.now();
      if (meetStarted !== null) meetSecs = Math.max(0, Math.floor((Date.now() - meetStarted) / 1000));
    }, 250);
    const un = [
      listen<Shown | { state: "idle" }>("dictation", ({ payload }) => {
        clearTimeout(hide);
        ask = null;
        on = payload.state !== "idle";
        if (payload.state === "idle") return;
        if (payload.state === "listening") (wave = Array(WAVE).fill(0)), (started = Date.now()), (secs = 0);
        pill = payload;
        // Results go away by themselves: long enough to read, a fallback and its reason longer, errors longest.
        if (payload.state === "added") hide = setTimeout(() => (on = false), 1600);
        if (payload.state === "fallback") hide = setTimeout(() => (on = false), 2600);
        if (payload.state === "error") hide = setTimeout(() => (on = false), 4000);
      }),
      listen<{ started: number | null; mic: string | null; words: string | null }>("meeting", ({ payload }) => ((mini &&= payload.started !== null), (paused = payload.mic === "paused"), (meetStarted = payload.started), (meetWords = payload.words ?? ""), (meetSecs = payload.started === null ? 0 : Math.max(0, Math.floor((Date.now() - payload.started) / 1000))))),
      listen<NonNullable<typeof notes>>("meeting-notes", ({ payload }) => {
        clearTimeout(notesHide);
        notes = payload;
        if (payload.state === "done") notesHide = setTimeout(() => (notes = null), 8000); // a failure stays until it's dealt with
      }),
      listen<NonNullable<typeof call> | { state: "none" }>("call", ({ payload }) => (call = payload.state === "none" ? null : payload)),
      listen<number>("dictation-ask", ({ payload }) => ((ask = { trace: payload, rated: null }), (on = true), close(4000))),
      // Over the buttons (pill.rs tells us; the page can't see the pointer leave): up to 8 s, then 1.5 s after leaving. Never for good.
      listen<boolean>("pill-hover", ({ payload }) => ask && ask.rated !== "up" && close(payload ? 8000 : 1500)),
      listen<number>("dictation-level", ({ payload }) => (wave = [...wave.slice(1), payload])),
      listen<number>("meeting-level", ({ payload }) => (meetLevel = payload)),
      listen<Look>("pill-look", ({ payload }) => (look = payload)),
      listen<boolean>("meeting-show", ({ payload }) => (showPill = payload)),
      listen<Appearance>("look", ({ payload }) => (appearance = payload)),
    ];
    return () => (clearInterval(tick), un.forEach((p) => p.then((f) => f())));
  });

  // Liquid Glass: tell the backend where the pill is drawn, so its native glass sits exactly behind it.
  function syncGlass() {
    const hud = spot.querySelector<HTMLElement>(".hud");
    if (!appearance || !glassOn(appearance) || !hud) return void invoke("glass_set", { glass: [] });
    const r = hud.getBoundingClientRect(), css = getComputedStyle(hud);
    const glass = [{ x: r.x, y: r.y, w: r.width, h: r.height, radius: Math.min(parseFloat(css.borderTopLeftRadius), r.height / 2), shown: on }];
    const mh = meetSpot.querySelector<HTMLElement>(".hud");
    if (mh) {
      const m = mh.getBoundingClientRect();
      glass.push({ x: m.x, y: m.y, w: m.width, h: m.height, radius: Math.min(m.width, m.height) / 2, shown: meeting }); // the hidden pill stands upright
    }
    if (ask && askEl) {
      const a = askEl.getBoundingClientRect();
      glass.push({ x: a.x, y: a.y, w: a.width, h: a.height, radius: Math.min(parseFloat(getComputedStyle(askEl).borderTopLeftRadius), a.height / 2), shown: on });
    }
    invoke("glass_set", { glass });
  }
  $effect(() => {
    void [appearance && glassOn(appearance), appearance?.glass, appearance?.accent, look.style, look.place, on, meeting, meetStarted, notes?.state, call?.state, ask?.rated, askEl]; // any of these moves or restyles the glass (the backend picks the tint)
    syncGlass();
  });
  $effect(() => {
    const ro = new ResizeObserver(() => syncGlass()); // the pill's words change its width
    ro.observe(spot.querySelector(".hud")!);
    void [meetStarted, notes?.state, call?.state]; // the pill is drawn only while recording, offering or showing notes
    const mh = meetSpot.querySelector(".hud");
    if (mh) ro.observe(mh);
    return () => ro.disconnect();
  });
</script>

<!-- The window sits against the same screen edges (pill.rs `place`), so the pill keeps clear of the menubar and Dock. -->
<div class="spot {look.place}" class:on role="status" bind:this={spot} ontransitionend={slid}>
  <span class="anchor {look.style}">
  <Pill style={look.style} {pill} {wave} {secs} {left} />
  {#if ask}
    <!-- A trailing segment: beside the pill, not in its row, so the pill doesn't move; same height, corners and material. -->
    <div class="ask" bind:this={askEl} in:fade={{ duration: 120 }}>
      {#if ask.rated !== "down"}
        <button aria-label="Good dictation" title="Good dictation" aria-pressed={ask.rated === "up"} disabled={!!ask.rated} onclick={() => rate("up")}><svg viewBox="0 0 24 24" aria-hidden="true">{@html THUMB}</svg></button>
      {/if}
      {#if ask.rated !== "up"}
        <button aria-label="Bad dictation" title="Bad dictation" aria-pressed={ask.rated === "down"} disabled={!!ask.rated} onclick={() => rate("down")}><svg viewBox="0 0 24 24" aria-hidden="true"><g transform="rotate(180 12 12)">{@html THUMB}</g></svg></button>
      {/if}
      {#if ask.rated === "down"}<button class="fix" onclick={fix}>Fix it</button>{/if}
    </div>
  {/if}
  </span>
</div>
<!-- The recording isn't clickable yet (live notes will open from it); a detected call, its countdown and the written-up notes have buttons. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions (dragging is pointer-only; the window never takes keyboard focus) -->
<div class="spot {look.place}" class:on={meeting} class:drag={meeting} role="status" bind:this={meetSpot} ontransitionend={slid} onpointerdown={down} onpointermove={drag} onpointerup={up}>
  {#if shown}
    <Pill style="capsule" pill={shown} wave={[meetLevel]} secs={meetSecs}>
      {#snippet actions()}
        <span class="acts" class:call={shown.state === "detected" || shown.state === "ending"}>
          {#if shown.state === "recording"}<button class="act x" aria-label={paused ? "Resume recording" : "Pause recording"} title={paused ? "Resume" : "Pause"} onclick={pause}>
              <svg viewBox="0 0 24 24" aria-hidden="true">{#if paused}<path class="fill" d="M8 5.5v13l10.5-6.5z" />{:else}<path d="M9 6v12M15 6v12" />{/if}</svg></button>
            {#if stopAsk}<button class="act stop" aria-label="Stop recording now" onclick={stop}>Stop?</button>
            {:else}<button class="act x" aria-label="Stop recording" title="Stop" onclick={stop}><svg viewBox="0 0 24 24" aria-hidden="true"><rect class="fill" x="7" y="7" width="10" height="10" rx="1.5" /></svg></button>{/if}
            <button class="act x" aria-label="Hide recording pill" title="Hide (click it to bring it back)" onclick={() => (mini = true)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 12h12" /></svg></button>
          {:else if shown.state === "detected"}<button class="act primary" onclick={() => callDo("call_start")}>Start notes</button>
          {:else if shown.state === "ending"}<button class="act primary" onclick={() => callDo("call_keep")}>Keep recording</button><button class="act quiet" onclick={() => callDo("call_stop")}>Stop now</button>
          {:else if notes?.state === "failed"}<button class="act" onclick={retry}>Try again</button>
          {:else}<button class="act" onclick={openNotes}>Open</button>{/if}
          {#if shown.state !== "ending"}
            <button class="act x" aria-label="Dismiss" onclick={() => (shown.state === "detected" ? callDo("call_dismiss") : dismiss())}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg></button>
          {/if}
        </span>
      {/snippet}
    </Pill>
  {/if}
</div>

<style>
  .drag :global(.hud) { cursor: grab; }
  .acts { display: flex; gap: 2px; margin-right: -8px; }
  .act { height: 24px; padding: 0 10px; border: 0; border-radius: 12px; background: var(--hud-fill); color: var(--hud-fg); font: inherit; cursor: default; }
  .act:hover { background: color-mix(in srgb, var(--hud-fg) 22%, transparent); }
  .act:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
  /* A call's choice: set off by a hairline, one filled button that is the answer most of the time, the other quiet. */
  .acts.call { gap: 6px; margin-left: 4px; padding-left: 12px; box-shadow: inset 0.5px 0 0 var(--hud-edge); }
  .act.primary { height: 28px; padding: 0 14px; border-radius: 14px; background: var(--hud-fg); color: #1c1c1e; font-weight: var(--fw-medium); }
  .act.primary:hover { background: color-mix(in srgb, var(--hud-fg) 88%, transparent); }
  .act.stop { background: var(--rec); color: #fff; }
  .act.quiet { height: 28px; background: none; color: var(--hud-2); }
  .act.quiet:hover { background: var(--hud-fill); color: var(--hud-fg); }
  .act.x { width: 24px; padding: 0; display: grid; place-items: center; background: none; color: var(--hud-2); }
  .act.x svg { width: 14px; height: 14px; stroke: currentColor; fill: none; stroke-width: var(--ic-stroke); stroke-linecap: round; stroke-linejoin: round; }
  .act.x svg .fill { fill: currentColor; }
  /* A grid, not flex: WebKit sizes a flex wrapper without the card's 280px minimum, so the card overflowed it
     (under the segment, and off the window at the right). */
  .anchor { position: relative; display: inline-grid; flex: none; --seg-r: var(--r-full); }
  .anchor.wave { --seg-r: var(--r-lg); }
  /* Card: the card's full height and corners, with bigger buttons and padding so the segment doesn't look empty. */
  .anchor.card { --seg-r: var(--r-xl); }
  .anchor.card .ask { padding: 8px; }
  .anchor.card .ask button { height: 40px; border-radius: calc(var(--r-xl) - 8px); }
  /* The pill's own height, corners, fill, edge and shadow (Pill.svelte .hud), 4px off its side. */
  .ask { position: absolute; top: 0; bottom: 0; left: calc(100% + var(--sp-1)); display: flex; align-items: center; gap: 2px; padding: 4px; box-sizing: border-box;
    border-radius: var(--seg-r); background: var(--hud-bg); color: var(--hud-fg); white-space: nowrap;
    -webkit-backdrop-filter: blur(24px) saturate(180%); backdrop-filter: blur(24px) saturate(180%); box-shadow: inset 0 0 0 0.5px var(--hud-edge), var(--shadow-hud); }
  .bottom-right .ask, .top-right .ask { left: auto; right: calc(100% + var(--sp-1)); }
  /* Square, at most 32px, so the tall card keeps them hand-sized; corners nest inside the segment's. */
  .ask button { height: min(100%, 32px); aspect-ratio: 1; display: grid; place-items: center; padding: 0; border: 0; border-radius: max(4px, calc(var(--seg-r) - 4px));
    background: none; color: var(--hud-2); font: inherit; cursor: default; transition: background-color var(--dur-fast) var(--ease-out), color var(--dur-fast) var(--ease-out); }
  .ask button:hover:not(:disabled) { background: color-mix(in srgb, var(--hud-fill) 60%, transparent); color: var(--hud-fg); }
  /* Rated: the thumb turns bright, no fill, so a lone one doesn't read as a button in a ring. */
  .ask button[aria-pressed="true"] { color: var(--hud-fg); }
  .ask button:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: -2px; }
  .ask .fix { aspect-ratio: auto; padding: 0 var(--sp-3); background: var(--hud-fill); color: var(--hud-fg); font-weight: var(--fw-medium); }
  .ask .fix:hover { background: color-mix(in srgb, var(--hud-fg) 22%, transparent); }
  .ask svg { width: var(--ic); height: var(--ic); stroke: currentColor; fill: none; stroke-width: var(--ic-stroke); stroke-linecap: round; stroke-linejoin: round; }
  /* Liquid Glass: like the pill, only the native glass shows behind it. */
  :global(html[data-liquid]) .ask { background: rgba(28,28,30, calc(max(0, var(--frost, .5) - .5) * .9)); -webkit-backdrop-filter: none; backdrop-filter: none; box-shadow: none; }
  .spot { position: fixed; z-index: var(--z-hud); display: flex;
    opacity: 0; visibility: hidden; translate: 0 8px;
    transition: opacity var(--dur-fast) var(--ease-in), translate var(--dur-fast) var(--ease-in), visibility 0s var(--dur-fast); }
  .spot.on { opacity: 1; visibility: visible; translate: 0 0; transition: opacity var(--dur) var(--ease-out), translate var(--dur) var(--ease-out); }
  .bottom, .bottom-right { bottom: var(--sp-5); }
  .top, .top-right { top: var(--sp-3); }
  .top, .top-right { translate: 0 -8px; }
  .bottom, .top { left: 0; right: 0; justify-content: center; }
  .bottom-right, .top-right { right: var(--sp-5); }
  /* Liquid Glass: only the native glass shows, and the pill fades in place so the glass stays behind it. */
  :global(html[data-liquid]) .spot { translate: none; }
  /* Opacity slider: clear (default .5) shows only the glass; Tinted darkens the pill behind its words. */
  :global(html[data-liquid]) .spot :global(.hud) { background: rgba(28,28,30, calc(max(0, var(--frost, .5) - .5) * .9)); -webkit-backdrop-filter: none; backdrop-filter: none; box-shadow: none; }
</style>
