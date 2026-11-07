<!-- Voice typing › Suggested fixes: mistakes Myna saw in at least two rated dictations (docs/index.html). Gone when there are none. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Fix } from "./feedback";
  import FixRow from "./FixRow.svelte";

  let { initial, onadd }: { initial: Fix[]; onadd: () => void } = $props();
  let fixes = $derived(initial); // assignable: each answer replaces it
  let error = $state("");

  async function act(cmd: "fix_add" | "fix_dismiss", key: string) {
    error = "";
    try {
      fixes = await invoke<Fix[]>(cmd, { key });
      if (cmd === "fix_add") onadd();
    } catch (err) {
      error = String(err);
    }
  }
</script>

{#if fixes.length}
  <section class="group" aria-labelledby="fix-h">
    <h2 id="fix-h">Suggested fixes</h2>
    <p class="help">Mistakes Myna noticed in the dictations you rated.</p>
    <ul class="list">
      {#each fixes as f (f.key)}<FixRow fix={f} onadd={() => act("fix_add", f.key)} onskip={() => act("fix_dismiss", f.key)} />{/each}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{/if}
