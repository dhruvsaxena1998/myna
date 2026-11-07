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

<section class="group" aria-labelledby="pill-h">
  <h2 id="pill-h">Pill</h2>
  <p class="help">Shows while you talk, then what happened. It never takes the cursor from the app you're typing in.</p>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="pill-style">Style</label><span class="desc">How the pill looks.</span></span>
      <select id="pill-style" class="select" value={look.style} onchange={(e) => save("style", e.currentTarget.value)}>
        {#each STYLES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <li class="row">
      <span><label class="name" for="pill-place">Position</label><span class="desc">Where it shows, on the screen you're typing on.</span></span>
      <select id="pill-place" class="select" value={look.place} onchange={(e) => save("place", e.currentTarget.value)}>
        {#each PLACES as [v, label]}<option value={v}>{label}</option>{/each}
      </select>
    </li>
    <!-- A small screen with the pill where it will show; fixed height so switching styles never moves the page. -->
    <li class="stage {look.place}" aria-label="Preview: {STYLES.find(([v]) => v === look.style)?.[1]} pill, {PLACES.find(([v]) => v === look.place)?.[1]}" role="img">
      <span class="pill" aria-hidden="true"><Pill style={look.style} pill={listening} {wave} secs={4} /></span>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .list { overflow: hidden; } /* the preview stage keeps the list's corners */
  /* The preview screen: a desktop-ish wash so the dark pill reads in both themes. */
  .stage { position: relative; height: 150px; display: flex; background: var(--stage-desk); }
  .pill { position: absolute; }
  .bottom .pill { bottom: var(--sp-4); left: 50%; translate: -50% 0; }
  .top .pill { top: var(--sp-3); left: 50%; translate: -50% 0; }
  .bottom-right .pill { bottom: var(--sp-4); right: var(--sp-4); }
  .top-right .pill { top: var(--sp-3); right: var(--sp-4); }
</style>
