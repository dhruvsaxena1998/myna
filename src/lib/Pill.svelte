<!-- The dictation pill, in one of four styles. Draws only; the pill window and the Settings preview feed it. -->
<script lang="ts" module>
  export type Style = "capsule" | "orb" | "wave" | "card";
  export type Place = "bottom" | "top" | "bottom-right" | "top-right" | "caret";
  export type Look = { style: Style; place: Place };
  export type Shown =
    | { state: "listening"; hold: boolean; key: string; limit?: number }
    | { state: "writing" | "added" }
    | { state: "error"; title: string; hint: string };
  export const STYLES: [Style, string][] = [["capsule", "Capsule"], ["orb", "Orb"], ["wave", "Waveform"], ["card", "Card"]];
  export const PLACES: [Place, string][] = [["bottom", "Bottom centre"], ["top", "Top centre"], ["bottom-right", "Bottom right"], ["top-right", "Top right"], ["caret", "Under the text cursor"]];
  /** Bars in the waveform; the pill window keeps this many recent levels. */
  export const WAVE = 48;
</script>

<script lang="ts">
  import { keycaps } from "./shortcut";
  // `wave` is the recent mic levels, 0 to 1, oldest first; its last one drives the bars and the ring.
  // `left`: seconds until tap mode stops by itself, during the last 30; otherwise null.
  let { style, pill, wave, secs, left = null }: { style: Style; pill: Shown; wave: number[]; secs: number; left?: number | null } = $props();
  const level = $derived(wave.at(-1) ?? 0);
  const WEIGHTS = [0.5, 0.8, 1, 0.75, 0.45]; // the middle bars move most
  // Orb says nothing while things go well, so screen readers still get words.
  const quiet = $derived(style === "orb" && pill.state !== "error");
  const fmt = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const clock = $derived(fmt(secs));
  const keys = $derived(pill.state === "listening" ? keycaps(pill.key) : []);
  // What ends it: letting go, or pressing again; near the limit, how long until it adds your words anyway.
  const hint = $derived(pill.state !== "listening" ? "" : left !== null ? `Adding in ${left} s` : pill.hold ? "Release to paste" : "Press again to paste");
</script>

