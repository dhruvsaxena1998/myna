# 02: Onboarding to a working app

**Status:** draft
**Spec stories:** 1, 2, 3, 5a–5d
**Depends on:** 01, 03; then grows as 04/05/09 land

First launch takes a new user, including a teammate who has never seen the app, from nothing to a first dictation or recording. No terminal, no manual setup.

## Stories
- As a new user, I want to pick Dictation, Meetings or both (both on by default), so that I only download what I need.
- As a new user, I want permissions explained one at a time before macOS asks (Microphone, Accessibility, Screen Recording, System Audio), so that I know why.
- As a new user, I want the model each module needs downloaded during onboarding with progress, so that the app works when setup ends.
- As a new user who enabled Meetings, I want to see how to install the browser extension, so that mute mirroring works on my first call.
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
