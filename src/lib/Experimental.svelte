<!-- Experimental features: the master switch, then each feature (Liquid Glass on macOS 26 or later, Stats for nerds). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { applyLook, type Look } from "./look";

  let { look = $bindable() }: { look: Look } = $props();
  let error = $state("");

  async function save(change: Partial<Look>) {
    const was = look;
    look = { ...look, ...change };
    applyLook(look);
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

<section aria-label="Experimental features">
  <ul class="list">
    <li class="row">
      <span><label class="name" for="exp-on">Use experimental features</label><span class="desc" id="exp-d">New things we’re still working on. They may change. Turning this off turns them all off.</span></span>
      <input type="checkbox" role="switch" class="switch" id="exp-on" aria-describedby="exp-d" checked={look.experimental} onchange={(e) => save({ experimental: e.currentTarget.checked })} />
    </li>
  </ul>
</section>
<!-- Liquid Glass needs macOS 26; below it the backend sends liquid: null and the group isn't there. -->
{#if look.experimental && look.liquid !== null}
  <section aria-labelledby="exp-look-h">
    <h2 id="exp-look-h">Look</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="liquid">Liquid Glass</label><span class="desc" id="liquid-d">Uses macOS’s own glass for the pill and the Settings window. Still being tested.</span></span>
        <input type="checkbox" role="switch" class="switch" id="liquid" aria-describedby="liquid-d" checked={look.liquid === true} onchange={(e) => save({ liquid: e.currentTarget.checked })} />
      </li>
      <!-- Always there, disabled while Liquid Glass is off, so the switch never shifts the page. -->
      <li class="row">
        <span><label class="name" for="frost">Opacity</label><span class="desc" id="frost-d">Clear to tinted. Tinted makes words easier to read on a busy desktop.</span></span>
        <div class="slider">
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="11" height="11" rx="3" /><rect x="10" y="10" width="11" height="11" rx="3" /></svg>
          <input type="range" id="frost" min="0" max="100" step="1" disabled={look.liquid !== true} value={look.frost} style:--v="{look.frost}%" aria-describedby="frost-d"
              aria-valuetext={look.frost === 0 ? "Clear" : look.frost === 100 ? "Tinted" : `${look.frost}%`}
              oninput={(e) => { look = { ...look, frost: e.currentTarget.valueAsNumber }; applyLook(look); }} onchange={(e) => save({ frost: e.currentTarget.valueAsNumber })} />
          <svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="3" width="11" height="11" rx="3" fill="currentColor" /><rect x="10" y="10" width="11" height="11" rx="3" fill="currentColor" opacity=".6" /></svg>
        </div>
      </li>
    </ul>
  </section>
{/if}
{#if look.experimental}
  <section aria-labelledby="exp-dict-h">
    <h2 id="exp-dict-h">Voice typing</h2>
    <ul class="list">
      <li class="row">
        <span><label class="name" for="nerds">Stats for nerds</label><span class="desc" id="nerds-d">Each dictation, opened up: where the time went and what Myna changed.</span></span>
        <input type="checkbox" role="switch" class="switch" id="nerds" aria-describedby="nerds-d" checked={look.nerds} onchange={(e) => save({ nerds: e.currentTarget.checked })} />
      </li>
    </ul>
  </section>
{/if}
<p class="err" role="status">{error}</p>
