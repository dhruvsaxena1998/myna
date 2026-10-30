# 07: Browser extension (Meet and Teams)

**Status:** draft
**Spec stories:** 20, 36
**Depends on:** 05, 06

A Chromium extension mirrors the mute button and, with captions on, speaker names into Myna over a paired localhost channel.

## Stories
- As a user, I want installing the extension guided from inside Myna.
- As a user, I want my Meet/Teams mute state mirrored, showing "mirrored from Meet".
- As a user, I want caption speaker names offered as suggestions.
- As a user, I want "mute unknown" rather than a guess when the page changes.

## Done when
- [ ] Install steps in onboarding and Settings, with Show folder and token
- [ ] Localhost only, token auth, Origin rejected, size caps (security reviewed)
- [ ] Mute mirrored on a real Meet and Teams call
- [ ] Selectors updatable without an app release

## Open questions for you
- Chrome Web Store listing instead of Load unpacked?
- Teams web only, or also native Teams later?
