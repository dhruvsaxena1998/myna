# Myna — phase 1 spec

Status: draft (user review pending)
Date: 2026-10-29, revised 2026-10-30

Stories by feature live in `stories/`. Nothing is built until the user has reviewed a story and asked for it.

## Problem Statement

Typing is the slowest part of talking to a computer, and meetings vanish the moment they end. Existing Mac tools solve one of these and send audio to someone's cloud to do it. Granola records your mic even when you are muted in the meeting app. Wispr Flow needs a server for cleanup. None let you pick the models, and none run without an account.

## Solution

Myna is a macOS app with two modules you enable separately at onboarding.

**Dictation.** Hold a hotkey, speak, release. Myna transcribes on-device and pastes the text into whatever app has focus. Filler words are stripped by rules; optional experimental passes use Desert Ant's Uhm model or a cloud LLM the user brings their own key for.

**Meetings.** Start a recording from the menubar. Myna captures microphone and system audio as separate tracks, plus periodic screenshots of the meeting window. After stop, it transcribes, diarizes speakers, and lets you rename a speaker once to label the whole meeting. It mirrors the meeting app's mute state where it can read it (Google Meet and Teams in Chromium browsers via a companion extension), and everywhere else it still records locally but keeps possibly-muted speech out of the transcript until you include it. Summaries and chat run through a user-supplied OpenAI-compatible provider.

All speech-to-text, diarization, and filler detection runs on the Mac. Every model slot is swappable from a curated list. Nothing leaves the machine unless the user turns on a feature that says it does.

## User Stories

### Install and onboarding
1. As a new user, I want to pick Dictation, Meetings, or both at first launch, with both on by default, so that I only download the models I need.
2. As a new user, I want the app to walk me through Microphone, Accessibility, and Screen Recording permissions in order, so that I know why each is needed before macOS asks.
3. As a user, I want to enable the other module later from settings, so that I am not locked into my first choice.
4. As a user, I want the app to update itself, so that I do not re-download DMGs.
5. As a user, I want the app to live in the menubar with no Dock icon by default, so that it stays out of the way.
5a. As a new user, I want onboarding to download the model each enabled module needs, with progress, so that I can use the app the moment setup ends.
5b. As a new user who enabled Meetings, I want onboarding to show how to install the browser extension, so that mute mirroring works on my first call.
5c. As a new user, I want the last onboarding screen to tell me exactly what to try first, so that I know the app works.
5d. As a teammate the app was shared with, I want to get from download to a first dictation and a first recording through onboarding alone, with no terminal or manual setup.

### Dictation
6. As a user, I want to hold a global hotkey to record and release to transcribe, so that dictation feels like push-to-talk.
7. As a user, I want to switch to press-to-start, press-to-stop, so that long dictation does not strain my hand.
8. As a user, I want a small floating pill that shows level and state (idle, recording, processing), so that I know the app heard me.
9. As a user, I want the text pasted into the focused app via the clipboard with my previous clipboard restored, so that dictation does not clobber what I copied.
10. As a user, I want a keystroke-simulation fallback for apps that reject paste, so that dictation works in terminals and password-free fields.
11. As a user, I want filler words and stutters removed by deterministic rules, so that the output reads cleanly without surprises.
12. As a user, I want a personal dictionary for names and jargon, so that the transcriber spells them right.
13. As a user, I want to pick the speech-to-text model from a curated list with size, speed, and quality labels, so that I can trade accuracy for latency.
14. As a user, I want to point the app at a custom model path or Hugging Face repo for the same runtime, so that I am not limited to the curated list.
15. As a user, I want the dictation pipeline to never hang on a cloud call, so that a slow network falls back to local text within a timeout.

