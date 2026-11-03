# Handoff: story 03 (Models), next is ticket 3 (runtimes)

Repo `/Users/dhruv@shurutech.com/Dev/dhruvsaxena1998/myna`, branch `feat/03-models`. Read `AGENTS.md` first, then `.scratch/myna/stories/03-models.md`.

## Done (commit f15fcb8, not pushed)
Tickets 03.1 and 03.2, and the user checked them in the real app: catalog, active model per task, download with progress/cancel/retry, sha256 check, Remove.

- Catalog: `models/local.json` (embedded with `include_str!`). `src-tauri/src/models.rs` lists it, reports installed (every file present at catalog size), saves the active model in SQLite (`active_model`, migration 2 in `storage.rs`).
- Downloads: `src-tauri/src/download.rs` (ureq + sha2). Temp folder `models/.downloading-<id>-<nonce>/`, size + sha256 per file, then rename to `models/<id>/`. Commands `model_download`, `model_cancel` (answers at once, thread winds down on its own), `model_delete` in `lib.rs`. Backend emits `model-state` events `{id, state: progress|done|canceled|failed, pct?, error?}`.
- UI: `src/lib/ModelTask.svelte`, shared download state in `src/lib/downloads.svelte.ts` (so progress survives switching Settings pages).
- Voz and Who have `files: []` and show "Coming soon" until ticket 03.4.
- Models folder: `~/Library/Application Support/Myna/models/<id>/`.

## Next: ticket 3
`Transcriber` / `Diarizer` behind a fake; Parakeet and Sortformer via parakeet-rs, Whisper via whisper-rs; one slow test per backend on a 10 s fixture. Nothing for this exists yet. Check parakeet-rs and whisper-rs APIs and build requirements (Metal/Core ML features, cmake) before choosing the trait shape. The files each model needs are in `models/local.json` (Parakeet: encoder, decoder_joint, vocab; Sortformer: one onnx; Whisper: one ggml bin).

## Rules from the user (memory)
- One story at a time; the user reviews each ticket. No autonomous multi-ticket loops.
- Never launch, click or screenshot the app unasked. Test with unit tests and Playwright (`pnpm test:ui`, UI against a faked Tauri in `tests/tauri.ts`). When asked to run it: `pnpm tauri dev` fails if port 1420 is taken.
- Build to `docs/index.html`, `docs/flow.html`, `DESIGN.md`. No jitter: reserve space, load before paint.
- No hacky workarounds. Don't patch user data to demo.
- Push a feature's tickets to GitHub only when it's being built.

## Checks
`cd src-tauri && cargo test` (11 pass), `pnpm check` (clean), `pnpm test:ui` (13 pass).

## Known loose ends
- Downloads have no resume and no read timeout (marked `shortcut:` in `download.rs`).
- Removing the active model leaves it selected and shows Download again; unconfirmed whether that is right.
- Remove button is not in the design (`docs/index.html`); added because the ticket requires delete.
- The checksum-failure path is only tested against a local test server, never a real bad download.
- Nothing is pushed and no PR exists.
