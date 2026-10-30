# 05: Meeting recording

**Status:** draft
**Spec stories:** 16–18, 29
**Depends on:** 01, 03

Start and stop a meeting from the menubar; mic and system audio recorded as separate tracks, then transcribed after stop.

## Stories
- As a user, I want Start/Stop meeting in the popover whenever Meetings is on, with elapsed time while recording.
- As a user, I want mic and system audio as separate tracks, system audio per process with no virtual driver.
- As a user, I want transcription after stop with visible progress, Me and Them labelled.
- As a user, I want a Meetings window listing my meetings and showing a transcript as a document.

## Done when
- [ ] Start meeting visible in the popover when Meetings is enabled
- [ ] Real call recorded: both tracks on disk, transcript shows Me and Them
- [ ] Progress visible; failure shows a reason
- [ ] Checked against a real meeting on the user's Mac

## Open questions for you
- Should it compare against Granola/Fathom on the same call as the acceptance test?
