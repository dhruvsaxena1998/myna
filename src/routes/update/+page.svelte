<!-- The update pop-up (prototype: updPopup in docs/index.html): what's new, and the one next step for the update state. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import MarkdownIt from "markdown-it";
  import { onMount, tick } from "svelte";
  import { upd, watchUpdates, type UpdateState } from "$lib/updates.svelte";

  let { data } = $props();
  onMount(() => watchUpdates(data.status));
  let channel = $derived(data.channel); // assignable: the "update-channel" event updates it
  let busy = $state(false); // dictating: Restart now waits
  let restarting = $state(false);
  let refusal = $state("");

  // Notes come from the feed: html: false escapes any raw HTML, and markdown-it drops javascript: and data: links.
  const md = new MarkdownIt({ html: false });
  const s = $derived(upd.s);
  const next = $derived("version" in s ? s : null);
  const err = $derived(s.state === "error" ? s : null);
  const title = $derived(
    s.state === "uptodate" ? "Myna is up to date"
    : err ? { signature: "Update not installed", check: "Couldn’t check for updates", download: "Couldn’t download the update", install: "Couldn’t install the update" }[err.kind]
    : next ? `Myna ${next.version} is available`
    : "Checking for updates…",
  );

  let kb = $state(false);
  const focusMain = () => (document.querySelector<HTMLElement>("[data-main]:not(:disabled)") ?? document.querySelector<HTMLElement>("footer button"))?.focus();
  // A new state can take the focused button away (Cancel, once the download stops): focus the main one again.
  $effect(() => {
    void s.state;
    tick().then(() => document.activeElement === document.body && focusMain());
  });
  $effect(() => {
    const un = [
      listen("update-open", async () => {
        refusal = "";
        kb = false;
        await tick();
        focusMain();
      }),
      listen<string>("update-channel", ({ payload }) => (channel = payload as typeof channel)),
      listen<{ state: string }>("dictation", ({ payload }) => (busy = payload.state === "listening" || payload.state === "writing")),
    ];
    return () => un.forEach((u) => u.then((f) => f()));
  });

  const close = () => getCurrentWindow().hide();
  // Check reports through the update state, and the backend opens this window again with the result.
  const retry = () => close().then(() => invoke("update_check")).catch(() => {});
  async function download() {
    await invoke("update_download").catch(() => {}); // a failure shows as the error state
    const now = await invoke<UpdateState>("update_state");
    if (now.state === "downloaded" && !busy) restart();
  }
  async function restart() {
    restarting = true;
    refusal = "";
    try {
      await invoke("update_restart");
    } catch (e) {
      restarting = false;
      if (upd.s.state === "downloaded") refusal = String(e); // a failed install shows as the error state
    }
  }
  function openLink(e: MouseEvent) {
    const a = (e.target as Element).closest("a");
    if (!a) return;
    e.preventDefault(); // never navigate this window
    invoke("open_link", { url: a.href }).catch(() => {});
  }
</script>

<!-- kb: a key was pressed since the pop-up opened. Until then the default button shows no ring, like a native one; Return still presses it. -->
<svelte:window onkeydown={(e) => { kb = true; if (e.key === "Escape") close(); }} />

