<script lang="ts">
  import "../../design/tokens.css";
  import { listen } from "@tauri-apps/api/event";
  import DebugSize from "$lib/DebugSize.svelte";
  import { applyLook, type Look } from "$lib/look";
  let { children } = $props();

  // A change in Settings restyles every window at once.
  $effect(() => {
    const un = listen<Look>("look", ({ payload }) => applyLook(payload));
    return () => void un.then((f) => f());
  });
</script>

{@render children()}
<DebugSize />

<style>
  /* Windows are transparent so native vibrancy shows through. */
  :global(html), :global(body) { background: transparent; margin: 0; height: 100%; cursor: default; user-select: none; }
</style>
