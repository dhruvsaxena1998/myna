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
  .group + .group { margin-top: var(--sp-5); }
  h2 { margin: 0 0 var(--sp-3); font-size: var(--fs-title3); line-height: var(--lh-title3); font-weight: var(--fw-semibold); }
  .list { list-style: none; margin: 0; padding: 0; border-radius: var(--r-xl); background: var(--bg-inset); box-shadow: inset 0 0 0 .5px var(--line); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  /* Switch, 38×22 (DESIGN.md). */
  .switch { appearance: none; width: 38px; height: 22px; margin: 0; border-radius: 11px; position: relative; flex: none; background: var(--fill-track);
    transition: background var(--dur) var(--ease-out); }
  .switch::after { content: ""; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: #fff;
    box-shadow: 0 1px 2px rgba(0, 0, 0, .3); transition: transform var(--dur) var(--ease-out); }
  .switch:checked { background: var(--accent); }
  .switch:checked::after { transform: translateX(16px); }
  .switch:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Reserved line so an error never shifts the page. */
  .err { margin: var(--sp-1) 0 0; min-height: var(--lh-footnote); color: var(--danger-text); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
</style>
