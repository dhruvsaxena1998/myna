<!-- Whether text goes off this Mac, for an AI provider (AI provider page, Voice typing's cleanup row). `off` is the backend's `off_device`. -->
<script lang="ts">
  let { off }: { off: boolean } = $props();
</script>

{#if off}
  <span class="pill caution"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 18h10a4 4 0 0 0 .6-7.95A5.5 5.5 0 0 0 7 8.5 4.8 4.8 0 0 0 7 18z" /></svg>Sends text off this Mac</span>
{:else}
  <span class="pill"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="2" /><path d="M9 20h6M12 16v4" /></svg>Stays on this Mac</span>
{/if}