### Meetings: capture
16. As a user, I want to start and stop a meeting recording from the menubar, so that I do not need to open a window.
17. As a user, I want microphone and system audio recorded as separate tracks, so that my voice is always identifiable as "me".
18. As a user, I want system audio captured per process without a virtual audio driver, so that my music player is not in the transcript.
19. As a user, I want a persistent mic-live indicator in the pill and menubar, so that I can never be recorded without seeing it.
20. As a user, I want Myna to mirror my mute state in Google Meet and Teams in Chromium browsers, so that muting in the meeting mutes the transcript.
21. As a user, I want speech recorded while I was muted or while mute state was unknown kept out of the transcript by default, so that my private asides do not appear.
22. As a user, I want those excluded segments shown collapsed with per-segment "include" and audio replay, so that nothing is lost if the mute detection was wrong.
23. As a user, I want a manual mute toggle and hotkey in Myna, so that I can control the mic track when no mirroring exists.
24. As a user, I want a per-app "always record mic" switch, so that I can opt out of the fail-safe for apps I trust.
25. As a user, I want screenshots of the meeting window every 15 seconds by default, with near-duplicates dropped, so that I can see what was on screen without filling my disk.
26. As a user, I want to switch capture to off or full window video, so that I choose the storage trade-off.
27. As a user, I want the meeting window auto-picked from whichever app holds the mic, with a manual override, so that I rarely have to choose.
28. As a user, I want a size estimate before window video starts, so that I know what a one-hour meeting costs on disk.

### Meetings: transcript
29. As a user, I want transcription to run after I stop with visible progress, so that I know when it is ready.
30. As a user, I want speakers separated automatically, so that I can read who said what.
31. As a user, I want to rename a speaker once and have every segment of that speaker in the meeting follow, so that labeling takes seconds.
32. As a user, I want to hear the three longest clips of a speaker while renaming, so that I remember who it was.
33. As a user, I want to play any segment from the transcript, so that I can check what was actually said.
34. As a user, I want Myna to remember a named speaker's voice and suggest the name in later meetings, marked as suggested until I confirm, so that regulars label themselves.
35. As a user, I want to merge two speakers the diarizer split, so that over-segmentation is fixable.
36. As a user, I want speaker names from Meet or Teams captions, when captions were on, applied as suggestions, so that the labels are right without my input.
37. As a user, I want the screenshot filmstrip on the timeline, with click-to-seek, so that I can jump to a slide.
38. As a user, I want to export a meeting as Markdown or plain text, so that I can paste it anywhere.
39. As a user, I want to pick the diarization model from a curated list, so that I can swap if one misbehaves.

### Meetings: AI
40. As a user, I want to configure one AI provider (OpenAI, Anthropic, OpenRouter, or any OpenAI-compatible local server) with my own key, so that I decide where text goes.
41. As a user, I want API keys stored in the macOS Keychain, so that they are not sitting in a config file.
42. As a user, I want a clear "sends text off this Mac" badge next to any feature that does, so that I am never surprised.
43. As a user, I want a Summary tab with summary, decisions, and action items, so that the meeting has a one-screen recap.
44. As a user, I want to regenerate with a custom prompt and save it as a template, so that my recap format is mine.
45. As a user, I want summaries stored, so that regenerating is a choice, not a cost on every open.
46. As a user, I want a chat per meeting with the transcript and summary as context, so that I can ask follow-up questions.
47. As a user, I want to see approximate tokens and cost for the chat so far, so that I know what I am spending.
48. As a user, I want a warning when the context exceeds my model's window, so that I understand why answers degrade.
49. As a user, I want price and context window taken from the provider or a maintained price list, never typed by me, and "cost unknown" when no price is known, so that the cost shown is never wrong.

### Experimental
50. As a user, I want an Experimental section that is off by default, so that stable features stay stable.
51. As a user, I want Uhm filler detection toggled separately for Dictation and Meetings, so that I can try it on one.
52. As a user, I want LLM cleanup toggled separately for Dictation and Meetings, so that meetings stay verbatim while dictation gets polished.
53. As a user, I want auto-triage of possibly-muted segments through a decision model, text only, so that relevant speech is included without my clicking.
54. As a user, I want a decision provider with presets (Laya local, Jev, OpenAI Decisions, custom), so that triage uses the model I trust.
55. As a user, I want an in-app "Set up Laya" wizard that installs it into an app-owned environment, so that local triage needs no terminal.
56. As a user, I want screenshot relevance filtering by a vision model, so that only informative frames are kept.
57. As a user, I want kept screenshots included in summarization and chat when my model supports images, so that visual content informs the recap.

