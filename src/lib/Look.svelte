<!-- Look & appearance: light or dark, the highlight colour, and clear or tinted see-through panels. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { applyLook, type Look } from "./look";

  // Shared with Experimental features (Liquid Glass), so neither saves a stale copy of the other's choice.
  let { look = $bindable() }: { look: Look } = $props();
  let error = $state("");

  const THEMES = [["auto", "Auto"], ["light", "Light"], ["dark", "Dark"]];
  const ACCENTS = [["blue", "Blue"], ["purple", "Purple"], ["pink", "Pink"], ["red", "Red"], ["orange", "Orange"], ["yellow", "Yellow"], ["green", "Green"], ["graphite", "Graphite"]];
  const GLASSES = [["clear", "Clear"], ["tinted", "Tinted"]];

  async function save(key: keyof Look, value: string) {
    const was = look;
    look = { ...look, [key]: value };
    applyLook(look); // at once; the backend's "look" event tells the other windows
    error = "";
    try {
      await invoke("look_set", { look });
    } catch (err) {
      look = was;
      applyLook(was);
      error = String(err);
    }
  }
</script>

{#snippet seg(key: "theme" | "glass", choices: string[][], labelledby: string)}
  <fieldset class="seg" aria-labelledby={labelledby}>
    {#each choices as [v, label]}
      <label><input type="radio" name={key} value={v} checked={look[key] === v} onchange={() => save(key, v)} /><span data-l={label}>{label}</span></label>
    {/each}
  </fieldset>
{/snippet}

<section class="group" aria-label="Look">
  <ul class="list">
    <li class="row">
      <span><span class="name" id="theme-l">Theme</span><span class="desc">Auto follows your Mac.</span></span>
      {@render seg("theme", THEMES, "theme-l")}
    </li>
    <li class="row">
      <span><span class="name" id="accent-l">Highlight colour</span><span class="desc">Used for buttons and selected items. Now: {ACCENTS.find(([v]) => v === look.accent)?.[1]}.</span></span>
      <fieldset class="swatches" aria-labelledby="accent-l">
        {#each ACCENTS as [v, label]}
          <label class="sw"><input type="radio" name="accent" value={v} checked={look.accent === v} onchange={() => save("accent", v)} /><span aria-hidden="true" style="--c: var(--sw-{v})"></span><span class="sr">{label}</span></label>
        {/each}
      </fieldset>
    </li>
    <li class="row">
      <span><span class="name" id="glass-l">See-through panels</span><span class="desc">{look.glass === "clear" ? "The sidebar and menus show a little of what’s behind them." : "The sidebar and menus take on a hint of your highlight colour."}</span></span>
      {@render seg("glass", GLASSES, "glass-l")}
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  /* Swatches: 22px dots; the chosen one gets a ring in its own colour. */
  .swatches { display: flex; flex-wrap: wrap; gap: 10px; margin: 0; padding: 0; border: 0; }
  .sw { position: relative; display: block; }
  .sw input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .sw > span[aria-hidden] { display: block; width: 22px; height: 22px; border-radius: 50%; background: var(--c); box-shadow: inset 0 0 0 .5px rgba(0, 0, 0, .25);
    transition: box-shadow var(--dur) var(--ease-out); }
  .sw input:checked + span { box-shadow: 0 0 0 2px var(--bg-inset), 0 0 0 4px var(--c); }
  .sw input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 5px; }
</style>
