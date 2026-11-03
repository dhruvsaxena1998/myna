# 01: App shell and menubar

**Status:** ready
**GitHub:** milestone "01 · App shell"
**Spec stories:** 1–5
**Depends on:** none

The bare app: lives in the menubar with no Dock icon, opens a native-looking popover on click, has a Settings window with every page listed, and stores data under Application Support.

## Stories
- As a user, I want Myna in the menubar with no Dock icon, so that it stays out of the way.
- As a user, I want clicking the menubar icon to open a popover that looks like Control Center or the Wi-Fi menu, so that Myna feels part of macOS.
- As a user, I want right-click on the icon to give a plain menu with Settings and Quit, so that I can always get out.
- As a user, I want Settings in its own window with a sidebar of pages, so that I can find any option.
- As a user, I want to pick light or dark, a highlight colour and clear or tinted panels, so that Myna looks the way I like.
- As a user on macOS 26 or later, I want to try Apple's Liquid Glass on Myna, so that it matches the rest of my Mac.

## Done when
- [ ] Menubar icon (template image, tints for light and dark); no Dock icon
- [ ] Popover uses system material, menu metrics, accent highlight; closes on click-away and Esc; keyboard navigable
- [ ] Settings window lists all pages; every page shows its error if its data fails to load, never blank
- [ ] Data folder `~/Library/Application Support/Myna/` with SQLite created on first launch
- [ ] Look & appearance sets light/dark, highlight colour and see-through panels, saved and applied live to every window
- [ ] Liquid Glass (experimental) gives the pill and the Settings window real system glass on macOS 26+, off by default
- [ ] Checked on the real app from a fresh install, light and dark, on macOS 26 and on 14.x or 15.x: window corners, shadow, sidebar vibrancy and traffic lights look right on both

## Decisions
- Settings is its own window (menu, `⌘,`), sidebar of pages like macOS Settings. The main window (Meetings, Dictation history) comes with stories 04 and 05.
- Popover shows only real rows: a "Myna" header, **Settings… ⌘,** and **Quit Myna ⌘Q**. Stories 04 and 05 add their rows when built; no greyed-out placeholders.
- Left- and right-click both open a real native menu (NSMenu). Revised 2026-11-02 after the panel prototype looked worse than the native menu. Nothing in the planned menu needs a custom panel: section headers (`NSMenuItem.sectionHeader`), trailing text like `00:15`, `fn`, `Wed` (`NSMenuItemBadge`), the mic-on status row (disabled item with an image) and the live timer (update while open) are all native on macOS 14+. Tauri's menu API lacks headers and badges, so stories 04/05 build the NSMenu directly via objc2.
- Every Settings page is in the sidebar, named as in the prototype: Look & appearance, Voice typing, Meeting notes, AI & privacy, Experimental features, About (revised 2026-11-02: models, AI providers and the Decision provider, shown as "Quick decisions", share AI & privacy). Unbuilt pages show their title and one line on what will live there. About is real (version, attribution slot). One shared error state.

