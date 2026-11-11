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
  .name { display: block; font-weight: var(--fw-medium); overflow-wrap: anywhere; }
  .desc { display: block; margin-top: 2px; font-size: var(--fs-footnote); line-height: var(--lh-footnote); color: var(--text-2); }
  .btns { display: flex; gap: var(--sp-2); }
</style>
