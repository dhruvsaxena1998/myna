<!-- Understand › Suggested fixes: mistakes Myna saw in at least two rated dictations (docs/settings.html). Gone when there are none. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { fixText, type Fix } from "./feedback";

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
  <section aria-labelledby="fix-h">
    <h2 id="fix-h">Suggested fixes</h2>
    <p class="help">Mistakes Myna noticed in the dictations you rated.</p>
    <ul class="list">
      {#each fixes as f (f.key)}
        {@const text = fixText(f)}
        <li class="row">
          <span><span class="name" title={text}>{text}</span><span class="desc">Came up in {f.count} dictations</span></span>
          <span class="end">
            <button type="button" class="btn" onclick={() => act("fix_add", f.key)}>Add</button>
            <button type="button" class="btn quiet" onclick={() => act("fix_dismiss", f.key)}>Not now</button>
          </span>
        </li>
      {/each}
    </ul>
    <p class="err" role="status">{error}</p>
  </section>
{/if}
