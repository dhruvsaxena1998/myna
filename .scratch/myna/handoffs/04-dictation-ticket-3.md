# Handoff: story 04 (Dictation), next is ticket 3 (shortcut and mode)

Repo `/Users/dhruv@shurutech.com/Dev/dhruvsaxena1998/myna`, branch `feat/04-dictation` (pushed, tracks origin). Read `AGENTS.md`, then `.scratch/myna/stories/04-dictation.md`. GitHub: milestone "04 · Dictation", story #47, ticket 04.3 is #50.

## Done
- **04.1** (`9b9f642`, checked by the user in the app): hold Option+Space → cpal mic → 16 kHz mono → active model → paste (NSPasteboard + synthetic Cmd-V, clipboard restored). Model unloads when idle (AI & privacy). Microphone and Accessibility rows on Voice typing.
- **04.2** (`d60a500`): the pill, in four styles and four positions.
  - `src-tauri/src/pill.rs`: the `pill` window is never shown through Tauri (that would make it key and break the paste). `setup` orders it front, transparent, click-through (`setIgnoresMouseEvents(true)`), status level, all Spaces. `place` moves it to the screen the user is typing on at each key press. `show` / `error(title, hint)` / `level` emit to the page. `look` / `set_look` store `pill_style` and `pill_place` in the `setting` table.
  - `src/lib/Pill.svelte`: the four styles (capsule, orb, wave, card), drawing only. `src/routes/pill/+page.svelte` feeds it events and hides it after Added (1.6 s) or an error (4 s). `src/lib/PillLook.svelte`: Style, Position and a preview on Voice typing.
  - `lib.rs` `dictate_start` checks the mic, Accessibility and model before recording and says what's missing on the pill. A level thread sends the mic loudness at 20 Hz while recording.
  - Throwaway prototype: `.scratch/myna/prototype/pill-variants.html`.

## Next: ticket 04.3 (#50)
Voice typing on/off switch; shortcut recorder that saves to SQLite and re-registers without a restart (keep the old shortcut if another app holds the new one); Hold to talk or Tap to start and stop; tap mode capped at 5 minutes with the pill counting down the last 30 s; menubar menu shows the shortcut. Design: `hotkeyRec` and the Voice typing page in `docs/index.html`.
- The shortcut is the `DICTATE` const in `lib.rs` (marked `shortcut:`), registered in `setup`.
- "Release to paste" is hard-coded in `Pill.svelte` (capsule and card). Show it in hold mode only.
- **Risk: the Done button.** Tap mode puts a Done button on the pill, but the pill window ignores the mouse. A click on a normal NSWindow of an accessory app activates Myna and takes focus from the target app. Making it clickable likely needs a non-activating NSPanel (or an equivalent that doesn't activate), and only the pill's own area may catch clicks, not the whole 720×100 window. Settle this before building the rest; the shortcut pressed again also stops, so Done may be optional.
- The countdown must fit all four styles. Waveform already shows a timer (`secs` prop).

## Checks
`cd src-tauri && mise exec cmake@4.4.5 -- cargo test` (19 pass, 3 ignored), clippy clean, `pnpm check`, `pnpm test:ui` (34 pass). Run the app with `mise exec cmake@4.4.5 -- pnpm tauri dev` from the repo root (port 1420 must be free); it's ready when the log shows ``Running `target/debug/myna` `` (the log has ANSI codes).

## Rules from the user
- One ticket at a time; the user reviews each. Commit or push only when asked.
- When a ticket is done: start the app (restart it if it's running) and end with a short manual-test checklist.
- Never click, type in or screenshot the app. Test with unit tests and Playwright.
- Build to `docs/index.html`, `docs/flow.html` and `DESIGN.md`. No jitter. No hacky workarounds.

## Known loose ends
- #56 (04.9): the pill under the text cursor.
- The pill has no real blur of the desktop (CSS can't blur behind a transparent window). Orb errors stay 32px tall.
- Cmd-V uses key code 9 (QWERTY V); the typing fallback is 04.6. Box-filter resampler, marked `shortcut:` in `dictation.rs`.