### Folders
62. As a user, I want to create folders in the Meetings sidebar and drag meetings into them, so that recurring contexts (a client, a team, 1:1s) stay together.
63. As a user, I want a new meeting to show a suggested folder from my AI provider, with one click to accept, so that filing takes no thought.
64. As a user, I want unfiled meetings in an Inbox, so that nothing gets lost while I decide.

### Data
58. As a user, I want everything stored under Application Support with SQLite for metadata, so that backup is one folder.
59. As a user, I want "delete audio and video after N days" with transcripts kept forever, so that disk use is bounded.
60. As a user, I want per-meeting delete of media, so that I control sensitive recordings.
61. As a user, I want model downloads verified by checksum, so that a corrupt download fails loudly.

## Implementation Decisions

### Platform and stack
- macOS only, Apple Silicon only, macOS 14.2 or newer. Direct download, notarized, unsandboxed. MIT license.
- Rust core with Tauri 2 UI. One Swift static library linked via swift-rs exposes Apple-only runtimes behind C functions. Inference speed is set by Core ML, ONNX Runtime, or Metal, not the host language, so Rust versus Swift is not a performance decision.
- Each model task is a Rust trait with pluggable backends: `Transcriber`, `Diarizer`, `FillerDetector`, `TextProvider` (chat LLM), `DecisionProvider`. Runtime choice is a backend, never an architecture change.
- Menubar app via Tauri tray. Global hotkey via tauri-plugin-global-shortcut with pressed/released state; hotkey plugin fallback if hold-to-talk proves unreliable. Paste via arboard plus synthetic Cmd-V, clipboard restored, configurable delay; enigo-style keystroke fallback.
- Microphone via cpal. System audio via Core Audio process taps using the coreaudio crate, per-process. ScreenCaptureKit is the fallback for system audio and the primary path for screenshots and window video. Video encoding lives in the Swift library (AVAssetWriter).
- Auto-update via the Tauri updater. Models download from Hugging Face with checksum verification into the app's models directory.

### Model lineup (phase 1)
- Speech-to-text default: NVIDIA Parakeet TDT 0.6B v3 via the parakeet-rs crate (ONNX Runtime). Curated alternatives: Whisper large-v3-turbo via whisper-rs; Desert Ant Voz via the Swift bridge as the Neural Engine option. Advanced: custom path or Hugging Face repo for the same runtime.
- Diarization default: Desert Ant Who via the Swift bridge. Open fallback: NVIDIA Sortformer via parakeet-rs. The mic track is always the known "me" speaker; diarization only runs on the system track.
- Filler detection: Desert Ant Uhm via the Swift bridge, experimental only. Rules always run regardless.
- Desert Ant policy: their models are used only through their Swift SDK, never by loading their ONNX from Rust. Every Desert Ant slot has an open fallback. Open backends are built first. The "Powered by Desert Ant Labs" attribution appears in Settings only while at least one Desert Ant model is enabled. Model weights are downloaded at runtime and never committed to the repo.
- No local LLM ships in phase 1. Text provider is one OpenAI chat-completions client; presets fill base URL (Anthropic via its OpenAI-compatible endpoint).
- Decision provider has two wire adapters: Jev-format (Jev hosted, Laya local, custom) and OpenAI Decisions format. Laya runs as a managed Python sidecar: app-owned uv environment, multilingual checkpoint (1,024-token context), `laya-serve` as a child process bound to localhost with a generated key, started on demand, stopped on quit.

