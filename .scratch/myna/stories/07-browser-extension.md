# 07: Browser extension (Teams, then Meet)

**Status:** ready
**GitHub:** milestone "07 · Browser extension"
**Spec stories:** 20, 36
**Depends on:** 05 (and the core of 06, built here as ticket 2)

A Chromium extension mirrors the mute button, records only the call tab's audio and reads caption speaker names, sending them to Myna over a paired localhost channel. Teams web first, because that's what the user's calls run on; Meet follows.

## Stories
- As a user, I want installing the extension guided from inside Myna.
- As a user, I want my Teams/Meet mute state mirrored, showing "mirrored from Teams".
- As a user, I want only the call tab recorded as the other side, so other tabs' sound stays out.
- As a user, I want the other side split into speakers, named from the call's captions.
- As a user, I want "mute unknown" rather than a guess when the page changes.

## Done when
- [ ] Install steps in Settings, with Show folder and connection code (onboarding gets them when story 02 is built)
- [ ] Localhost only, token auth, Origin rejected, size caps (security reviewed)
- [ ] Mute mirrored on a real Teams call (Meet after)
- [ ] Them recorded from the call tab only
- [ ] Them lines split into speakers, named from captions when captions are on
- [ ] Selectors updatable without an app release

## Decisions
- Teams web (`teams.microsoft.com`, `teams.live.com`, `teams.cloud.microsoft`) first; Meet is its own ticket after.
- Load unpacked for now: the extension lives in `extension/` in the repo, Settings has Show folder. A fixed manifest `key` keeps the extension id stable, so Myna can check the Origin exactly. Chrome Web Store is a later decision.
- Channel: a WebSocket on 127.0.0.1 only, started while Meeting notes is on. The connection code (design: `MYNA-XXXX-XX`) is the token; the extension sends it first, anything else is dropped. Origin must be the extension's `chrome-extension://<id>`. Messages are size-capped; audio frames have their own cap.
- Settings › Meeting notes gets the design's "Browser add-on" group: Status (Connected / Not connected, Connect / Disconnect) and Connection code.
- Mute: the extension reports `unmuted` / `muted` / `unknown` with a time; Myna keeps a mute timeline per meeting. The core of story 06 comes in here: Me lines said while mirrored-muted are left out of the transcript and shown as "Said while muted · left out" with Include. Only mirrored `muted` hides speech for now; `unknown` (no extension, page changed) keeps speech shown until story 06 settles "unknown = hidden" (its open question).
- Pill and menu say "Mic live · mirrored from Teams" / "Muted · mirrored from Teams" while mirroring.
- Tab audio: clicking the extension's icon on the call tab (Chrome needs that click for `tabCapture`) streams the tab's audio to Myna, which uses it as the Them track for the meeting instead of the process tap. The tab keeps playing out loud. Without the click, the process tap from 05.2 stays the fallback. Clicking the icon with no meeting recording starts one.
- Speakers: after Stop, the Them track is diarized with the active diarization model through `runtime::diarizer` (Sortformer now; Desert Ant Who plugs in behind the same trait with 03.4). Them lines become Speaker 2, Speaker 3… With Teams live captions on, the extension sends "name spoke at time" events, and each speaker takes the caption name it overlaps most. No captions: Speaker N. Renaming and remembered voices stay in story 09.
- Selectors live in one JSON the extension fetches from the repo's raw GitHub URL (cached, with a bundled copy as fallback), so a Teams UI change is fixed without an app release. When a selector misses, the extension reports `unknown` / "captions unavailable" rather than guessing.

## Tickets
1. Add-on pairing: extension skeleton, localhost WebSocket with code + Origin + caps, Settings "Browser add-on" group, Show folder
2. Teams mute mirroring + the core of 06: mute timeline, muted Me lines left out with Include, "mirrored from Teams" in pill and menu
3. Teams tab audio as the Them track: icon click → tabCapture → Myna, process tap as fallback
4. Speakers on the Them track: diarize with Sortformer (Who via 03.4), named from Teams captions when on
5. Selectors updatable without a release
6. Meet: mute, tab audio and captions for meet.google.com
7. Real Teams call check (human)

## Open questions for you
- Chrome Web Store listing instead of Load unpacked?
- Native Teams later? (The user's calls are Teams in Chrome.)
