# 03: Model catalog and downloads

**Status:** draft
**Spec stories:** 13, 14, 39, 61, 65
**Depends on:** 01 (the Endpoint option for meeting speech-to-text needs 10)

One place to see, download, verify, pick and delete the on-device models for each task, and to choose where meeting speech-to-text runs. LLMs are not here: Myna runs none (story 10).

## Stories
- As a user, I want a curated list per task (speech-to-text: Parakeet, Voz, Whisper; diarization: Who, Sortformer) with size, speed and quality labels.
- As a user, I want downloads with progress, cancel and retry, verified by checksum, so that a corrupt download fails loudly.
- As a user, I want to pick the active model per task and delete ones I don't use.
- As an advanced user, I want a custom model path or Hugging Face repo for the same runtime.
- As a user, I want meeting speech-to-text to run locally by default, or on an Endpoint I choose after the meeting (never live, never for Dictation).

## Decisions (detail in the linked tickets)
- Default speech-to-text is Parakeet TDT 0.6B v3 (ONNX via parakeet-rs), as in the spec. Voz is the same weights on Core ML (macOS 15+): [Uhm, Voz and cloud STT endpoints](../../local-ai/issues/05-stt-models-and-endpoints.md).
- Custom path or Hugging Face repo ships in Phase 1 under Advanced, as in the spec.
- "Meetings speech-to-text: Local model / Endpoint": Groq and Together presets, any other Endpoint must pass a word-timestamp Test; 10-minute FLAC chunks per track, possibly-muted mic spans never uploaded, failed chunks offer Retry or Transcribe locally, off-device pill and per-minute cost: [Speech-to-text through an Endpoint](../../local-ai/issues/13-cloud-stt-endpoint.md).

## Done when
- [ ] Models page lists the catalog on a fresh install with an empty models folder
- [ ] Download, cancel, retry, delete; checksum failure shown
- [ ] Active model per task persists
- [ ] Meeting speech-to-text on a Groq Endpoint transcribes a real meeting, with the pill shown and the Endpoint recorded in the meeting inspector
- [ ] Checked on the real app from a fresh install

## Open questions for you
- none