## Tickets
1. Scaffold: Tauri 2, template menubar icon, no Dock icon, native right-click menu (Settings, Quit)
2. Popover panel prototype: vibrancy panel under the icon, judged by you on a Mac (human)
3. Popover: rows, keyboard navigation, Esc and click-away close, light and dark (blocked by 2)
4. Settings window: sidebar of every page, placeholder pages, shared error state, About
5. Data folder and SQLite on first launch, with schema version table; errors shown
6. Fresh-install check on macOS 26 and 14 or 15 (human; blocked by 1 to 5)
7. (#60) Look & appearance page: Appearance, Highlight colour, See-through panels, saved and applied live (detail below)
8. (#61) Liquid Glass (experimental): native NSGlassEffectView on the pill and the Settings window, macOS 26+, blocked by 7 (detail below)

## Ticket 7: Look & appearance

Build the page as drawn in `docs/index.html` (Settings › Look & appearance): three rows in an "Appearance" group.
- **Appearance:** Auto, Light, Dark (segmented). Auto follows the Mac. Default Auto.
- **Highlight colour:** 8 swatches (blue, purple, pink, red, orange, yellow, green, graphite), radio group. Default blue. The description names the current colour.
- **See-through panels:** Clear or Tinted (segmented). Tinted washes the materials with the highlight colour. Default Clear. The description changes with the choice, as in the prototype.

How:
- `design/tokens.css` already does the work: `<html data-theme>`, `data-accent`, `data-glass="tinted"`. The ticket only saves the three values and sets those attributes.
- Saved in the `setting` table like `pill_style`/`pill_place` (`pill.rs` `read`/`set_look`): unknown values fall back to the default. Commands `look_get`/`look_set`; `look_set` emits `look` to every window.
- Every window (settings, pill) sets the attributes before first paint, so nothing flashes from the default look (no jitter). The page loads them in `+layout.ts` or equivalent, not after mount.
- A save that fails reverts the control and shows the error, like PillLook.
- Native window vibrancy follows Light/Dark too: the settings window's `sidebar` effect and the native menu use the chosen appearance (`NSWindow.appearance`; Auto clears it), so a dark page never sits on light vibrancy.

Done when:
- [ ] The page matches the prototype's rows, wording and controls, light and dark, keyboard and VoiceOver usable
- [ ] Changing any row updates the settings window and the pill at once, and survives a restart
- [ ] No flash of the default look on opening Settings or showing the pill
- [ ] Tests: Rust unit tests for read/save and fallbacks; Playwright for the three rows and the attributes they set

## Ticket 8: Liquid Glass (experimental)

Research: `.scratch/glass-ui/research.md` (Glisse ruled out: React-only, and its refraction is Chromium-only).

- Experimental features gets a master switch at the top, **Use experimental features**, "New things we're still working on. They may change. Turning this off turns them all off." Off by default. While off, the feature groups are hidden and every experimental feature acts off; their own switches keep their saved state. (Added 2026-11-03.)
- First, add the row to `docs/index.html` (Experimental features, in a "Look" group below the master switch): **Liquid Glass**, switch, "Uses macOS's own glass for the pill and the Settings window. Still being tested." Off by default. (Revised 2026-11-03: was on Look & appearance and pill-only.) Shown only on macOS 26 or later; on 14/15 the row isn't there.
- When on: Rust puts an `NSGlassEffectView` (objc2) behind the pill. (Planned via `window_vibrancy::apply_liquid_glass`; dropped because it manages one glass view per window and puts it under the window's vibrancy.) Style Clear when See-through panels is Clear, tinted with the highlight colour when Tinted. Corner radius follows the pill style (capsule, orb, wave, card differ in height). The pill page drops its CSS `--hud-bg` fill and backdrop blur so only the native glass shows. When off: `clear_liquid_glass` and today's look.
- The glass view must sit exactly behind the drawn pill: the pill window is wider than the pill (720×100), so either size the glass view to the pill's rect or resize the window to the pill. Pick the one with no layout shift when the pill changes state.
- Settings window: when on, one sheet of Regular glass (tinted with the highlight colour when Tinted) goes behind the whole window in place of its `sidebar` vibrancy, which is hidden meanwhile (glass over the vibrancy only shows the vibrancy, near-white in light mode). The sidebar, page and grouped lists thin into frosted fills on it, so the page is see-through while Liquid Glass is on (DESIGN.md, Liquid Glass). Revised 2026-11-03 after hand checks: glass behind the panels over an opaque page looked too dark in dark and too white in light. When off: the vibrancy and opaque page come back.
- Toggling, or changing pill style, highlight colour or See-through panels, updates the glass at once, no restart.
- The menubar menu is a native NSMenu and already gets system glass on macOS 26; nothing to do.

Risk: window-vibrancy issue #229 reports AppKit crashes on macOS 27 in packaged builds with a glass view behind the webview. The developer's Mac is on macOS 27.

Done when:
- [ ] Master switch and Liquid Glass row in `docs/index.html` and in the app; Liquid Glass hidden below macOS 26 and while the master switch is off
- [ ] Pill shows real system glass in all four styles and positions, light and dark, clear and tinted
- [ ] Settings window shows system glass with frosted panels, light and dark, clear and tinted; text stays readable over light and dark desktops
- [ ] Off restores today's pill and Settings window exactly
- [ ] Tests: Rust unit test for the saved switch and the macOS-version gate; Playwright for the row and its hiding
- [ ] Checked by hand in a dev build and a packaged build (`pnpm tauri build`) on macOS 27; if the packaged build crashes, the switch stays hidden and the ticket says so
