<!-- MYNA_DEBUG=1: the window's size (logical px, as tauri.conf.json takes it) while it's resized. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let size = $state("");
  onMount(() => {
    let timer: ReturnType<typeof setTimeout>;
    const show = () => {
      size = `${window.innerWidth} × ${window.innerHeight}`;
      clearTimeout(timer);
      timer = setTimeout(() => (size = ""), 1200);
    };
    let on = false;
    invoke<boolean>("debug_on").then((v) => (on = v) && window.addEventListener("resize", show)).catch(() => {});
    return () => on && window.removeEventListener("resize", show);
  });
</script>

{#if size}<output class="size" aria-label="Window size">{size}</output>{/if}

<style>
  .size { position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); z-index: var(--z-toast); pointer-events: none;
    padding: var(--sp-2) var(--sp-3); border-radius: var(--r-full); background: var(--hud-bg-solid); color: var(--hud-fg);
    box-shadow: var(--shadow-hud); font: var(--fw-semibold) var(--fs-title3) / 1 var(--font); font-variant-numeric: tabular-nums; }
</style>
