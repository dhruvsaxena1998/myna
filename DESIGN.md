# Myna design language

Values live in `design/tokens.css`. This file holds the rules for using them. `design/specimen.html` shows every token and core component in light and dark. Run `node design/check-contrast.mjs` after changing any colour.

Source: `docs/index.html` (prototype, latest round) and `docs/flow.html`. Where the prototype stacked four rounds of overrides, the last round wins and is now the token value.

## Thesis

A quiet Mac utility with a document at its centre. Chrome is glass and hairlines. Content (summaries, transcripts, chat) sits on an opaque page in a reading column. One accent colour carries selection and the primary action. Red means the mic is live, and nothing else.

References: macOS 26 system apps (Notes, Settings, Control Center), Things, Craft, Linear. Not: gradient blobs, purple-on-dark, emoji icons, card grids, marketing adjectives.

## Structure

**Window.** Glass chrome (`--mat-window`), radius `--r-2xl`, 44px titlebar with traffic lights. Inside it sit two panels, inset `--win-inset` from the edge and from each other:
- Sidebar: `--mat-sidebar`, radius `--r-xl`, width `--w-sidebar`.
- Main: opaque `--bg-content`, radius `--r-xl`. A 56px toolbar with no bottom border, then content.

**Surfaces, back to front.** desk → window material → sidebar material → content page → inset (grouped lists, inspector, live card) → menu material (menus, popovers, toasts, live-notes panel) → HUD. In dark mode each step up is lighter; shadows alone don't separate surfaces there.

**Materials are real vibrancy in the app.** CSS `backdrop-filter` only blurs what is inside the WebView. In Tauri the window and sidebar glass come from native vibrancy (transparent window plus `NSVisualEffectView`); the `--mat-*` tokens are the tint laid over it. Menus and popovers inside the window can use CSS blur. Without blur support, the opaque `--bg-*` fallbacks apply automatically.

**Liquid Glass (experimental, macOS 26+).** Settings › Experimental features can swap the vibrancy for macOS's own glass (`NSGlassEffectView`): one sheet of Regular glass behind the whole Settings window, and Clear glass behind the pill. While it's on, and only then, the page is not opaque: `--bg-content`, `--bg-inset` and `--mat-sidebar` thin into frosted fills (`html[data-liquid]` in `tokens.css`) so the glass shows through. Contrast then depends on the desktop, so it stays experimental.

**Radii are concentric.** Inner radius = outer radius − padding between them. Window 26, panels 18 (26 − 8), popover 14 with menu items 8 at 5–6px padding. Interactive controls are capsules (`--r-full`); fields are `--r-md`; multi-line text areas `--r-lg`.

**Reading column.** Document pages are centred at `--w-doc` (720px). Reading text stops at `--measure` (68ch). The inspector (transcript, chat) is `--w-inspector` on the right of the document; below 1100px it floats over it.

## Colour

Components use semantic tokens only. No hex outside `tokens.css`.

| Job | Token |
|---|---|
| Primary action, selection fill, focus ring, switch on, progress | `--accent` (text on it: `--on-accent`) |
| Accent-coloured words and icons (links, selected nav icon) | `--accent-text` (separate token so it passes contrast on light and dark) |
| Selected nav row, active toolbar toggle | `--accent-soft-strong` |
| Suggestions (folder chip), selected template | `--accent-soft` |
| Recording dot, mic-live level bars | `--rec` |
| Destructive buttons, errors | `--danger`, `--danger-text`, `--danger-soft` |
| "Possibly muted" segments, "sends text off this Mac" | `--caution`, `--caution-text`, `--caution-soft` |
| Installed, saved, meeting detected | `--ok`, `--ok-text` |
| Tag and folder dots | `--label-*` (dot only; the label text stays `--text`) |

Rules:
- **Red is reserved for capture.** `--rec` appears only when a mic is live. Errors and destructive actions use `--danger`, a different red tuned for text and fills. Never use either for decoration.
- **Privacy states are amber, not red.** "Possibly muted" and "sends off this Mac" are things to notice, not errors. They share `--caution` so the user learns one colour for "this concerns your privacy".
- **Status is never colour alone.** Mic state is a dot plus words ("Mic live · mirrored from Meet", "Mic off · manual"). The recording dot pulses; possibly-muted groups carry a text label; badges are text pills.
- Text tiers: `--text` for content and labels, `--text-2` for secondary copy, `--text-3` for help, timestamps and section labels. All three clear 4.5:1 on every surface, checked by script.
- Accent is user-selectable (Settings → Look: blue default, plus purple, pink, red, orange, yellow, green, graphite) with optional tinted glass. Every accent defines its own fill, on-fill text and light/dark text colour.
- HUD pills (dictation, recording) are dark in both themes, like system HUDs.

## Type

One family: the system font (`--font`). WebKit picks SF Pro Text or Display by size and applies SF's tracking, so no `letter-spacing` anywhere. Check this once in the real WKWebView during story 01.

| Role | Size / line | Weight | Used for |
|---|---|---|---|
| title1 | 26/32 | 700 | Document title, onboarding heading. One per screen. |
| title2 | 18/24 | 600 | Toolbar title |
| title3 | 15/20 | 600 | Section headings, day headers |
| reading | 15/24 | 400 | Summary, transcript, chat messages |
| body | 13/18 | 400 / 500 for labels | Every control, list row, menu item |
| footnote | 12/16 | 400 | Help text, meta, timestamps |
| caption | 11/14 | 600 | Sidebar section labels, pill text |

- Numbers that change or line up (timers, durations, counts, sizes, costs) use `font-variant-numeric: tabular-nums`.
- Paths, keys and model IDs use `--font-mono` at 12px.
- Headings run in order on every screen: one h1, then h2, then h3.
- Sentence case everywhere. No all-caps labels except keyboard glyphs.

