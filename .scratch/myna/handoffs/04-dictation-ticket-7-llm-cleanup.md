# Handoff: LLM cleanup on Endpoints ("Dictate with cleanup", 04.7)

Repo `dhruvsaxena1998/myna`. Start a fresh worktree from `origin/main` (at `6652393` or later) and run `scripts/worktree-setup.sh`. Read `AGENTS.md`, then the sources below. GitHub: milestone "04 · Dictation", story #47, ticket **04.7 is #54** (open). The AI side is story 10, milestone "10 · AI providers and models", story #108.

## Where things stand (2026-11-06)
- **Story 10 built so far (merged):**
  - 10.1 Design (#107).
  - 10.2 Endpoints (#113).
  - 10.3 Decision provider (#111) and 10.4 the fresh-install check (#112) are not started. LLM cleanup doesn't need either.
- **Also merged:** the Liquid Glass opacity slider and capsule dropdowns (#114), and the updater feed release created as a pre-release (#115).
- **Releases:** the last is `v0.0.2-beta.6`. The user wants the next release only after LLM cleanup ships. Don't tag until they say so.
- **LLM cleanup here means dictation cleanup.** Transcript cleanup is story 16 and needs meeting transcripts (story 05), which don't exist yet.

## What's decided (don't re-grill)
Sources:
- `.scratch/local-ai/issues/28-llm-cleanup.md` (the Answer section and its amendments)
- `.scratch/local-ai/issues/13-cloud-stt-endpoint.md` (the two hotkeys)
- `.scratch/myna/stories/04-dictation.md` (ticket 7)
- `.scratch/myna/stories/16-summary-and-cleanup.md` (the digits and spelling lines in the prompt)
- `.scratch/myna/stories/10-ai-providers.md`

Decisions:
- **Two hotkeys.** They share mode, Words to remember and paste method.
  - **Dictate** (existing): STT → rules → paste.
  - **Dictate with cleanup** (new): STT → rules → **LLM cleanup** → paste. It starts unassigned and shows "Needs an Endpoint" until the Fast model is set. The plain hotkey never runs cleanup.
- **Route.** The Fast model through `ai::route(db, "fast")`. Agents are deferred, so only Endpoints exist.
- **Budget.** 3 s from key release, separate from STT and rules. On a timeout or an error, paste the rules output and briefly mark "pasted without cleanup".
- **What cleanup does:** fixes punctuation, capitals, grammar slips, missed fillers and self-corrections ("Tuesday, no wait, Wednesday" → "Wednesday"). Dates, times, phone numbers, versions and decades go in digits. It never rephrases, shortens or replies.
- **Prompt.** One line, "Spell these exactly as written: …", lists Words to remember (the alias's correct form). Styles don't change that line. Dictated text goes inside data tags, never as instructions.
- **Guards.** Fall back to the rules output if the result reads like a reply ("Sure, here's…", an answer to a dictated question). With the default prompt, also fall back if the length changes by more than about 40% or most words are new.
- **Editable prompt.** Only the instruction text is editable, inside the fixed wrapper (data tags, dictionary). It has "Reset to default". If the user has edited it and Myna ships a new default, keep their edit and show "A newer default is available".
- **Styles.** Named prompts bound to apps, picked by the frontmost app at hotkey press, Default otherwise. Myna ships Default, Code, Message and Email; the user can edit them, add more and reassign apps. Matching is by app only, and the app name is never sent. Styles apply only to the cleanup hotkey.
- **Where it lives.** Settings › Voice typing, not Experimental. Off until set up, with the "Sends text off this Mac" pill when the Fast model is off-device.

## Suggested tickets (propose these to the user; they decide)
1. **Design first, as with 10.1.** `docs/index.html` still shows the old "Tidy up with AI" rows in Experimental (`row('Tidy up with AI', …)`, ~line 1893). Remove them, and draw on Voice typing: the second shortcut row ("Needs an Endpoint" state), the off-device pill, the prompt editor with Reset, and Styles. Use the prototype skill and stop for review.
2. **Core.** The second hotkey and the cleanup step with the default prompt: 3 s budget, guards, fallback with the "pasted without cleanup" mark, and a `cleanup` step in the trace (text, ms, model, cost) so Stats for nerds shows it. Fake `Backend` plus a local HTTP stub for tests; no real calls in tests.
3. **The editable prompt and Styles.**

## Code pointers
- **Dictation pipeline:** `src-tauri/src/session.rs`, the `run` closure (~L240–265). It runs STT → `rules::edits` → `s.world.paste`, and pushes `Step`s into the trace. Cleanup goes between the rules step and `paste`.
- **`Session::new`** (`session.rs` ~L85) has the DB but no Keychain. Pass `Arc<dyn ai::Keys>` in. `Data.keys` in `lib.rs` is the real one (`ai::Keychain`).
- **Hotkeys:**
  - `Session::key(pressed)` is called from the global-shortcut handler in `lib.rs` `setup`; shortcut registration is in `lib.rs` (~L306, L455).
  - The fn key path is `src-tauri/src/fnkey.rs`.
  - Voice settings are in `voice.rs`.
  - A second hotkey needs `key` to know which one fired.
- **AI** (`src-tauri/src/ai.rs`):
  - `route(db, "fast")` returns `(Record, model)`, or an error: "Fast model: Not set" or "… is no longer offered by …".
  - `provider(db, keys, id)` returns a `Box<dyn Backend>`. `Backend::chat(model, prompt)` returns a `Reply { text, tokens, cost }`. `off_device(url)` tells whether a URL leaves this Mac.
  - Two gaps for cleanup: `chat` sends one user message, so the wrapper may want system + user messages; and its timeout is a fixed 60 s, so cleanup needs a 3 s one. Extend `chat` minimally; `Fake` lives in `ai.rs` tests.
- **Words to remember:** `rules::load` and `rules::prompt` in `rules.rs`.
- **Traces:** `trace.rs`; the UI is Stats for nerds (`src/lib/Traces.svelte`, `Overview.svelte`).
- **Settings UI:** `src/lib/VoiceShortcut.svelte` (shortcut rows) and `src/routes/settings/+page.svelte`. Fake backend for Playwright: `tests/tauri.ts`.

## Checks
- `cd src-tauri && mise exec cmake@4.4.5 -- cargo test`: 98 pass, 4 ignored. Clippy has two old warnings, in `download.rs` and `rules.rs`.
- `pnpm run check` must report 0 errors and 0 warnings. `pnpm run test:ui`: 152 pass. Use `pnpm run <script>`; `pnpm -s` isn't accepted here.
- Run the app with `mise exec cmake@4.4.5 -- pnpm tauri dev` from the worktree (port 1420 must be free). It's ready when the log shows ``Running `target/debug/myna` ``; the log has ANSI codes, so don't anchor the match.
- For a real check: `OPENROUTER_API_KEY` is in `.env.local` (temporary key, $5 cap, expires 2026-11-08). `.scratch/local-ai/live/12/or.py <path>` calls OpenRouter with it without printing the key.

## Rules from the user
- One ticket at a time, and stop for review. Commit, push or merge only when asked. Don't release.
- Size subagents: Sonnet for bounded builds, Haiku for mechanical work, Opus only for hard reasoning.
- **Work in your own worktree.** Never share one with another thread: it reset uncommitted work once.
- When a ticket is done: start the app (restart it if it's running) and end with a short manual-test checklist. Never click, type in or screenshot the real app; test with unit tests and Playwright.
- Build to `docs/index.html`, `docs/flow.html` and `DESIGN.md`. No jitter. No hacky workarounds. Keep designs model-agnostic.
