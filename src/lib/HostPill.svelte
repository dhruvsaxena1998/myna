<!-- Whether text (or audio) goes off this Mac, for an AI provider (AI providers, Write's Cleanup model row, meeting speech-to-text). `off` is the backend's `off_device`. -->
<script lang="ts">
  let { off, what = "text" }: { off: boolean; what?: "text" | "audio" } = $props();
</script>

{#if off}<span class="pill caution">Sends {what} off this Mac</span>{:else}<span class="pill">Stays on this Mac</span>{/if}
