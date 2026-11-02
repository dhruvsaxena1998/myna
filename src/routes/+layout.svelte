<script lang="ts">
  import "../../design/tokens.css";
  let { children } = $props();
</script>

{@render children()}

<style>
  /* Windows are transparent so native vibrancy shows through. */
  :global(html), :global(body) { background: transparent; margin: 0; height: 100%; cursor: default; user-select: none; }
</style>
