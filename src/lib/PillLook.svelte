<!-- Voice typing › Pill: which style, and where on the screen, with a preview of the choice. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Pill, { PLACES, STYLES, WAVE, type Look, type Shown } from "./Pill.svelte";

  let { initial, listening }: { initial: Look; listening: Shown } = $props();
  let look = $derived({ ...initial }); // assignable: a failed save reverts it
  let error = $state("");
  // A still frame of someone talking, for the preview.
  const wave = Array.from({ length: WAVE }, (_, i) => Math.max(0.05, 0.5 + 0.4 * Math.sin(i * 0.9) * Math.sin(i * 0.23)));

  async function save(key: keyof Look, value: string) {
    const was = look;
    look = { ...look, [key]: value };
    error = "";
    try {
      await invoke("pill_look_set", look);
    } catch (err) {
      look = was;
      error = String(err);
    }
  }
</script>

<section aria-labelledby="pill-h">
  <h2 id="pill-h">Pill</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="pill-style">Style</label><span class="desc" title="Shows while you talk, then what happened.">Shows while you talk, then what happened.</span></span>
      <select id="pill-style" class="select" value={look.style} onchange={(e) => save("style", e.currentTarget.value)}>
        {#each STYLES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <li class="row">
      <span><label class="name" for="pill-place">Position</label><span class="desc" title="On the screen you’re typing on.">On the screen you’re typing on.</span></span>
      <select id="pill-place" class="select" value={look.place} onchange={(e) => save("place", e.currentTarget.value)}>
        {#each PLACES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <!-- A neutral desk with the pill where it will show; fixed height so switching styles never moves the page. -->
    <li class="stage {look.place}" aria-label="Preview: {STYLES.find(([v]) => v === look.style)?.[1]} pill, {PLACES.find(([v]) => v === look.place)?.[1]}" role="img">
      <span class="at" aria-hidden="true"><Pill style={look.style} pill={listening} {wave} secs={4} /></span>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  /* The desk is inset in the list, like docs/settings.html's .stage-row; the divider above it comes from the list. */
  .stage { position: relative; height: 128px; margin: 0 var(--sp-3) var(--sp-3); border-radius: var(--r-md); background: var(--fill-field); box-shadow: inset 0 0 0 .5px var(--line); overflow: hidden; }
  .at { position: absolute; }
  .bottom .at { bottom: var(--sp-3); left: 50%; translate: -50% 0; }
  .top .at { top: var(--sp-3); left: 50%; translate: -50% 0; }
  .bottom-right .at { bottom: var(--sp-3); right: var(--sp-3); }
  .top-right .at { top: var(--sp-3); right: var(--sp-3); }
</style>
