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

## Done when
- [ ] Menubar icon (template image, tints for light and dark); no Dock icon
- [ ] Popover uses system material, menu metrics, accent highlight; closes on click-away and Esc; keyboard navigable
- [ ] Settings window lists all pages; every page shows its error if its data fails to load, never blank
- [ ] Data folder `~/Library/Application Support/Myna/` with SQLite created on first launch
- [ ] Checked on the real app from a fresh install, light and dark, on macOS 26 and on 14.x or 15.x: window corners, shadow, sidebar vibrancy and traffic lights look right on both

## Decisions
- Settings is its own window (menu, `⌘,`), sidebar of pages like macOS Settings. The main window (Meetings, Dictation history) comes with stories 04 and 05.
- Popover shows only real rows: a "Myna" header, **Settings… ⌘,** and **Quit Myna ⌘Q**. Stories 04 and 05 add their rows when built; no greyed-out placeholders.
- Left- and right-click both open a real native menu (NSMenu). Revised 2026-11-02 after the panel prototype looked worse than the native menu. Nothing in the planned menu needs a custom panel: section headers (`NSMenuItem.sectionHeader`), trailing text like `00:15`, `fn`, `Wed` (`NSMenuItemBadge`), the mic-on status row (disabled item with an image) and the live timer (update while open) are all native on macOS 14+. Tauri's menu API lacks headers and badges, so stories 04/05 build the NSMenu directly via objc2.
- Every Settings page is in the sidebar: Modules, Dictation, Meetings, Models, AI providers, Experimental, About. Unbuilt pages show their title and one line on what will live there. About is real (version, attribution slot). One shared error state.

## Tickets
1. Scaffold: Tauri 2, template menubar icon, no Dock icon, native right-click menu (Settings, Quit)
2. Popover panel prototype: vibrancy panel under the icon, judged by you on a Mac (human)
3. Popover: rows, keyboard navigation, Esc and click-away close, light and dark (blocked by 2)
4. Settings window: sidebar of every page, placeholder pages, shared error state, About
5. Data folder and SQLite on first launch, with schema version table; errors shown
6. Fresh-install check on macOS 26 and 14 or 15 (human; blocked by 1 to 5)
