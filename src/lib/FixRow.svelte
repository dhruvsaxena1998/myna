<!-- One suggested fix with its Add and Not now buttons. Shared by Settings › Voice typing and the 👎 panel. -->
<script lang="ts">
  import { fixText, type Fix } from "./feedback";

  let { fix, onadd, onskip }: { fix: Fix; onadd: () => void; onskip: () => void } = $props();
</script>

<li>
  <span><span class="name">{fixText(fix)}</span><span class="desc">Came up in {fix.count} dictations</span></span>
  <span class="btns">
    <button type="button" class="btn" onclick={onadd}>Add</button>
    <button type="button" class="btn" onclick={onskip}>Not now</button>
  </span>
</li>

<style>
  li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--sp-3); align-items: center; padding: var(--sp-3) var(--sp-4); }
  .name { display: block; font-weight: var(--fw-semibold); overflow-wrap: anywhere; }
  .desc { display: block; margin-top: 2px; color: var(--text-2); }
  .btns { display: flex; gap: var(--sp-2); }
  .btn { display: inline-flex; align-items: center; height: var(--h-ctl); padding: 0 14px; border: 0; border-radius: var(--r-full); background: var(--fill-control); color: var(--text); font: inherit;
    box-shadow: inset 0 0 0 .5px var(--line-strong), var(--shadow-control); }
  .btn:hover { background: color-mix(in srgb, var(--fill-control) 85%, var(--text)); }
  .btn:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
</style>