<div class="window" class:quiet={!kb} role="dialog" aria-labelledby="upd-h">
  <div class="titlebar" data-tauri-drag-region>Software update</div>
  <div class="panel">
    <div class="body">
      <h1 id="upd-h">{title}</h1>
      {#if s.state === "uptodate"}<p class="lead">You’re on the latest version ({data.version})</p>
      {:else if err}<p class="lead alert">{err.message}</p>
      {:else}<p class="lead">You have {data.version}{channel ? ` · ${channel === "beta" ? "Beta" : "Stable"}` : ""}</p>{/if}
      {#if next}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="notes" tabindex="0" aria-label="What’s new" onclick={openLink}>
          <h2 class="label">What’s new</h2>
          {@html md.render(next.notes)}
        </div>
      {/if}
    </div>
    <footer>
      {#if restarting}<span class="help" role="status">Restarting…</span>
      {:else if s.state === "downloading"}
        <div class="bar" role="progressbar" aria-label="Downloading update" aria-valuemin="0" aria-valuemax="100" aria-valuenow={s.pct}><b style:transform="scaleX({s.pct / 100})"></b></div>
        <span class="help pct">{s.pct}%</span>
        <button class="btn" data-main onclick={() => invoke("update_cancel")}>Cancel</button>
      {:else}
        <span class="fill">{#if s.state === "downloaded" && (busy || refusal)}<span class="help" id="upd-busy">{busy ? "Finish recording first" : refusal}</span>{/if}</span>
        {#if s.state === "uptodate"}<button class="btn primary" data-main onclick={close}>OK</button>
        {:else if err}
          <button class="btn" onclick={close}>Close</button>
          {#if err.kind !== "signature"}<button class="btn primary" data-main onclick={retry}>Try again</button>{/if}
        {:else if s.state === "downloaded"}
          <button class="btn" onclick={close}>Later</button>
          <button class="btn primary" data-main disabled={busy} title={busy ? "Finish recording first" : undefined} aria-describedby={busy ? "upd-busy" : undefined} onclick={restart}>Restart now</button>
        {:else if s.state === "available"}
          <button class="btn" onclick={close}>Later</button>
          <button class="btn primary" data-main onclick={download}>Download and restart</button>
        {:else}<button class="btn" onclick={close}>Close</button>{/if}
      {/if}
    </footer>
  </div>
</div>

<style>
  .window { display: grid; grid-template-rows: var(--h-titlebar) minmax(0, 1fr); height: 100vh; box-sizing: border-box; padding: 0 var(--win-inset) var(--win-inset); }
  /* Left of the title: the traffic lights. */
  .titlebar { display: flex; align-items: center; padding-left: 76px; font-weight: var(--fw-semibold); color: var(--text-2); }
  .panel { display: grid; grid-template-rows: minmax(0, 1fr) 56px; min-height: 0; border-radius: var(--r-xl); background: var(--bg-content); box-shadow: var(--shadow-panel); overflow: hidden; }
  .body { display: flex; flex-direction: column; min-height: 0; padding: var(--sp-4) var(--sp-5) 0; }
  h1 { margin: 0; font-size: var(--fs-title2); line-height: var(--lh-title2); font-weight: var(--fw-semibold); }
  .lead { margin: var(--sp-1) 0 0; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: text; }
  .lead.alert { white-space: normal; }
  .notes { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; margin-top: var(--sp-3); padding-bottom: var(--sp-4); border-top: 1px solid var(--line); user-select: text;
    scrollbar-width: thin; scrollbar-color: var(--line-strong) transparent; }
  .notes:focus-visible { outline: 2px solid var(--focus-ring); outline-offset: 2px; }
  /* Beats the release notes' heading rule below, which would make it 13px. */
  .notes h2.label { margin: var(--sp-3) 0 0; font-size: var(--fs-caption); line-height: var(--lh-caption); font-weight: var(--fw-semibold); color: var(--text-3); }
  .notes :global(:is(h1, h2, h3, h4, h5, h6)) { margin: var(--sp-3) 0 var(--sp-1); font-size: var(--fs-body); line-height: var(--lh-body); font-weight: var(--fw-semibold); }
  .notes :global(p) { margin: var(--sp-2) 0 0; }
  .notes :global(ul), .notes :global(ol) { margin: 0; padding-left: 20px; }
  .notes :global(a) { color: var(--accent-text); }
  .notes :global(code) { font: 12px/var(--lh-body) var(--font-mono); }
  .notes :global(img) { max-width: 100%; }
  footer { display: flex; align-items: center; gap: var(--sp-2); padding: 0 var(--sp-5); border-top: 1px solid var(--line); }
  .fill { flex: 1 1 0; min-width: 0; }
  .help { color: var(--text-3); font-size: var(--fs-footnote); white-space: nowrap; }
  .pct { min-width: 4ch; text-align: right; font-variant-numeric: tabular-nums; }
  .bar { flex: 1; height: 4px; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .bar b { display: block; height: 100%; background: var(--accent); transform-origin: left; }
  .quiet .btn:focus-visible { outline: none; }
</style>
