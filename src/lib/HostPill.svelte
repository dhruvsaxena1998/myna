<!-- Whether text goes off this Mac, for an AI provider (AI providers, Write's Cleanup model row). `off` is the backend's `off_device`. -->
<script lang="ts">
  let { off }: { off: boolean } = $props();
</script>

{#if off}<span class="pill caution">Sends text off this Mac</span>{:else}<span class="pill">Stays on this Mac</span>{/if}
