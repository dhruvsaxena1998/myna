# 03: Model catalog and downloads

**Status:** draft
**Spec stories:** 13, 14, 39, 61
**Depends on:** 01

One place to see, download, verify, pick and delete the on-device models for each task.

## Stories
- As a user, I want a curated list per task (speech-to-text, diarization) with size, speed and quality labels.
- As a user, I want downloads with progress, cancel and retry, verified by checksum, so that a corrupt download fails loudly.
- As a user, I want to pick the active model per task and delete ones I don't use.
- As an advanced user, I want a custom model path or Hugging Face repo for the same runtime.

## Done when
- [ ] Models page lists the catalog on a fresh install with an empty models folder
- [ ] Download, cancel, retry, delete; checksum failure shown
- [ ] Active model per task persists
- [ ] Checked on the real app from a fresh install

## Open questions for you
- Default speech model: Parakeet (ONNX) as in the spec?
- Do custom models ship in the first cut or later?
