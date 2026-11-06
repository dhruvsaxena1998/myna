<!-- Whether text goes off this Mac, for an AI provider (AI provider page, Voice typing's cleanup row). `off` is the backend's `off_device`. -->
<script lang="ts">
  let { off }: { off: boolean } = $props();
</script>

{#if off}
  <span class="pill caution"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 18h10a4 4 0 0 0 .6-7.95A5.5 5.5 0 0 0 7 8.5 4.8 4.8 0 0 0 7 18z" /></svg>Sends text off this Mac</span>
{:else}
  <span class="pill status"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5" width="16" height="11" rx="2" /><path d="M9 20h6M12 16v4" /></svg>Stays on this Mac</span>
{/if}

<style>
  .pill { display: inline-flex; align-items: center; gap: var(--sp-1); height: 20px; padding: 0 var(--sp-2); border-radius: var(--r-full); font-size: var(--fs-caption); font-weight: var(--fw-medium); white-space: nowrap; flex: none; }
  .pill.status { background: var(--accent-soft); color: var(--accent-text); }
  .pill.caution { background: var(--caution-soft); color: var(--caution-text); }
  .ic { width: 14px; height: 14px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
</style>
