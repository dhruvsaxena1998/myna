<!-- 👍/👎 for one dictation: line icons, a pressed one clears when clicked again. -->
<script lang="ts">
  import type { Rating } from "./feedback";

  let { rating, onrate }: { rating: Rating; onrate: (r: Rating) => void } = $props();
  let down: HTMLButtonElement;
  export const focusDown = () => down.focus();

  const THUMB = '<path d="M4 11h3v9H4zM7 11l3.5-7a2 2 0 0 1 2 2v3.5h5.2a2 2 0 0 1 2 2.3l-1.1 6a2 2 0 0 1-2 1.7H7"/>';
</script>

<span class="thumbs">
  <button type="button" class="icon" aria-pressed={rating === "up"} aria-label="Good dictation" title="Good dictation" onclick={() => onrate(rating === "up" ? null : "up")}>
    <svg viewBox="0 0 24 24" aria-hidden="true">{@html THUMB}</svg></button>
  <button type="button" class="icon" bind:this={down} aria-pressed={rating === "down"} aria-label="Bad dictation" title="Bad dictation" onclick={() => onrate(rating === "down" ? null : "down")}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><g transform="rotate(180 12 12)">{@html THUMB}</g></svg></button>
</span>

<style>
  .thumbs { display: flex; gap: 2px; }
  svg { width: 16px; height: 16px; stroke: currentColor; fill: none; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .icon { width: var(--h-ctl); height: var(--h-ctl); display: grid; place-items: center; border: 0; border-radius: var(--r-md); background: none; color: var(--text-2); }
  .icon:hover { background: var(--fill-hover); }
  .icon[aria-pressed="true"] { background: var(--accent-soft); color: var(--accent-text); }
  .icon:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
</style>
