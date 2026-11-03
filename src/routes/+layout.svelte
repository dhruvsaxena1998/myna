<script lang="ts">
  import "../../design/tokens.css";
  import DebugSize from "$lib/DebugSize.svelte";
  let { children } = $props();
</script>

{@render children()}
<DebugSize />

<style>
  /* Windows are transparent so native vibrancy shows through. */
  :global(html), :global(body) { background: transparent; margin: 0; height: 100%; cursor: default; user-select: none; }
</style>
