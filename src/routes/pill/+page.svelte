<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Pill, { WAVE, type Look, type Shown } from "$lib/Pill.svelte";

  let look = $state<Look>({ style: "capsule", place: "bottom" });
  // What the pill last said; kept while it fades out so the text doesn't vanish first.
  let pill = $state<Shown>({ state: "listening" });
  let on = $state(false);
  let wave = $state<number[]>(Array(WAVE).fill(0));
  let started = 0;
  let secs = $state(0);
  let hide: ReturnType<typeof setTimeout> | undefined;

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
    ];
    return () => (clearInterval(tick), un.forEach((p) => p.then((f) => f())));
  });
</script>

<!-- The window sits against the same screen edges (pill.rs `place`), so the pill keeps clear of the menubar and Dock. -->
<div class="spot {look.place}" class:on role="status">
  <Pill style={look.style} {pill} {wave} {secs} />
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
</style>
