# Speech-to-text through an Endpoint

Type: grilling
Status: resolved
Blocked by: 05, 09

## Question

How does a user point speech-to-text at an Endpoint: which presets, how chunked post-meeting audio is sent, how Dictation behaves (local only, or allowed with a timeout), and how the off-device pill shows? Cloud STT is never used for realtime meeting transcription.

## Notes from grilling

- Round 1: Q1 refined by the user: Dictation never uses a cloud STT Endpoint, and gets two hotkeys, one plain and one that adds LLM cleanup, so cleanup can be skipped per dictation even when set up. Q2–Q7 accepted as recommended.
- Round 2 (Q8–Q10, from the two hotkeys): all accepted. The second hotkey replaces ticket 28's dictation cleanup toggle.

## Answer

- Dictation is always local, with two hotkeys sharing mode, dictionary and paste method. **Dictate**: Uhm (if on) → local STT → rules → paste. **Dictate with cleanup**: the same, then LLM cleanup (ticket 28's 3 s budget, guards and Styles) → paste. The cleanup hotkey starts unassigned and shows "Needs an Endpoint" until dictation cleanup has one. Styles apply only to it. This replaces ticket 28's dictation cleanup toggle.
- Only the post-meeting pass can use an Endpoint: "Meetings speech-to-text: Local model / Endpoint" on the Models page, reusing Endpoint records and keys. Never realtime.
- Presets: Groq and Together. Any other Endpoint, local ones included, must pass a Test (a bundled 5 s clip; the reply must carry word timestamps) before it can be picked. Models come from the Endpoint's `/v1/models`. OpenAI is left out until it has a timestamped model that stays supported (`gpt-4o-*` return none, `whisper-1` ends 2027-02-26). Diarization, Uhm and everything after STT stay on the Mac.
- Chunks: each track separately, up to 10 min cut at silence, 16 kHz mono FLAC (about 20 MB, under every cap), 3 in parallel, timestamps shifted by chunk offset. Same chunks as the chunked-transcription wayfinder.
- Possibly-muted mic spans are cut before upload and transcribed locally; they stay collapsed, and including one later never re-sends it.
- Failure: 3 retries with backoff per chunk; then "Transcription incomplete (n of m)" with **Retry** and **Transcribe these locally**, keeping the chunks that succeeded. No silent fallback. A broken or deleted Endpoint shows "Not set" and holds new meetings.
- Off-device pill on the selector and the transcription progress row; the meeting inspector records the Endpoint and model that transcribed it. STT cost uses LiteLLM's per-minute price, else "cost unknown" (Together has no source), and adds to the meeting total. Local Endpoints show no pill and no price.
- The personal dictionary and speaker names go as `prompt` where the Endpoint accepts one, trimmed to fit; rules still run afterwards.
