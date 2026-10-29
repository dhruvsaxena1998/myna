# Product

<!-- impeccable:product-schema 1 -->

All facts below were inferred from the planning conversation (grilling rounds Q1–Q52), not from a live interview; the init ran inside a subagent with no answer mechanism. Items marked (assumed) are the subagent's guesses.

## Platform

web

(The shipped app is a Rust + Tauri macOS desktop app rendering HTML in a WebView. The design language is native macOS, not web; the `web` value only records that the UI is built from HTML/CSS.)

## Stack

static HTML/CSS for the throwaway prototype (user-specified: "basic HTML, just to feel it"). The real app is Rust + Tauri; the prototype does not share code with it.

## Users

A single Mac user, typically a developer or knowledge worker, who (a) dictates text into whatever app has focus by holding a hotkey, and (b) records their own video calls (Google Meet and Teams first, then Zoom) to get a speaker-labeled transcript, summary, and chat afterward. Privacy-sensitive: they have been burned by tools that kept recording the mic while they were muted in the meeting app.

## Product Purpose

Myna is a fully local macOS app with two opt-in modules, Dictation and Meetings. Everything (speech-to-text, diarization, filler removal) runs on-device by default. Cloud providers exist only as bring-your-own-key, off-by-default options. Success: dictated text appears in the focused app within a second of releasing the hotkey; a recorded meeting yields a transcript where "you" and each other speaker are separated, nothing said while muted leaks, and a summary or chat is one click away.

## Positioning

Mute-aware capture: the mic track is always recorded locally, but segments are tagged unmuted / muted / unknown from the meeting app's own state, and only unmuted segments enter the transcript or leave the machine. Plus swappable models per task (STT, diarization, LLM, decision model) rather than lock-in to one vendor.

## Operating Context

- macOS 14.2+, Apple Silicon only. Lives in the menubar; a main window for Meetings and Settings; a floating pill during dictation.
- Permissions needed at onboarding: Microphone, Accessibility (paste), Screen & System Audio Recording.
- Meetings capture mic and system audio as separate tracks, optional window screenshots (default every 15 s) or window video.
- A companion browser extension relays Meet/Teams mute state and caption speaker names.
- Models download lazily per enabled module from Hugging Face.

## Capabilities and Constraints

- Dictation: hold-to-talk (toggle mode optional), finalize on release, clipboard paste with restore, rules-based filler removal and personal dictionary. Experimental: Uhm audio filler detection, LLM cleanup (per-module toggles).
- Meetings: manual start/stop; post-process transcription; diarization with rename-once propagation, remembered voice profiles ("suggested" matches), merge speakers, replayable clips while renaming; collapsed "possibly muted" segment groups with per-segment include; screenshot filmstrip timeline; Summary tab with built-in and saved prompt templates; Chat tab with approximate token and cost counter.
- Settings: Models (curated STT list: Parakeet TDT v3 default, Whisper large-v3-turbo, Desert Ant Voz; diarization: Desert Ant Who default, Sortformer fallback; advanced custom path), AI provider (OpenAI / Anthropic / OpenRouter / Custom OpenAI-compatible, Keychain key, per-model price table, context window), Decision provider (Laya local via managed sidecar, Jev, OpenAI Decisions, Custom), Experimental section.
- Terminology: "Dictation" and "Meetings" are the module names. "Possibly muted" is the user-facing label for muted/unknown segments. "Suggested" marks an auto-applied speaker name awaiting confirmation.
- Any feature that sends text off the Mac shows a visible "sends text off this Mac" badge.
- Attribution "Powered by Desert Ant Labs" is required and lives on the Models/About page.
- Undecided: app icon, exact hotkey default (assumed Fn or Right Option).

## Brand Commitments

Name: Myna (the bird that mimics speech). Open source, MIT. Voice (assumed): plain, short, no marketing adjectives. Binding visual constraint from the user: minimal and aesthetic, native macOS feel; explicitly not "AI slop" (no gradient blobs, purple-on-dark, emoji icons, rounded card grids). References named by the user's brief: Things, Linear, Craft.

## Evidence on Hand

No logo, screenshots, testimonials, or benchmarks of our own. Model benchmark numbers are vendor-reported and must not be quoted as ours. Transcript and meeting content in the prototype is synthetic and must be labeled as such.

## Product Principles

- Local by default; off-device is opt-in and visibly labeled.
- Never lose audio, never leak audio: capture everything locally, gate inclusion by mute state.
- The tool disappears into the task; familiar macOS patterns over invention.
- Every model slot is swappable; no single vendor is load-bearing.
- Experimental means off by default and per-module.

## Accessibility & Inclusion

Standard macOS expectations: keyboard-operable, visible focus, system light/dark, contrast ≥ 4.5:1 for text. The mic-live indicator must not rely on color alone.
