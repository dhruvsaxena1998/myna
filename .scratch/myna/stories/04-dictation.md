# 04: Dictation

**Status:** draft
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

## Open questions for you
- Default hotkey: Option+Space, Fn, or something else?
- Recording cap for toggle mode?
