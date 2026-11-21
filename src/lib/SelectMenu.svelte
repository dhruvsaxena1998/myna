<script lang="ts">
  // Every <select class="select"> opens this menu instead of the native one (DESIGN.md › Menu / popover).
  // The <select> stays the source of truth: a pick sets its value and fires input + change, so bind:value,
  // onchange and Playwright's selectOption all keep working unchanged.
  import { fuzzy } from "$lib/fuzzy";

  type Item = { kind: "head"; label: string } | { kind: "opt"; i: number; label: string; group: string; disabled: boolean };
  const SEARCH_FROM = 12; // a list this long gets a search box (model lists run to hundreds)

  let menu = $state<HTMLDivElement>();
  let sel = $state<HTMLSelectElement | null>(null);
  let items = $state<Item[]>([]);
  let active = $state(-1); // index into sel.options
  let pos = $state({ left: 0, top: 0, minWidth: 0, maxHeight: 0 });
  let typed = "", typedAt = 0;
  let q = $state("");
  let search = $state<HTMLInputElement>();

  const all = $derived(items.filter((x): x is Extract<Item, { kind: "opt" }> => x.kind === "opt"));
  const searchable = $derived(all.length >= SEARCH_FROM);
  // A group's heading shows only while one of its options does.
  const hit = (x: Item) => x.kind === "opt" && fuzzy(q, `${x.label} ${x.group}`);
  const shown = $derived(q.trim() ? items.filter((x) => (x.kind === "opt" ? hit(x) : items.some((y) => y.kind === "opt" && y.group === x.label && hit(y)))) : items);
  const opts = () => shown.filter((x): x is Extract<Item, { kind: "opt" }> => x.kind === "opt");

  let home: ParentNode | null = null; // where the menu lives when no dialog holds it

  function open(s: HTMLSelectElement, byKey: boolean) {
    home ??= menu!.parentNode;
    sel = s;
    q = "";
    items = [];
    for (const child of s.children) {
      if (child instanceof HTMLOptGroupElement) {
        items.push({ kind: "head", label: child.label });
        for (const o of child.querySelectorAll("option")) items.push({ kind: "opt", i: o.index, label: o.text, group: child.label, disabled: o.disabled || child.disabled });
      } else if (child instanceof HTMLOptionElement) items.push({ kind: "opt", i: child.index, label: child.text, group: "", disabled: child.disabled });
    }
    active = byKey ? s.selectedIndex : -1; // like a native menu: nothing lit until the pointer or a key picks a row
    const r = s.getBoundingClientRect();
    const below = innerHeight - r.bottom - 8, above = r.top - 8;
    const up = below < 160 && above > below;
    pos = { left: Math.min(r.left, innerWidth - 8 - Math.max(r.width, 160)), top: up ? 0 : r.bottom + 4, minWidth: r.width, maxHeight: Math.min(up ? above - 4 : below - 4, 360) };
    // A modal dialog makes everything outside it inert, a popover too: the menu has to live inside the dialog.
    (s.closest("dialog[open]") ?? home)!.appendChild(menu!);
    menu!.showPopover();
    if (up) pos.top = r.top - 4 - menu!.offsetHeight;
    menu!.focus();
    if (searchable) queueMicrotask(() => search?.focus()); // the box renders on the next tick
    menu!.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function close(refocus = true) {
    if (menu?.matches(":popover-open")) menu.hidePopover();
    if (refocus) sel?.focus();
  }

  function pick(i: number) {
    const s = sel!;
    close();
    if (s.selectedIndex === i) return;
    s.selectedIndex = i;
    s.dispatchEvent(new Event("input", { bubbles: true }));
    s.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function move(step: number) {
    const list = opts().filter((o) => !o.disabled);
    if (!list.length) return;
    const at = list.findIndex((o) => o.i === (active < 0 ? sel!.selectedIndex : active));
    active = list[at < 0 ? 0 : Math.max(0, Math.min(list.length - 1, at + step))].i;
    menu!.querySelector<HTMLElement>(`[data-i="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function keys(e: KeyboardEvent) {
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.target === search && !["Enter", "Escape", "Tab"].includes(e.key)) return; // typing in the search box
    else if (e.key === "Home") move(-Infinity);
    else if (e.key === "End") move(Infinity);
    else if (e.key === "Enter" || e.key === " ") { if (active >= 0 && !sel!.options[active]?.disabled) pick(active); }
    else if (e.key === "Escape") close();
    else if (e.key === "Tab") return close(false);
    else if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) {
      // Type-ahead, like a native menu.
      typed = (e.timeStamp - typedAt > 700 ? "" : typed) + e.key.toLowerCase();
      typedAt = e.timeStamp;
      const hit = opts().find((o) => !o.disabled && o.label.toLowerCase().startsWith(typed));
      if (hit) (active = hit.i), menu!.querySelector<HTMLElement>(`[data-i="${hit.i}"]`)?.scrollIntoView({ block: "nearest" });
    } else return;
    e.preventDefault();
  }

  const target = (e: Event) => (e.target instanceof HTMLSelectElement && e.target.matches(".select:not(:disabled)") ? e.target : null);

  $effect(() => {
    // manual, not auto: auto's light dismiss would shut the menu on the mouseup of the click that opened it.
    const down = (e: MouseEvent) => {
      if (menu?.matches(":popover-open") && !menu.contains(e.target as Node)) close(false);
      const s = target(e);
      if (!s || e.button !== 0) return;
      e.preventDefault(); // stops WebKit opening its own menu
      s.focus();
      open(s, false);
    };
    const key = (e: KeyboardEvent) => {
      const s = target(e);
      if (s && [" ", "Enter", "ArrowDown", "ArrowUp"].includes(e.key) && !e.metaKey) (e.preventDefault(), open(s, true));
    };
    const shut = () => close(false);
    // The menu is pinned to where the select was; a page scroll would leave it floating.
    const scroll = (e: Event) => { if (!menu?.contains(e.target as Node)) close(false); };
    document.addEventListener("scroll", scroll, true);
    document.addEventListener("mousedown", down, true);
    document.addEventListener("keydown", key, true);
    addEventListener("resize", shut);
    addEventListener("blur", shut);
    return () => {
      document.removeEventListener("scroll", scroll, true);
      document.removeEventListener("mousedown", down, true);
      document.removeEventListener("keydown", key, true);
      removeEventListener("resize", shut);
      removeEventListener("blur", shut);
    };
  });
</script>

<div bind:this={menu} popover="manual" class="menu" role="listbox" tabindex="-1" aria-activedescendant={active >= 0 ? `sm-${active}` : undefined} aria-label={sel?.getAttribute("aria-label") ?? sel?.labels?.[0]?.textContent ?? undefined}
  style:left="{pos.left}px" style:top="{pos.top}px" style:min-width="{pos.minWidth}px" style:max-height="{pos.maxHeight}px"
  onkeydown={keys} onpointerleave={() => (active = -1)} ontoggle={(e) => { if (e.newState === "closed") sel = null; }}>
  {#if searchable}
    <input bind:this={search} class="search" type="search" aria-label="Search" placeholder="Search" autocomplete="off" spellcheck="false" bind:value={q}
      oninput={() => { active = opts().find((o) => !o.disabled)?.i ?? -1; }} />
  {/if}
  {#each shown as it}
    {#if it.kind === "head"}
      <div class="head" role="presentation">{it.label}</div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events (keys are handled on the listbox) -->
      <div class="opt" role="option" tabindex="-1" id="sm-{it.i}" data-i={it.i} aria-selected={it.i === sel?.selectedIndex} aria-disabled={it.disabled} class:active={it.i === active}
        onpointermove={() => { if (!it.disabled) active = it.i; }} onclick={() => { if (!it.disabled) pick(it.i); }}>
        <svg class="check" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
        <span>{it.label}</span>
      </div>
    {/if}
  {/each}
</div>

<style>
  .menu { position: fixed; inset: auto; margin: 0; box-sizing: border-box; max-width: calc(100vw - 16px); overflow-y: auto; padding: 5px; border: 0; border-radius: var(--r-lg);
    background: var(--mat-menu); -webkit-backdrop-filter: var(--blur); backdrop-filter: var(--blur); box-shadow: inset 0 0 0 .5px var(--edge), var(--shadow-menu);
    color: var(--text); font: inherit; scrollbar-width: thin; outline: none; }
  /* DESIGN.md › Motion: popovers enter with a short rise and a fade. */
  .menu:popover-open { animation: rise var(--dur) var(--ease-out); }
  @keyframes rise { from { opacity: 0; transform: translateY(4px); } }
  .search { display: block; box-sizing: border-box; width: 100%; margin: 0 0 4px; padding: 4px 8px; border: 0; border-radius: var(--r-sm); background: var(--bg-inset); color: var(--text); font: inherit; outline: none; box-shadow: inset 0 0 0 .5px var(--line); }
  .search:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  /* A group's heading (a provider in a model menu): a label a step above its options, a hairline above every group after the first. */
  .head { padding: 8px 10px 4px 28px; font-size: var(--fs-footnote); line-height: var(--lh-footnote); font-weight: var(--fw-semibold); color: var(--text-2); }
  .opt + .head { margin-top: 5px; padding-top: 9px; box-shadow: inset 0 .5px 0 var(--line-strong); }
  .opt { display: flex; align-items: center; gap: 4px; min-height: 26px; padding: 0 12px 0 6px; border-radius: var(--r-sm); white-space: nowrap; }
  .opt span { overflow: hidden; text-overflow: ellipsis; }
  .opt.active:not([aria-disabled="true"]) { background: var(--accent); color: var(--on-accent); }
  .opt[aria-disabled="true"] { color: var(--text-3); }
  .check { width: 14px; height: 14px; flex: none; margin-right: 4px; fill: none; stroke: currentColor; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; visibility: hidden; }
  .opt[aria-selected="true"] .check { visibility: visible; }
</style>
