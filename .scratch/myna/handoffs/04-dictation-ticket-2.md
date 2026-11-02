# Handoff: story 04 (Dictation), next is ticket 2 (the pill)

Repo `/Users/dhruv@shurutech.com/Dev/dhruvsaxena1998/myna`, branch `feat/04-dictation`. Read `AGENTS.md`, then `.scratch/myna/stories/04-dictation.md`. GitHub: milestone "04 · Dictation", story #47, ticket 04.2 is #49.

## Done
- 04.1 (commit 9b9f642, user checked it in the real app: Parakeet dictation works and is fast). Hold Option+Space → cpal mic → 16 kHz mono → active model → paste via NSPasteboard + synthetic Cmd-V, clipboard restored item by item.
  - `src-tauri/src/dictation.rs`: `record()` / `Recording::finish()`, `to_16k_mono`, `text()`, `paste()`.
  - `src-tauri/src/lib.rs`: `dictate_start` / `dictate_stop` (global-shortcut handler, Pressed/Released), `load_transcriber` (model loads while the user talks), `schedule_unload` (idle unload, setting on AI & privacy, default 5 min, key `stt_unload_secs` in the new `setting` table).
  - `src-tauri/src/permissions.rs` + `src/lib/Permissions.svelte`: Microphone and Accessibility rows on Voice typing.
- Every dictation error currently goes to stderr only (`eprintln!("dictation: …")`, marked `shortcut:` in `dictate_start`). 04.2 replaces that with the pill.

## Next: ticket 04.2 (#49)
Floating HUD pill, bottom centre, never takes focus from the app being typed into (needs a non-activating panel; a normal Tauri window steals focus and breaks the paste). States: Listening with level bars from the real mic level ("Release to paste" in hold mode), Writing…, Added where you were typing, and errors (no model, permission missing, nothing heard). Dark in both themes, 40px capsule. Design: `.hud`, `.dpill`, `.lvl` and the overlay render (~line 1451) in `docs/index.html`; HUD pill in `DESIGN.md`. Playwright covers each state against the fake Tauri (`tests/tauri.ts`, `window.__emit`).

## Checks
`cd src-tauri && mise exec cmake@4.4.5 -- cargo test` (17 pass, 3 ignored), clippy clean, `pnpm check`, `pnpm test:ui` (17 pass). Run the app with `mise exec cmake@4.4.5 -- pnpm tauri dev` (port 1420 must be free). When waiting for it to start, grep the log for "Running" without anchoring on colours (the log has ANSI codes).

## Rules from the user
- One ticket at a time; the user reviews each. Commit or push only when asked.
- Never launch, click or screenshot the app unasked. Test with unit tests and Playwright.
- Build to `docs/index.html`, `docs/flow.html`, `DESIGN.md`. No jitter. No hacky workarounds.

## Known loose ends
- Hold mode has no length cap yet (Parakeet takes about 4–5 min per call); 04.3 adds the 5-minute cap.
- Cmd-V uses key code 9 (QWERTY V); the typing fallback is 04.6.
- Box-filter resampler, marked `shortcut:` in `dictation.rs`.
