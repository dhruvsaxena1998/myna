<!-- A shortcut's keycaps inside one button: it shows the key and changes it (docs/settings.html, recorder). Recorder behaviour: recorder.svelte.ts. -->
<script lang="ts">
  import { keycaps, spoken } from "./shortcut";

  // `name`: what it's for, e.g. "Dictate shortcut". The id is how the recorder returns focus to it.
  let { id, name, key, recording, onclick }: { id: string; name: string; key: string | null | undefined; recording: boolean; onclick: () => void } = $props();
</script>

<button class="btn" class:rec={recording} {id} aria-live={recording ? "polite" : undefined} {onclick}
  aria-label={recording ? undefined : key ? `${name}: ${spoken(key)}` : `Set the ${name}`}>
  {#if recording}Press keys…
  {:else if key}<span class="keys">{#each keycaps(key) as k}<kbd>{k}</kbd>{/each}</span>
  {:else}Set key{/if}
</button>

<style>
  /* Recording: the focus ring sits inside the button, so it never changes its size. */
  .btn.rec { box-shadow: inset 0 0 0 2px var(--focus-ring); color: var(--text-2); }
</style>
