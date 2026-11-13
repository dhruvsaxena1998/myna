<!-- The Experimental page (docs/settings.html, VIEWS.experimental): Liquid Glass on macOS 26 or later, and Stats for nerds. General lists it while its switch is on. -->
<script lang="ts">
  import { applyLook, saveLook, type Look } from "./look";

  let { look = $bindable() }: { look: Look } = $props();
  // One error line per section, so a failed save shows under its own rows.
  let errors = $state({ look: "", dict: "" });

  async function save(change: Partial<Look>, what: string, slot: keyof typeof errors) {
    const was = look;
    look = { ...look, ...change };
    errors[slot] = "";
    errors[slot] = await saveLook(was, look, what);
    if (errors[slot]) look = was;
  }
</script>

<!-- Liquid Glass needs macOS 26; below it the backend sends liquid: null and the section isn't there. -->
{#if look.liquid !== null}
  <section aria-labelledby="exp-look-h">
    <h2 id="exp-look-h">Appearance</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="liquid">Liquid Glass</label><span class="desc" id="liquid-d" title="Uses macOS’s own glass for the pill and the Settings window.">Uses macOS’s own glass for the pill and the Settings window.</span></span>
        <input type="checkbox" role="switch" class="switch" id="liquid" aria-describedby="liquid-d" checked={look.liquid === true} onchange={(e) => save({ liquid: e.currentTarget.checked }, "Liquid Glass", "look")} />
      </li>
      <!-- Always there, dimmed as a whole while Liquid Glass is off, so the switch never shifts the page. -->
      <li class="row">
        <span><label class="name" for="frost">Glass opacity</label><span class="desc" id="frost-d" title="Tinted is easier to read on a busy desktop.">Tinted is easier to read on a busy desktop.</span></span>
        <div class="slider">
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="11" height="11" rx="3" /><rect x="10" y="10" width="11" height="11" rx="3" /></svg>
          <input type="range" id="frost" min="0" max="100" step="1" disabled={look.liquid !== true} value={look.frost} style:--v="{look.frost}%" aria-describedby="frost-d"
              aria-valuetext={look.frost === 0 ? "Clear" : look.frost === 100 ? "Tinted" : `${look.frost}%`}
              oninput={(e) => { look = { ...look, frost: e.currentTarget.valueAsNumber }; applyLook(look); }} onchange={(e) => save({ frost: e.currentTarget.valueAsNumber }, "Glass opacity", "look")} />
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="11" height="11" rx="3" fill="currentColor" /><rect x="10" y="10" width="11" height="11" rx="3" fill="currentColor" opacity=".6" /></svg>
        </div>
      </li>
    </ul>
    <p class="err" role="status">{errors.look}</p>
  </section>
{/if}
<section aria-labelledby="exp-dict-h">
  <h2 id="exp-dict-h">Voice typing</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="nerds">Stats for nerds</label><span class="desc" id="nerds-d" title="Adds a page to the main window with the numbers behind each dictation.">Adds a page to the main window with the numbers behind each dictation.</span></span>
      <input type="checkbox" role="switch" class="switch" id="nerds" aria-describedby="nerds-d" checked={look.nerds} onchange={(e) => save({ nerds: e.currentTarget.checked }, "Stats for nerds", "dict")} />
    </li>
  </ul>
  <p class="err" role="status">{errors.dict}</p>
</section>
