<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { fade } from "svelte/transition";
  import Pill, { WAVE, type Look, type Shown } from "$lib/Pill.svelte";
  import { glassOn, type Look as Appearance } from "$lib/look";

  let { data } = $props();
  const THUMB = '<path d="M4 11h3v9H4zM7 11l3.5-7a2 2 0 0 1 2 2v3.5h5.2a2 2 0 0 1 2 2.3l-1.1 6a2 2 0 0 1-2 1.7H7"/>'; // as Thumbs.svelte

  let look = $state<Look>({ style: "capsule", place: "bottom" });
  // Liquid Glass and what tints it; the layout already loaded them before first paint.
  let appearance = $derived<Appearance | null>(data.appearance.ok ? data.appearance.look : null); // assignable: the "look" event updates it
  let spot: HTMLElement;
  // What the pill last said; kept while it fades out so the text doesn't vanish first.
  let pill = $state<Shown>({ state: "listening", hold: true, key: "Alt+Space" });
  let on = $state(false);
  let wave = $state<number[]>(Array(WAVE).fill(0));
  let started = 0;
  let secs = $state(0);
  let hide: ReturnType<typeof setTimeout> | undefined;
  // Ask how it went: 👍/👎 for the dictation just added, then "Fix it" after a 👎.
  let ask = $state<{ trace: number; rated: "up" | "down" | null } | null>(null);
  let askEl = $state<HTMLElement>();
  const close = (after: number) => (clearTimeout(hide), (hide = setTimeout(() => ((on = false), (ask = null)), after)));
  // The window takes the mouse only over these buttons (pill.rs `pill_hit`); the rest stays click-through.
  $effect(() => {
    if (!ask || !askEl) return void invoke("pill_hit", { rect: null });
    const r = askEl.getBoundingClientRect();
    invoke("pill_hit", { rect: [r.x, r.y, r.width, r.height] });
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
    const tick = setInterval(() => pill.state === "listening" && (secs = Math.floor((Date.now() - started) / 1000)), 250);
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
      listen<number>("dictation-ask", ({ payload }) => ((ask = { trace: payload, rated: null }), (on = true), close(4000))),
      listen<number>("dictation-level", ({ payload }) => (wave = [...wave.slice(1), payload])),
      listen<Look>("pill-look", ({ payload }) => (look = payload)),
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
    if (ask && askEl) {
      const a = askEl.getBoundingClientRect();
      glass.push({ x: a.x, y: a.y, w: a.width, h: a.height, radius: Math.min(parseFloat(getComputedStyle(askEl).borderTopLeftRadius), a.height / 2), shown: on });
    }
    invoke("glass_set", { glass });
  }
  $effect(() => {
    void [appearance && glassOn(appearance), appearance?.glass, appearance?.accent, look.style, look.place, on, ask?.rated, askEl]; // any of these moves or restyles the glass (the backend picks the tint)
    syncGlass();
  });
  $effect(() => {
    const ro = new ResizeObserver(() => syncGlass()); // the pill's words change its width
    ro.observe(spot.querySelector(".hud")!);
    return () => ro.disconnect();
  });
</script>

<!-- The window sits against the same screen edges (pill.rs `place`), so the pill keeps clear of the menubar and Dock. -->
<div class="spot {look.place}" class:on role="status" bind:this={spot}>
  <span class="anchor {look.style}">
  <Pill style={look.style} {pill} {wave} {secs} {left} />
  {#if ask}
    <!-- A trailing segment: beside the pill, not in its row, so the pill doesn't move; same height, corners and material. Hovering keeps it up. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ask" bind:this={askEl} in:fade={{ duration: 120 }} onmouseenter={() => clearTimeout(hide)} onmouseleave={() => close(1500)}>
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

<style>
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