### Pipelines
- Dictation: hotkey down → capture mic → hotkey up → [Uhm on audio, experimental] → STT → rules (filler list, repeated-word collapse, dictionary) → [LLM cleanup, experimental, timeout ~3 s falls back to rules output] → paste.
- Meeting post-process: [Uhm] → STT on both tracks → diarize system track → assign mute state per mic segment → caption-name suggestion (if extension events exist) → voice-profile suggestion → [LLM cleanup, experimental] → store.
- Mute state per mic segment: `unmuted`, `muted`, `unknown`. Only `unmuted` appears by default. Myna's own mute toggle sets `muted`. Mirrored state from the extension sets `unmuted`/`muted`. No source means `unknown`. Mic audio is always recorded to disk; the state gates inclusion, not capture. Nothing collapsed is sent to any provider unless the user includes it or experimental triage is on.
- Experimental triage: for each collapsed segment, send segment text plus the two neighboring included lines to the decision provider with a yes/no question; include on yes. Text only, never audio. Summarization system prompt tells the model that collapsed segments may be self-talk.
- Screenshots: ScreenCaptureKit capture of the chosen window every 15 s, JPEG at 1280 px width, perceptual-hash compared with the last kept frame, near-duplicates dropped. Experimental: vision-model relevance filter on previous-kept plus current. Experimental: up to 20 downscaled kept screenshots attached to summarization and chat when the model's "supports images" box is checked.
- Window video is never sent to a provider.

### Companion extension
- One extension for Chromium browsers (Chrome, Arc, Brave, Edge) targeting meet.google.com and the Teams web domains. It relays mute-button state and, when captions are on, "name X spoke at time T" events to the app over localhost. Selectors are expected to churn; the extension reports "captions unavailable" and "mute unknown" rather than guessing, and the selector list is updatable without an app release.
- Zoom native mute mirroring via Accessibility is phase 2. Safari, native Teams, Gather, and Slack huddles are `unknown` in phase 1.

### Speaker identity
- Rename propagates to the diarization cluster within the meeting. Saving stores the voice embedding (Who voice-print or Sortformer embedding). Later matches show as "suggested" until confirmed or dismissed. Merge two speakers is phase 1; split is phase 2.
- Caption-derived names: each cluster is auto-suggested the caption speaker with the greatest time overlap.

### AI provider and chat
- One provider config: preset, base URL, model name, Keychain key. Price and context window come from the provider's model list where it has one (OpenRouter), else from a bundled price list refreshed from the repo; unknown means "cost unknown", local means free. Per-feature on/off toggles consume the single config.
- Three built-in prompt templates (Summary, Action items, Decisions). Regenerate opens a prompt editor prefilled with the last template, with save-as-template. Templates are global.
- Chat: one persisted thread per meeting; every turn sends transcript + summary + history. Token estimate is characters ÷ 4; cost from the resolved price, unknown price means tokens only.

### Folders
- Flat folders (no nesting in phase 1) in the Meetings sidebar; a meeting belongs to at most one folder; unfiled meetings show in Inbox. Create, rename, delete (deleting moves meetings back to Inbox), drag-and-drop and a "Move to" menu.
- Folder suggestion: after transcription, when the AI provider is configured and the "suggest folder" toggle is on, send folder names plus the meeting title and the first lines of the transcript to the provider and ask it to pick one or "none". The result shows as a suggestion chip on the meeting until accepted or dismissed. Carries the off-device pill. If the provider is off, no suggestion.

### Storage
- `~/Library/Application Support/Myna/`: SQLite for meetings, segments, speakers, voice profiles, templates, chats, settings; `audio/`, `video/`, `screenshots/`, `models/` on disk. Retention setting deletes audio, video, and screenshots after N days; transcripts, summaries, and chats are never auto-deleted.

### Layout rules (from the prototype)
- Transcript is a document, not chat bubbles: speaker and time sit in a left margin, prose in the content column, inspector on the right.
- Mic state is always text plus a dot, never color alone: "Mic live · mirrored from Meet" or "Mic off · manual". The menubar popover shows which mode is active.
- Possibly-muted segments render inline as a collapsible group between normal rows, each with Play and Include. Included segments keep a marker.
- Rename popover: name field, "remember this voice" choice, the three longest clips with play buttons, and actions "Merge into…" and "Rename N segments".
- Any feature that sends data off the Mac carries a visible "sends text off this Mac" pill (or "sends images off this Mac") where its toggle is and on any generated output.
- Experimental rows use a Dictation / Meetings switch pair.
- Model rows: radio, name, vendor, one description line, and a trailing size and state (Installed, progress with Cancel, or Download).
- Folders sit in the Meetings sidebar under Dictation history. A meeting's detail view has a "More" menu with "Move to" listing Inbox and folders, with the current folder checked.
- Dark theme uses an elevation ladder (desk, inset, window, sidebar, raised) with visible hairlines; accent text and accent fill are separate tokens so both meet contrast.
- Heading levels run in sequence on every screen.
- The menubar popover looks and behaves like a native macOS menubar menu (Control Center, Wi-Fi): system material, menu metrics, system colours, accent highlight.
- Status pills follow one hierarchy (status, neutral, alert) at one size everywhere.
- A screen whose data fails to load shows the error. No screen ever renders blank.