{#snippet icon(kind: "check" | "warn" | "mic")}
  <svg class="ic" viewBox="0 0 24 24" aria-hidden="true">
    {#if kind === "check"}<path d="M5 12.5l4.5 4.5L19 7.5" />
    {:else if kind === "warn"}<path d="M12 4 2.5 20h19zM12 10v4.5M12 17.2v.1" />
    {:else}<rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" />{/if}
  </svg>
{/snippet}
{#snippet bars()}
  <span class="lvl" aria-hidden="true">{#each WEIGHTS as w}<i style:transform="scaleY({0.2 + 0.8 * level * w})"></i>{/each}</span>
{/snippet}

<div class="hud {style}" class:quiet>
  {#if style === "card"}
    <span class="ring" class:live={pill.state === "listening"} style:--s={0.85 + 0.3 * level} style:--o={0.35 + 0.65 * level}>
      {#if pill.state === "listening"}{@render icon("mic")}{:else if pill.state === "writing"}<span class="spin"></span>{:else if pill.state === "added"}{@render icon("check")}{:else}{@render icon("warn")}{/if}
    </span>
    <span class="lines">
      {#if pill.state === "listening"}<span class="t">Listening</span>
        <span class="sub">{#if left !== null}{hint}{:else}{pill.hold ? "Release" : "Press"} {#each keys as k}<kbd>{k}</kbd>{/each} {pill.hold ? "to paste" : "again to paste"}{/if}</span>
      {:else if pill.state === "writing"}<span class="t">Writing…</span><span class="sub">On this Mac</span>
      {:else if pill.state === "added"}<span class="t">Added</span><span class="sub">Where you were typing</span>
      {:else if pill.state === "error"}<span class="t">{pill.title}</span><span class="sub">{pill.hint}</span>{/if}
    </span>
  {:else if pill.state === "error"}
    {@render icon("warn")}<span>{pill.title}</span><span class="sub">{pill.hint}</span>
  {:else if pill.state === "listening"}
    {#if style === "wave"}
      <span class="mic">{@render icon("mic")}</span>
      <span class="trace" aria-hidden="true">{#each wave as l}<i style:height="{3 + 21 * l}px"></i>{/each}</span>
      <span class="sr">Listening</span>{#if left !== null}<time class="end">{fmt(left)} left</time>{:else}<time>{clock}</time>{/if}
    {:else}
      {@render bars()}
      {#if quiet}<span class="sr">Listening</span>{#if left !== null}<span class="end" aria-label={hint}>{left}</span>{/if}
      {:else}<span>Listening</span><span class="sub" class:end={left !== null}>{hint}</span>{/if}
    {/if}
  {:else if pill.state === "writing"}
    {#if style !== "capsule"}<span class="spin" aria-hidden="true"></span>{/if}<span class:sr={quiet}>Writing…</span>
  {:else}
    {@render icon("check")}<span class:sr={quiet}>Added where you were typing</span>
  {/if}
</div>

<style>
  /* Dark in both themes, like system HUDs. */
  .hud { display: inline-flex; align-items: center; gap: 10px; box-sizing: border-box; background: var(--hud-bg); color: var(--hud-fg); white-space: nowrap;
    -webkit-backdrop-filter: blur(24px) saturate(180%); backdrop-filter: blur(24px) saturate(180%); box-shadow: inset 0 0 0 0.5px var(--hud-edge), var(--shadow-hud); }
  .sub { color: var(--hud-2); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  .ic { width: var(--ic); height: var(--ic); flex: none; stroke: currentColor; fill: none; stroke-width: var(--ic-stroke); stroke-linecap: round; stroke-linejoin: round; }
  .spin { width: 14px; height: 14px; flex: none; box-sizing: border-box; border-radius: 50%; border: 2px solid var(--hud-fill); border-top-color: var(--hud-fg); animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .lvl { display: flex; align-items: center; gap: 2px; height: 16px; }
  .lvl i { width: 3px; height: 16px; border-radius: 2px; background: currentColor; transition: transform 80ms linear; }

  /* Capsule: 40px, bars and words. */
  .capsule { height: var(--h-hud); padding: 0 var(--sp-4); border-radius: var(--r-full); }
  /* Orb: a 32px blob with no words while things go well; errors grow it into a capsule. */
  .orb { height: 32px; min-width: 56px; padding: 0 var(--sp-3); justify-content: center; border-radius: var(--r-full); }
  .orb .lvl { height: 14px; } .orb .lvl i { width: 2.5px; height: 14px; }
  /* Waveform: the recent mic level scrolling by, and how long you've talked. */
  .wave { height: 44px; min-width: 300px; padding: 0 var(--sp-4); gap: var(--sp-3); border-radius: var(--r-lg); }
  .trace { display: flex; flex: 1; align-items: center; gap: 2px; height: 24px; min-width: 0; overflow: hidden; }
  .trace i { flex: none; width: 2px; border-radius: 1px; background: currentColor; }
  .wave time { font-variant-numeric: tabular-nums; color: var(--hud-2); }
  /* The last 30 s of tap mode. */
  .hud .end { color: var(--hud-fg); font-variant-numeric: tabular-nums; font-weight: var(--fw-semibold); }
  .mic { width: 22px; height: 22px; flex: none; display: grid; place-items: center; border-radius: 50%; background: var(--rec); }
  .mic .ic { width: 12px; height: 12px; stroke: #fff; }
  /* Card: two lines, a mic ring that swells with your voice. */
  .card { min-width: 280px; min-height: 56px; padding: 10px var(--sp-4) 10px 10px; gap: var(--sp-3); border-radius: var(--r-xl); }
  .ring { position: relative; width: 36px; height: 36px; flex: none; display: grid; place-items: center; border-radius: 50%; background: var(--hud-fill); }
  .ring.live::before { content: ""; position: absolute; inset: -4px; border-radius: 50%; border: 2px solid var(--rec);
    transform: scale(var(--s)); opacity: var(--o); transition: transform 80ms linear, opacity 80ms linear; }
  .lines { display: grid; }
  .t { font-weight: var(--fw-semibold); }
  .lines .sub { font-size: var(--fs-footnote); line-height: var(--lh-footnote); }
  kbd { font: inherit; padding: 0 4px; border-radius: var(--r-xs); background: var(--hud-fill); }
</style>
