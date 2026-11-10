# 05: Meeting recording

**Status:** ready
**GitHub:** milestone "05 · Meetings"
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

## Decisions
- UI words follow the design: "Meeting notes" in menus and Settings, "Meetings" in the sidebar (DESIGN.md, open question 1).
- Menubar menu (native NSMenu): a "Meeting notes" section with "Start meeting notes" and its shortcut, the note "Records whichever app is using your microphone.", or disabled with "Meeting notes are off. Turn them on in Settings." While recording: "Stop recording" with the elapsed time, and the menubar icon shows the red dot and timer.
- Settings › Meeting notes: "Use meeting notes" switch; "Start or stop notes" shortcut, ⌃⌥M (default), ⌃⌥N or none. Onboarding (story 02) isn't built, so the System Audio permission row lives on this page, like Microphone and Accessibility on Voice typing.
- Them is the apps that are using the microphone, captured with Core Audio process taps. macOS 14.2 is already the floor, so no ScreenCaptureKit fallback and no virtual driver. Apps that start using the mic after Start are picked up while recording. Myna itself is never tapped.
- Each meeting gets a folder under the data folder with `mic.wav` and `them.wav`, written as they record so a crash or quit keeps the audio. A meeting that was cut off is finished and transcribed on next launch.
- Transcription runs after Stop, per track, in chunks under the Parakeet limit, then merges by time: mic is "Me", system audio is "Them". Splitting Them into people is story 09.
- Progress: the recording pill and the list's live card say "Writing up your notes…" with progress. A failure says why and offers Try again; the audio is kept.
- Recording pill: the design's HUD capsule (red dot, "Recording", timer, "Mic live"). Clicking it does nothing until live notes exist.
- Meeting page: "Untitled meeting", date · time · duration · source app. Until summaries (story 16) the transcript is the document body, with Copy; when 16 lands it moves to the Transcript side panel.
- Not in this story: Mute mic (06), live notes, Meeting detected toast, folders and tags (11), search and chat (16, 17), titles, the summary, cloud speech-to-text (03.6).

## Tickets
1. Meeting notes on/off and mic-only recording: Settings page, shortcut, menubar Start/Stop with timer, recording pill, `mic.wav` on disk, meeting saved in SQLite
2. Them track: Core Audio process tap of the apps using the mic, `them.wav`, System Audio permission row, clear error when denied
3. Transcribe after Stop: chunked per track, merged Me/Them, progress, failure reason and Try again, cut-off meetings resumed on launch
4. Meetings in the main window: sidebar entry, date-grouped list, live card, Start notes button, meeting page with the transcript as a document
5. Real meeting check on the user's Mac (human)

## Open questions for you
- Should it compare against Granola/Fathom on the same call as the acceptance test?
