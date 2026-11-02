# 04: Dictation

**Status:** ready
**GitHub:** milestone "04 · Dictation"
**Spec stories:** 6–12, 15
**Depends on:** 01, 03

Hold a hotkey, speak, release; text appears in the focused app.

> Amended by [Speech-to-text through an Endpoint](../../local-ai/issues/13-cloud-stt-endpoint.md) and [LLM cleanup for dictation and transcripts](../../local-ai/issues/28-llm-cleanup.md): Dictation is always local; two hotkeys, Dictate and Dictate with cleanup (LLM cleanup on an Endpoint, 3 s budget, guards, app-bound Styles), replacing a cleanup toggle. Dictation cleanup and Styles leave Experimental.

## Stories
- As a user, I want hold-to-talk on a global hotkey, and a toggle mode, so that short and long dictation both work.
- As a user, I want to change the hotkey in Settings and have it work immediately.
- As a user, I want a small floating pill showing level and state.
- As a user, I want text pasted with my clipboard restored, and a typing fallback for apps that reject paste.
- As a user, I want fillers and stutters removed by rules, and a personal dictionary.

## Done when
- [ ] Default hotkey works; changing it in Settings takes effect without restart
- [ ] Pill states: idle, recording, processing, error
- [ ] Paste into Notes, a browser field and Terminal; clipboard restored
- [ ] Rules cleanup and dictionary covered by unit tests
- [ ] Checked on the real app from a fresh install

## Decisions
- Default hotkey is Option+Space, through tauri-plugin-global-shortcut. Onboarding (story 02) will ask which key to use, fn included.
- Tap to start and stop is capped at 5 minutes, which fits one Parakeet call with no chunking. The pill counts down the last 30 s, then stops and pastes.
- The speech-to-text model unloads after 5 idle minutes by default, set under AI & privacy since meetings share it.
- Microphone and Accessibility permissions are rows on the Voice typing page (state, and Allow opens the System Settings pane). Onboarding reuses the same rows.

## Tickets
1. End to end: hold Option+Space → mic via cpal, resampled to 16 kHz mono → active speech-to-text model through `runtime::transcriber` → clipboard paste (arboard plus synthetic Cmd-V), clipboard restored. Voice typing page gets the Microphone and Accessibility rows. Command-layer test: a WAV plus the Fake transcriber gives the pasted text. The model unloads when idle (AI & privacy: right after use, 1, 5 (default), 15 or 60 minutes, or never)
2. Pill: a floating HUD that never takes focus, showing Listening with level bars, Writing…, Added, and errors (no model, no permission, nothing heard). Style (Capsule, Orb, Waveform, Card) and position (bottom centre, top centre, bottom right, top right) are picked on Voice typing
3. Shortcut and mode on Voice typing: the module's on/off switch, a shortcut recorder that re-registers without a restart, Hold to talk or Tap to start and stop, the 5-minute cap with its countdown
4. fn as a shortcut: event tap on modifier changes, plus a check of the 🌐-key setting with the steps to set it to Do Nothing
5. Rules and dictionary: filler list, repeated-word collapse and dictionary replacements as pure functions with table tests; "Words to remember" on Voice typing
6. Typing fallback: paste method setting (paste, or type it out) and paste delay; checked in Notes, a browser field and Terminal
7. Dictate with cleanup hotkey: starts unassigned and shows "Needs an Endpoint". Blocked by story 10
8. Fresh-install check (human)
9. Pill under the text cursor: a fifth position read through Accessibility, falling back to bottom centre where the app doesn't report a cursor

## Open questions for you
- none
