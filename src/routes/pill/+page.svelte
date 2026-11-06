<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Pill, { WAVE, type Look, type Shown } from "$lib/Pill.svelte";
  import { glassOn, type Look as Appearance } from "$lib/look";

  let { data } = $props();

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
  // Tap mode counts down its last 30 s; the backend stops it at the limit.
  const left = $derived(pill.state === "listening" && pill.limit && !pill.hold && pill.limit - secs <= 30 ? Math.max(0, pill.limit - secs) : null);

  $effect(() => {
    invoke<Look>("pill_look_get").then((l) => (look = l), () => {}); // the default look until it answers
    const tick = setInterval(() => pill.state === "listening" && (secs = Math.floor((Date.now() - started) / 1000)), 250);
    const un = [
      listen<Shown | { state: "idle" }>("dictation", ({ payload }) => {
        clearTimeout(hide);
        on = payload.state !== "idle";
        if (payload.state === "idle") return;
        if (payload.state === "listening") (wave = Array(WAVE).fill(0)), (started = Date.now()), (secs = 0);
        pill = payload;
        // Results go away by themselves: long enough to read, errors longer.
        if (payload.state === "added") hide = setTimeout(() => (on = false), 1600);
        if (payload.state === "error") hide = setTimeout(() => (on = false), 4000);
      }),
      listen<number>("dictation-level", ({ payload }) => (wave = [...wave.slice(1), payload])),
      listen<Look>("pill-look", ({ payload }) => (look = payload)),
      listen<Appearance>("look", ({ payload }) => (appearance = payload)),
    ];
    return () => (clearInterval(tick), un.forEach((p) => p.then((f) => f())));
  });

  // Liquid Glass: tell the backend where the pill is drawn, so its native glass sits exactly behind it.
  function syncGlass() {
    const hud = spot.firstElementChild as HTMLElement | null;
    if (!appearance || !glassOn(appearance) || !hud) return void invoke("glass_set", { glass: [] });
    const r = hud.getBoundingClientRect(), css = getComputedStyle(hud);
    invoke("glass_set", { glass: [{ x: r.x, y: r.y, w: r.width, h: r.height, radius: Math.min(parseFloat(css.borderTopLeftRadius), r.height / 2), shown: on }] });
  }
  $effect(() => {
    void [appearance && glassOn(appearance), appearance?.glass, appearance?.accent, look.style, look.place, on]; // any of these moves or restyles the glass (the backend picks the tint)
    syncGlass();
  });
  $effect(() => {
    const ro = new ResizeObserver(() => syncGlass()); // the pill's words change its width
    ro.observe(spot.firstElementChild!);
    return () => ro.disconnect();
  });
</script>

<!-- The window sits against the same screen edges (pill.rs `place`), so the pill keeps clear of the menubar and Dock. -->
<div class="spot {look.place}" class:on role="status" bind:this={spot}>
  <Pill style={look.style} {pill} {wave} {secs} {left} />
</div>

<style>
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