## Space and size

4px grid (`--sp-*`). 2px and 6px only for optical nudges inside controls.

- Controls: 30px default (`--h-ctl`), 24px compact (inline chips, suggestion buttons), 36px large. Nav rows 30px, menu items 26px.
- Padding: list rows 12/16, panels 16, document page 24/32 with 48 at the bottom, onboarding 32/48.
- Gaps: 8 between related controls, 12 inside a group, 24 between groups.
- Icons: 16px default, 14px inside small controls, 20px in tiles. Line icons, 1.5 stroke, round caps and joins, `currentColor`. They follow SF Symbols' shapes but are our own SVGs.

## Components (core grammar)

- **Button.** Capsule, 30px, 14px side padding. Default: `--fill-control` with a 0.5px `--line-strong` inner ring and `--shadow-control`. Primary: `--accent` fill, `--on-accent` text, weight 500. Plain: transparent until hover. Destructive: `--danger` fill. Icon-only: 30×30 with an `aria-label`. One primary per view.
- **Field.** `--fill-field` with a 0.5px `--line-strong` inner ring, radius `--r-md`, 30px tall. Invalid: 1.5px `--danger` ring plus a message underneath with an icon.
- **Switch** 38×22, **segmented control** as a capsule track with a raised `--fill-control-on` thumb, **checkbox/radio** using the native control with `accent-color`.
- **Pill** (status badge). One size, 20px, caption text. Three kinds: neutral (`--fill-hover` / `--text-2`), status (`--accent-soft` / `--accent-text`), caution (`--caution-soft` / `--caution-text`). Off-device badges are always the caution pill with words.
- **Tag.** Capsule, 22px, `--fill-field`, a 7px `--label-*` dot, removable with a 16px ×.
- **Nav row.** 30px, radius `--r-md`, icon in `--text-2`. Selected: `--accent-soft-strong` fill, icon in `--accent-text`, weight 500. Counts trail in `--text-3`, tabular.
- **Grouped list** (Settings grammar). `--bg-inset`, radius `--r-xl`, hairline ring, rows split by `--line`. Rows: title, one line of description, trailing control.
- **Menu / popover.** `--mat-menu` with blur, `--edge` inner ring, `--shadow-menu`, radius `--r-lg`, 5px padding. Items 26px, radius `--r-sm`; hover/focus fills `--accent` with `--on-accent` text, like a native menu. Section labels in caption. Shortcuts trail in `--text-3`.
- **HUD pill.** `--hud-bg` with blur, `--hud-fg` text. Recording: 40px capsule with the red dot, timer and mic state. Dictation: the user picks a style and a position on Voice typing (`src/lib/Pill.svelte`): **Capsule** (default) 40px, level bars and words; **Orb** 32px, bars only while things go well, words for errors; **Waveform** 44px, scrolling level and a timer; **Card** 56px, two lines, a mic ring that swells with the level. Positions: bottom centre (default), top centre, bottom right, top right, inside the visible frame (clear of the menubar and Dock). Errors always say what's wrong and what to do (title + hint).
- **Transcript row.** Speaker and time in a 112px left margin, right-aligned; prose in the reading style. In the narrow inspector, speaker and time sit inline above the text. Never chat bubbles.
- **Possibly muted group.** Collapsed row between transcript rows: caution dot plus "N segments while possibly muted", with Show. Expanded, each segment has Play and Include. An included segment keeps a caution marker in its margin so the decision stays visible.
- **Chat.** Assistant messages are plain reading text. User messages are an accent bubble, right-aligned, radius 18 with the bottom-right corner 6.
- **Empty and error states.** A heading, one sentence, one action. No screen ever renders blank.

## Motion

- `--dur` 180ms with `--ease-out` for things appearing and toggling; `--dur-fast` 120ms with `--ease-in` for things leaving and for hovers. Popovers and toasts enter with an 8px rise plus fade.
- Only live indicators loop: the recording dot (`--pulse`) and level bars. Skeletons pulse while loading.
- **No jitter.** A screen never shifts or pops in after it appears. Load a screen's data before showing it (route `load`, or before the window shows); reserve space for anything that arrives later (fixed row heights, skeletons the size of the real content, `tabular-nums` for changing numbers); never let text wrap or controls resize when state changes. Polish is judged on the real app.
- `prefers-reduced-motion` sets every duration to 0 and stops loops. `prefers-contrast: more` makes hairlines strong and lifts `--text-3` to `--text-2`.

## Accessibility

- Text 4.5:1 minimum on every surface, both themes, every accent (enforced by `check-contrast.mjs`).
- Visible focus: 2px `--focus-ring`, 2px offset, on every interactive element. Menus move focus with arrows and close on Esc.
- Status never relies on colour alone.
- Hit areas at least 24×24 even when the glyph is smaller.

## Voice

Plain and short. Say what happens: "Writing up your notes…", "Added where you were typing". No marketing adjectives, no exclamation marks. Sample content is always labelled "Sample data".

## Open decisions

These are places where the prototype and the spec disagree. They need your call before the stories that touch them:

1. **Module names.** The prototype says "Voice typing" and "Meeting notes". `PRODUCT.md` and the spec say "Dictation" and "Meetings".
2. **Possibly-muted UI.** The spec's core privacy feature does not appear in the latest prototype's transcript panel. The component above is drawn from the spec; the specimen shows it.
3. **Older macOS.** Myna draws its own window corners, panels and shadows, and gets the blur from the same native vibrancy view on every version, so it should look the same on 14.2, 15 and 26. Story 01 checks this on real machines (fine to differ, not fine to look bad).