### Settings layout
- Modules (enable Dictation / Meetings), Dictation (hotkey, mode, dictionary, paste method), Meetings (capture mode, interval, per-app mic policy, retention), Models (STT, diarization, with download state), AI provider, Decision provider, Experimental (Uhm × module, LLM cleanup × module, muted-segment triage, screenshot relevance, screenshots in AI), About (attribution, version).

## Testing Decisions

- A good test drives the system from the outside with fixture audio and checks user-visible output. It does not assert on internal struct shapes or model internals.
- Highest seam: the Rust command layer that Tauri exposes to the UI. Every feature is testable there with no WebView: "given this WAV and these settings, the dictation command returns this text"; "given these two tracks, this mute timeline, and these caption events, the meeting command yields these segments with these states and names".
- Model backends are tested behind a `Transcriber`/`Diarizer` fake that returns scripted output, so pipeline tests run in milliseconds without model downloads. One slow integration test per real backend runs the smallest model on a 10-second fixture and asserts a loose word-error threshold.
- Rules cleanup, mute-state assignment, caption-overlap naming, perceptual-hash dedupe, and token/cost estimation are pure functions and get table-driven unit tests.
- Providers (text, decision) are tested against a local HTTP stub that records requests; one test asserts the off-device badge state matches whether any request would leave the machine.
- Paste, hotkey, Core Audio taps, ScreenCaptureKit, and the Swift bridge are verified by a manual checklist per release, not automated.
- No prior art: greenfield repo.

## Out of Scope (phase 2 and later)

- Meeting auto-detect ("you seem to be in a call, record?").
- Live transcript during a meeting.
- Zoom native mute mirroring; Safari, native Teams, Gather, Slack mirroring.
- Speaker split.
- Shipped local LLM (llama.cpp in-process).
- Native Rust port of Laya.
- Calendar integration.
- Active-speaker detection from screen pixels.
- Cloud "public on demand" beyond user-supplied keys.
- LLM router: a second "cheap model" slot so small tasks (folder suggestion, titles, cleanup) go to a fast cheap model while summarization and chat go to a stronger one. Phase 1 uses the single provider for everything.
- Nested folders.
- Mac App Store distribution. Intel Macs. Windows, Linux.
- Multilingual UI, rules, and dictionary (models may be multilingual; the app is English-first).

## Further Notes

- Desert Ant Who is beta (weights v0.3.4). If it proves unstable the default flips to Sortformer with no architecture change.
- Laya's zero-shot accuracy is weak on large option sets. Its use stays yes/no.
- The companion extension's selectors are the most fragile piece; budget for a quick update path from day one.
- The fail-safe mute default means users of unsupported meeting apps see their own speech collapsed until they include it or set "always record mic". This is deliberate.
- A throwaway HTML prototype lives in `.scratch/myna/prototype/` for look and feel only.

### Lessons from the first build (deleted 30 Oct 2026)
The first build passed 224 unit tests and still failed the user's first real run:
- Settings pages (Models, Experimental, Meetings) rendered blank on a fresh install; the mocked UI harness hid it.
- Onboarding saved Meetings as off, so Start meeting never appeared, and it never got the user to a downloaded model.
- Cost needed typed prices, which go stale.
- The popover did not feel native; pills had no consistent hierarchy.

So each story's "done" includes a check on the real app from a fresh install, not just tests and a mocked UI.
