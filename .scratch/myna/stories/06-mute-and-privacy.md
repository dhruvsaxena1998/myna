# 06: Mute state and private segments

**Status:** draft
**Spec stories:** 19–24
**Depends on:** 05

Speech recorded while muted, or while mute state is unknown, stays out of the transcript until the user includes it.

## Stories
- As a user, I want a mic-live indicator in the pill and menubar at all times while recording.
- As a user, I want a manual mute toggle and hotkey in Myna.
- As a user, I want muted or unknown speech collapsed, with Play and Include per segment.
- As a user, I want a per-app "always record mic" switch.

## Done when
- [ ] Mute timeline drives segment state (unmuted, muted, unknown)
- [ ] Collapsed groups with Play and Include; included keep a marker
- [ ] Nothing collapsed reaches any AI request (tested)
- [ ] Checked on a real call

## Open questions for you
- Is fail-safe "unknown = hidden" still what you want for apps without mirroring?
