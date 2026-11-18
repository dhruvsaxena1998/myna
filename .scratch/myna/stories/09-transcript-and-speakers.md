# 09: Speakers and transcript

**Status:** draft. **Next up:** speaker editing (rename, hear their clips, merge) is the priority for the next round (user, 2026-11-18). Diarization already runs (Sortformer, story 07).
**Spec stories:** 30–35, 38
**Depends on:** 05

Speakers separated automatically; rename once and the whole meeting follows; remembered voices suggested later.

## Stories
- As a user, I want speakers separated on the system track.
- As a user, I want to rename a speaker once, hearing their longest clips, and merge split speakers.
- As a user, I want remembered voices suggested in later meetings until I confirm.
- As a user, I want to play any segment and export Markdown or text.

## Decisions
- Speaker names are sent to the AI as the transcript shows them, so a Summary can say who owns what (decided 2026-11-17, story 16). A rename changes the input of every chunk that speaker is in, so the next Summary run re-does those chunk notes.

## Done when
- [ ] Diarization runs with the chosen model
- [x] Rename, hear their clips, merge (`.scratch/speaker-editing`, 2026-11-18)
- [ ] Remember and suggest voices across meetings (`.scratch/speaker-editing/issues/04`)
- [ ] Checked on a real multi-person call

## Open questions for you
- Default diarizer: Who (Desert Ant) or Sortformer?
