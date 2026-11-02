<!-- Shown in place of any screen whose data failed to load. No screen renders blank. -->
<script lang="ts">
  let { title, message }: { title: string; message: string } = $props();
</script>

<div class="error" role="alert">
  <h2>{title}</h2>
  <p>{message}</p>
</div>

<style>
  .error { margin: var(--sp-4) 0; padding: var(--sp-3) var(--sp-4); border-radius: var(--r-lg);
    background: var(--danger-soft); color: var(--text); }
  h2 { margin: 0 0 var(--sp-1); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); color: var(--danger-text); }
  p { margin: 0; color: var(--text-2); font-size: var(--fs-footnote); line-height: var(--lh-footnote); user-select: text; overflow-wrap: anywhere; }
</style>
