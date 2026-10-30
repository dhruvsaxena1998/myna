# 01: App shell and menubar

**Status:** draft
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
- [ ] Checked on the real app from a fresh install, light and dark

## Open questions for you
- Exactly which rows the popover shows before any module is built?
- Settings as a window, or inside the popover like some menubar apps?
