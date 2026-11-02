<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  // Only rows that work today. Dictation and Meetings add theirs when they are built.
  const items = [
    { label: "Settings…", key: "⌘,", run: () => invoke("open_settings") },
    { label: "Quit Myna", key: "⌘Q", run: () => invoke("quit") },
  ];
  let active = $state(-1);
  let menu: HTMLElement;
  const win = getCurrentWindow();

  onMount(() => {
    // Size the panel to its rows, the way a native menu is.
    win.setSize(new LogicalSize(menu.offsetWidth, menu.offsetHeight));
    // Each opening starts with nothing highlighted, like a native menu.
    const off = win.onFocusChanged(({ payload }) => { if (payload) active = -1; });
    return () => { off.then((f) => f()); };
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") win.hide();
    else if (e.key === "ArrowDown") active = (active + 1) % items.length;
    else if (e.key === "ArrowUp") active = (active - 1 + items.length) % items.length;
    else if ((e.key === "Enter" || e.key === " ") && active >= 0) items[active].run();
    else if (e.metaKey && e.key === ",") items[0].run();
    else if (e.metaKey && e.key === "q") items[1].run();
    else return;
    e.preventDefault();
  }
</script>

<svelte:window {onkeydown} />

<div class="menu" role="menu" aria-label="Myna" bind:this={menu}>
  <div class="header">Myna</div>
  <div class="sep" role="separator"></div>
  {#each items as item, i}
    <div
      class="item"
      class:active={active === i}
      role="menuitem"
      tabindex="-1"
      aria-keyshortcuts={item.key === "⌘," ? "Meta+," : "Meta+Q"}
      onmouseenter={() => (active = i)}
      onmouseleave={() => (active = -1)}
      onclick={item.run}
      onkeydown={() => {}}
    >
      <span>{item.label}</span><kbd>{item.key}</kbd>
    </div>
  {/each}
</div>

<style>
  /* The panel's material is native vibrancy (NSVisualEffectView, popover); this only lays out rows. */
  .menu { width: var(--w-menu); box-sizing: border-box; padding: 5px; font: var(--fs-body)/var(--lh-body) var(--font); color: var(--text); }
  .header { padding: var(--sp-1) 9px 0; height: 22px; font-weight: var(--fw-semibold); }
  .sep { height: 1px; margin: var(--sp-1) 9px; background: var(--line-strong); }
  .item { display: flex; align-items: center; justify-content: space-between; height: var(--h-menu-item);
    padding: 0 9px; border-radius: var(--r-sm); outline: none; }
  .item.active { background: var(--accent); color: var(--on-accent); }
  kbd { font: inherit; color: var(--text-3); }
  .item.active kbd { color: var(--on-accent); }
</style>
