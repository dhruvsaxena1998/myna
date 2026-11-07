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

<section class="group" aria-labelledby="exp-h">
  <h2 id="exp-h">Experimental features</h2>
  <ul class="list">
    <li class="row">
      <span><label class="name" for="exp-on">Use experimental features</label><span class="desc" id="exp-d">New things we’re still working on. They may change. Turning this off turns them all off.</span></span>
      <input type="checkbox" role="switch" class="switch" id="exp-on" aria-describedby="exp-d" checked={look.experimental} onchange={(e) => save({ experimental: e.currentTarget.checked })} />
    </li>
  </ul>
</section>
<!-- Liquid Glass needs macOS 26; below it the backend sends liquid: null and the group isn't there. -->
{#if look.experimental && look.liquid !== null}
  <section class="group" aria-labelledby="exp-look-h">
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
          <svg class="end" viewBox="0 0 18 18" aria-hidden="true"><rect x="1" y="1" width="10" height="10" rx="3" fill="none" stroke="currentColor" stroke-width="1.5" /><rect x="7" y="7" width="10" height="10" rx="3" fill="none" stroke="currentColor" stroke-width="1.5" /></svg>
          <span class="rail">
            <input type="range" id="frost" min="0" max="100" step="1" disabled={look.liquid !== true} value={look.frost} style:--v="{look.frost}%" aria-describedby="frost-d"
              aria-valuetext={look.frost === 0 ? "Clear" : look.frost === 100 ? "Tinted" : `${look.frost}%`}
              oninput={(e) => { look = { ...look, frost: e.currentTarget.valueAsNumber }; applyLook(look); }} onchange={(e) => save({ frost: e.currentTarget.valueAsNumber })} />
            <span class="ticks" aria-hidden="true"><i></i><i></i><i></i></span>
          </span>
          <svg class="end" viewBox="0 0 18 18" aria-hidden="true"><rect x="1" y="1" width="10" height="10" rx="3" fill="currentColor" /><rect x="7" y="7" width="10" height="10" rx="3" fill="currentColor" stroke="var(--bg-inset)" stroke-width="1" /></svg>
        </div>
      </li>
    </ul>
  </section>
{/if}
{#if look.experimental}
  <section class="group" aria-labelledby="exp-dict-h">
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

<style>
  /* Opacity slider: 4px track, accent left of a 20px white knob; --v is the fill's width. */
  .slider { display: flex; align-items: center; gap: var(--sp-2); width: 260px; color: var(--text-2); }
  .slider:has(:disabled) { opacity: .45; }
  .end { width: 18px; height: 18px; flex: none; }
  .rail { flex: 1; min-width: 0; }
  input[type="range"] { appearance: none; display: block; width: 100%; height: 20px; margin: 0; background: none; }
  input[type="range"]::-webkit-slider-runnable-track { height: 4px; border-radius: 2px; background: linear-gradient(to right, var(--accent) var(--v), var(--fill-track) var(--v)); }
  input[type="range"]::-webkit-slider-thumb { appearance: none; width: 20px; height: 20px; margin-top: -8px; border-radius: 50%; background: #fff; box-shadow: var(--shadow-control); }
  input[type="range"]:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; border-radius: 10px; }
  .ticks { display: flex; justify-content: space-between; padding: 0 8px; margin-top: 2px; }
  .ticks i { width: 3px; height: 3px; border-radius: 50%; background: var(--text-3); }
</style>
