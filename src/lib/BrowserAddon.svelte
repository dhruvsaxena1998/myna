<!-- Settings › Meeting notes › Browser add-on: its status, the steps to load it, and the code to paste into it (docs/settings.html, VIEWS.meetings).
     The page loads its state before paint and listens for `addon-status`, so `connected` and `code` come in as props. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let { connected = $bindable(), code = $bindable(), loadError }: { connected: boolean; code: string; loadError: string } = $props();
  let steps = $state(false);
  let copied = $state(false);
  let failed = $state("");
  const error = $derived(failed || loadError);
  let timer: ReturnType<typeof setTimeout>;

  // Connecting (the page hears `addon-status`) closes the steps.
  $effect(() => {
    if (connected) steps = false;
  });

  async function run(f: () => Promise<void>) {
    failed = "";
    try {
      await f();
    } catch (e) {
      failed = String(e);
    }
  }
  const disconnect = () => run(async () => {
    code = await invoke<string>("addon_disconnect");
    connected = false;
  });
  const folder = () => run(() => invoke("addon_folder"));
  const copy = () =>
    run(async () => {
      await invoke("text_copy", { text: code });
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1500);
    });
  const desc = $derived(connected ? "Sees when you’re muted in Teams, and who is talking." : "Add it to Chrome to keep your voice when muted in Teams.");
</script>

<section aria-labelledby="addon-h">
  <h2 id="addon-h">Browser add-on</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name">{connected ? "Connected" : "Not connected"}</span><span class="desc" title={desc}>{desc}</span></span>
      {#if connected}
        <button class="btn" style="min-width: 100px" onclick={disconnect}>Disconnect</button>
      {:else}
        <button class="btn" style="min-width: 100px" aria-expanded={steps} onclick={() => (steps = !steps)}>Connect</button>
      {/if}
    </li>
    {#if steps && !connected}
      <li class="row">
        <ol class="steps">
          <li>Click Show folder.</li>
          <li>In Chrome, open <span class="mono">chrome://extensions</span>, turn on Developer mode, then Load unpacked.</li>
          <li>Pick that folder, click the Myna icon and paste the code below.</li>
        </ol>
        <span class="end"><button class="btn" onclick={folder}>Show folder</button></span>
      </li>
    {/if}
    <li class="row">
      <span><span class="name">Pairing code</span><span class="desc">Paste this into the add-on.</span></span>
      <span class="end"><span class="mono code">{code}</span><button class="btn" disabled={!code} style="min-width: 72px" onclick={copy}>{copied ? "Copied" : "Copy"}</button></span>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .steps { margin: 0; padding-left: var(--sp-4); display: grid; gap: var(--sp-1); color: var(--text-2); font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  /* A value to copy, not a label. */
  .code { color: var(--text-2); }
</style>
