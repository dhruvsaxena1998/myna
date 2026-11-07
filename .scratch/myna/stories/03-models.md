# 03: Model catalog and downloads

**Status:** ready
**GitHub:** milestone "03 · Models"
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

## Tickets
1. Catalog and model rows on AI & privacy: bundled catalog (Parakeet, Whisper, Sortformer; Voz and Who listed), size/accuracy/speed, Installed or Download, active model per task saved in SQLite
2. Download manager: Hugging Face download with progress, cancel, retry, sha256 check, atomic move into `models/`, delete
3. Runtimes: `Transcriber` / `Diarizer` behind a fake; Parakeet and Sortformer via parakeet-rs, Whisper via whisper-rs; one slow test per backend on a 10 s fixture
4. Desert Ant via the Swift bridge: Voz and Who through swift-rs (macOS 15+), their SDK fetches weights; attribution on About while one is enabled
5. Advanced: custom model path or Hugging Face repo for the same runtime
6. Meeting speech-to-text on an Endpoint (Groq, Together, Test for others), blocked by stories 05 and 10
7. Fresh-install check (human)
8. Apple Speech: Apple's built-in speech-to-text as a speech-to-text model on macOS 26+, so a user can dictate without downloading Parakeet (detail below)
9. ONNX Runtime on demand: ship without it, download it with the first model that needs it (detail below)
10. Whisper small, medium and large-v3 (#124); 11. Parakeet Unified English (#125)
12. Find a model in a long list: speech-to-text models grouped by provider, Sort by (Recommended, Accuracy, Speed, Size) inside each provider, fuzzy search on the name; the chosen model never searches out of sight (map: `.scratch/model-list/map.md`)

## 03.8 Apple Speech

Settled with the user on 2026-11-03. Spike on macOS 27 (headless CLI, `say` audio plus `two-speakers.wav`): no Speech Recognition prompt, fillers and stutters kept ("um", "uh", "we should, we should"), the fixture word for word with punctuation, 9 s of audio in 0.16 s, 45 languages, English variants preinstalled.

- **API:** SpeechTranscriber (SpeechAnalyzer), macOS 26+. Below 26 the row shows "Needs macOS 26", like Voz. No SFSpeechRecognizer fallback.
- **Bridge:** this ticket adds the swift-rs Swift package (the spec's Swift bridge; 03.4 reuses it). `@_cdecl` functions: transcribe 16 kHz f32 to segments with times; the language and whether it's installed; download its assets. The deployment target stays 14.2 behind `#available(macOS 26, *)`.
- **Row:** "Apple Speech", vendor Apple, "Built into macOS. Listens in <language>." from the Dictation language (Language you speak on Voice typing, pre-filled from the Mac's language; `supportedLocale(equivalentTo:)`), per the [rules cleanup map](../../rules-cleanup/issues/06-non-english.md). No size. Installed when the language is in `installedLocales` (not `AssetInventory.status`, which says "supported" for preinstalled English). Otherwise Download through `AssetInventory`, with progress and no Cancel (unverified: English is preinstalled on the dev Mac). No Remove: macOS owns the assets. An unsupported language shows "Needs a language Apple supports" and can't be picked.
- **Ratings:** placeholder Accuracy 4, Speed 5. A prototype measures them against Parakeet later.
- **Active model:** the saved pick if it runs here; else Parakeet if installed; else Apple Speech if it runs here; else Parakeet ("Download Parakeet…"). Parakeet stays the recommended model, and downloading it takes over from an Apple Speech that was never picked explicitly.
- **Shared:** one speech-to-text slot for Dictation and Meetings, as today.
- **Permissions:** no Speech Recognition row and no Info.plist key. If macOS refuses anyway, its message shows on the pill.
- **Not here:** Words to remember (SpeechTranscriber has no documented vocabulary option; rules cleanup respells after it, story 04 ticket 5), the language picker (story 04 ticket 10).
- **Tests:** active-model rule and catalog in `models.rs`; Apple Speech row states in Playwright; one slow test on `two-speakers.wav` (macOS 26+).

## Open questions for you
- none

## 03.9 ONNX Runtime on demand

ONNX Runtime is 12.6 MiB of the 27 MiB app binary (cargo-bloat on 0.0.1-beta1), and only Parakeet and Sortformer use it. Someone on Whisper or Apple Speech should never get it.

- **Build:** parakeet-rs without default features, with `load-dynamic` (plus `cpu`, `api-28`, `sortformer`), so `ort` loads `libonnxruntime.dylib` at runtime instead of linking it.
- **Download:** the runtime is a download dependency of the models that need it. Downloading Parakeet or Sortformer fetches it first (once, shared) through the existing download manager: progress, cancel, retry, sha256, atomic move into the app data folder. The row shows one combined size.
- **Version:** pinned to the ONNX Runtime release `ort` 2.0.0-rc.15 expects, from Microsoft's GitHub release for macOS arm64, sha256 in the catalog.
- **Load:** `ort::init_from(path)` once, before the first Parakeet or Sortformer session. Missing or broken runtime is a clear error on the model row ("Runtime missing, Download again"), never a crash.
- **Delete:** the runtime goes when the last model that needs it is deleted.
- **Signing:** a downloaded dylib loaded into a hardened, notarized app needs library validation off (`com.apple.security.cs.disable-library-validation`) or Microsoft's signature to pass. Check this before story 14 signs the app.

Done when: a build without Parakeet has no ONNX Runtime in it (binary about 12 MiB smaller); downloading Parakeet fetches the runtime and dictation works; Whisper-only use never downloads it; command-layer test with a fake download covers "first ONNX model fetches the runtime, second doesn't, deleting the last one removes it".

