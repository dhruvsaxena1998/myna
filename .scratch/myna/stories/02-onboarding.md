# 02: Onboarding to a working app

**Status:** built on `feat/onboarding` (2026-11-21), awaiting a clean-Mac test
**Spec stories:** 1, 2, 3, 5a–5d
**Depends on:** 01, 03; then grows as 04/05/09 land

First launch takes a new user, including a teammate who has never seen the app, from nothing to a first dictation or recording. No terminal, no manual setup.

## Stories
- As a new user, I want to pick Dictation, Meetings or both (both on by default), so that I only download what I need.
- As a new user, I want permissions explained one at a time before macOS asks (Microphone, Accessibility, Screen Recording, System Audio), so that I know why.
- As a new user, I want the model each module needs downloaded during onboarding with progress, so that the app works when setup ends.
- As a new user who enabled Meetings, I want to see how to install the browser extension, so that mute mirroring works on my first call.
- As a new user, I want to pick my voice typing key (fn, Option+Space, or my own), so that it fits how I work. Picking fn walks through the 🌐-key setting (story 04, ticket 4).
- As a new user who picked Voice typing, I want to say which language I speak (pre-filled from my Mac's language) on "What to help with", so that rules cleanup and Apple Speech know it. Same setting as Language you speak on Voice typing (story 04, ticket 10).
- As a new user, I want a final screen telling me what to try first, so that I know it works.
- As a user, I want to turn the other module on later and be walked through only its missing steps.

## Done when
- [ ] Module choices are saved correctly and both default on
- [ ] Each permission step shows granted or not, with a way to open System Settings
- [ ] Model download step with size, progress, cancel, retry, checksum verified
- [ ] Extension step (only if Meetings): Show folder button, pairing token
- [ ] Finish screen: hotkey for dictation, where Start meeting is
- [ ] A teammate on a clean Mac reaches a first dictation and a first recording with onboarding alone

## Open questions for you
- Which permissions are asked in onboarding vs. on first use?
- Skip the model download if the user is offline, or block?

## Tickets (2026-11-21)
- 02.1 Setup window at first launch; existing installs marked done; reopens until Done. **Built.**
- 02.2 What to help with: Voice typing / Meeting notes, both on. **Built.** (Language you speak: not in setup yet.)
- 02.3 Permissions step, only the ones the picked parts need. **Built** (reuses Permissions.svelte).
- 02.4 Voice typing key: recorder, fn + 🌐 Do Nothing hint. **Built.** (No preset buttons, no Hold/Tap.)
- 02.5 Models: recommended STT (+ Who for meetings), progress, cancel, retry; Apple Speech meanwhile. **Built.**
- 02.6 Browser add-on step (Meetings only). **Built** (reuses BrowserAddon.svelte).
- 02.7 Finish screen: the key to try, where Start meeting is. **Built.**
- 02.8 Settings › General › Set up Myna again. **Built.**
- 02.9 Turning a part on later walks only its missing steps. Not built: Settings already shows permissions inline.
- 02.10 Clean-Mac test: first dictation + first recording via setup alone. Open.
