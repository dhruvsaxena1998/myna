<!-- Settings › Meeting notes › Browser add-on: its status, the steps to load it, and the code to type into it (prototype: `sgroup('Browser add-on', …)`). -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let connected = $state(false);
  let code = $state("");
  let steps = $state(false);
  let copied = $state(false);
  let error = $state("");
  let timer: ReturnType<typeof setTimeout>;

  onMount(() => {
    invoke<{ connected: boolean; code: string }>("addon_get").then((a) => ({ connected, code } = a), (e) => (error = String(e)));
    const off = listen<boolean>("addon-status", (e) => {
      connected = e.payload;
      if (connected) steps = false;
    });
    return () => void off.then((f) => f());
  });

  async function run(f: () => Promise<void>) {
    error = "";
    try {
      await f();
    } catch (e) {
      error = String(e);
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
</script>

<section class="group" aria-labelledby="addon-h">
  <h2 id="addon-h">Browser add-on</h2>
  <ul class="list">
    <li class="row">
      <span><span class="name">Status</span><span class="desc">{connected ? "Connected. Myna can see when you’re muted in Teams, and who is talking." : "Not connected. Install the Myna add-on in your browser, then connect it here."}</span></span>
      {#if connected}
        <button class="btn" onclick={disconnect}>Disconnect</button>
      {:else}
        <button class="btn" aria-expanded={steps} onclick={() => (steps = !steps)}>Connect</button>
      {/if}
    </li>
    {#if steps && !connected}
      <li class="row">
        <ol class="steps">
          <li>Show the add-on’s folder.</li>
          <li>In Chrome, open <span class="mono">chrome://extensions</span>, turn on Developer mode and choose Load unpacked.</li>
          <li>Pick that folder, click the Myna icon and paste the code below.</li>
        </ol>
        <button class="btn" onclick={folder}>Show folder</button>
      </li>
    {/if}
    <li class="row">
      <span><span class="name">Connection code</span><span class="desc">Type this into the add-on so it can talk to Myna.</span></span>
      <span class="code"><span class="mono">{code}</span><button class="btn" onclick={copy}>{copied ? "Copied" : "Copy"}</button></span>
    </li>
  </ul>
  <p class="err" role="status">{error}</p>
</section>

<style>
  .steps { margin: 0; padding-left: 1.2em; font: var(--text-sm, 13px/1.4 inherit); }
  .mono { font: 12px/var(--lh-body) var(--font-mono); overflow-wrap: anywhere; }
  .code { display: flex; align-items: center; gap: 10px; }
</style>
