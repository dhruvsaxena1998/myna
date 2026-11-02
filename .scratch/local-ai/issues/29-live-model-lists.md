# Live model lists for Agents and OpenRouter

Type: grilling
Status: resolved
Blocked by: 30, 31

## Question

The user wants every model each Agent CLI offers, and every OpenRouter model, instead of a curated subset; only Decision models stay curated. How does Myna get each list (`opencode models`, `pi --list-models`, Claude's aliases with no list command, Codex's app-server model list, OpenRouter `/api/v1/models`), how often, and what happens when one fails? Where do price, context window and image support come from per model (OpenRouter's API, Pi's table, models.dev by id match, else "cost unknown" and the 8K fallback)? Do OpenCode's `openrouter/*` and Pi's `openrouter` models (billed to a key, not a plan) show under the Agent? Which models may serve Main, Fast and Decision when nothing carries curated roles? What is left in `models/curated.json`, and what about the providers the user did not list (OpenAI, Anthropic, direct Jev and OpenAI Decisions, cloud STT)? Amends Curated model list: shape and upkeep, and Live pricing source.

## Facts found (2026-11-01)

- OpenRouter `GET /api/v1/models` needs no key: 452 text models with `context_length`, `pricing` (per token), `architecture.input_modalities` (images), `top_provider.max_completion_tokens`, `supported_parameters` (386 list `tools`).
- Decision models are hidden from the default list; `?output_modalities=all` returns 687 models, 33 with output `decisions`. All 11 of the user's decision models are there, priced on input only. They are served at `/api/alpha/decisions`, not chat completions. The user's view: a decision model has different input and output, so a separate endpoint is the natural design, not a surprise.
- The same list has 29 `transcription` models (Voxtral, Scribe, Gemini transcribe…), relevant to Speech-to-text through an Endpoint.
- Agent discovery per How T3 Code discovers available models: Codex app-server `model/list`, Pi RPC `get_available_models` (cost, window, images), OpenCode `opencode api GET /api/model` (cost, window, images), Claude has none (T3 bundles an alias manifest).
- The OpenCode 451 vs 440 count gap is not worth chasing (user).

## Notes from grilling

- Round 1: Q1–Q4, Q6, Q7 accepted as recommended.
- Q5 changed: the Decision provider picker shows every decision model OpenRouter lists (33 today), not only the user's 11.
- Q8 changed: direct Jev and OpenAI Decisions stay, with the user's own key. OpenRouter is an alternative route to them and to others. Rest of Q8 as recommended (OpenRouter the only cloud Endpoint preset; OpenAI, Anthropic and custom through the generic Endpoint with `/v1/models`; cloud STT parked).
- Round 2: Q9 (one Decision picker grouped by route: OpenRouter decision models, direct Jev, direct OpenAI Decisions; same model via two routes is two rows; Agents and Fast model still allowed) and Q10 (`models/curated.json` holds only the Claude model list; ticket 23 rescoped; tickets 20 and 22 superseded on lists and pricing) accepted.
- Round 3: Q11 (two decision adapters in Phase 1), Q12 (failure handling), Q13 (direct listing) accepted.

## Answer

- **Agents list live**, as T3 Code does: Codex app-server `model/list`, Pi RPC `get_available_models`, OpenCode `opencode api GET /api/model`. Claude has no list command, so `models/curated.json` carries Claude's aliases, windows and minimum `claude` version, bundled and refreshed on launch; models the installed CLI is too old for are hidden.
- **Under an Agent, only its own providers show** (OpenCode `opencode`, `opencode-go`, `openai`; Pi `openai-codex`). Their `openrouter` models are hidden; OpenRouter is used through Myna's own Endpoint.
- **Endpoints list live**: OpenRouter's keyless `/api/v1/models` (the only cloud preset), any other Endpoint's `/v1/models` with the user's key, local Endpoints as before.
- **Refresh** on launch and whenever a picker opens; last good list cached on disk; on failure the cached list stays with the reason (CLI missing, signed out, too old).
- **Metadata**: OpenRouter, Pi and OpenCode report price, window and image input; Codex and Claude prices and windows come from models.dev, fetched at launch and cached; Codex image support from its own list. Unknown shows "cost unknown" and the 8K fallback.
- **Roles are checked, not curated**: any chat model can be Main or Fast; Chat lookups need tool support (OpenRouter `supported_parameters`; Agents always via Myna's MCP); screenshots need image input. The picker greys out models lacking what a feature needs, with the reason.
- **Picker**: fuzzy search, grouped by provider, each row with price per million tokens or "your plan", window and an image badge; "Use a custom model id…" at the bottom.
- **Decision provider picker**: one picker grouped by route: every OpenRouter decision model (33 today, `output_modalities=decisions`), direct Jev (`GET /v1/models` with the user's key) and direct OpenAI Decisions (fixed id `gpt-6-luna`, in `models/curated.json`). The same model via two routes is two rows. Agents and the Fast model stay allowed.
- **Two decision adapters**: Jev format (direct Jev and all OpenRouter decision models via `/api/alpha/decisions`) and OpenAI Decisions format, behind one Decision provider interface.
- **Failures**: a failed decision call skips that task for the meeting with "Couldn't … : Decision provider error" and Retry, never falling back to the Fast model; a bad key or vanished endpoint counts as a broken provider (Provider configuration).
- **Not in Phase 1 presets**: OpenAI and Anthropic (usable through the generic Endpoint). Cloud STT stays parked; OpenRouter lists 29 transcription models.
- Replaces the lists and pricing parts of Curated model list: shape and upkeep and Live pricing source.
