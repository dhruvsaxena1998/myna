<!-- Look & appearance: light or dark, the highlight colour, and clear or tinted see-through panels. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { applyLook, type Look } from "./look";

  let { initial }: { initial: Look } = $props();
  let look = $derived({ ...initial }); // assignable: a failed save reverts it
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

<section class="group" aria-labelledby="look-h">
  <h2 id="look-h">Appearance</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name" id="theme-l">Appearance</span><span class="desc">Auto follows your Mac’s light or dark setting.</span></span>
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
  .group { margin-bottom: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-3); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .list > li + li { border-top: 1px solid var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  /* Segmented control: a capsule track with a raised thumb. */
  .seg { display: inline-flex; gap: 2px; margin: 0; padding: 3px; border: 0; border-radius: var(--r-full); background: var(--fill-field); }
  .seg label { position: relative; }
  .seg input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .seg span { display: grid; align-content: center; justify-items: center; height: 26px; padding: 0 14px; border-radius: var(--r-full); color: var(--text-2); white-space: nowrap; }
  /* Room for the bold label, so choosing never widens the control. */
  .seg span::after { content: attr(data-l); height: 0; overflow: hidden; visibility: hidden; font-weight: var(--fw-medium); }
  .seg input:checked + span { background: var(--fill-control-on); color: var(--text); font-weight: var(--fw-medium); box-shadow: 0 0 0 .5px var(--line-strong), 0 1px 2px rgba(0, 0, 0, .1); }
  .seg input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 1px; }
  /* Swatches: 22px dots; the chosen one gets a ring in its own colour. */
  .swatches { display: flex; flex-wrap: wrap; gap: 10px; margin: 0; padding: 0; border: 0; }
  .sw { position: relative; display: block; }
  .sw input { position: absolute; inset: 0; margin: 0; opacity: 0; }
  .sw > span[aria-hidden] { display: block; width: 22px; height: 22px; border-radius: 50%; background: var(--c); box-shadow: inset 0 0 0 .5px rgba(0, 0, 0, .25);
    transition: box-shadow var(--dur) var(--ease-out); }
  .sw input:checked + span { box-shadow: 0 0 0 2px var(--bg-inset), 0 0 0 4px var(--c); }
  .sw input:focus-visible + span { outline: 2px solid var(--focus-ring); outline-offset: 5px; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
