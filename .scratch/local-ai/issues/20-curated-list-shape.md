# Curated model list: shape and upkeep

Type: grilling
Status: resolved
Blocked by: 

## Question

What does the curated model list cover (cloud Endpoint presets, Decision APIs, cloud STT, Agents), what does each entry hold, what roles do models play in it (Main, Fast, Decision, STT), how does it stay current without stale prices, and what happens when a model a user picked leaves the list? Which models go in is researched afterwards against the criteria decided here.

## Answer

- **Covers**: cloud Endpoint presets (OpenAI, Anthropic, OpenRouter; Groq and Together for STT), Decision APIs (Jev, OpenAI Decisions) and Agents (Claude Code, Codex, OpenCode, Pi; OpenCode and Pi entries use their own `provider/model` ids). Local Endpoints are not in it.
- **Roles**: each model carries one or more of `main`, `fast`, `decision`, `stt`; each settings dropdown shows only models with that role. 2–4 models per provider or Agent.
- **Entry**: `provider`, `id` (exact id sent), `name`, `roles`, `context_window`, `max_output`, price (`input_per_mtok`/`output_per_mtok`, or `per_minute` for STT; `null` for Agents, shown as "uses your plan"), `images`, `checked` (date verified), `source` (price URL). JSON at `models/curated.json` in the repo.
- **Upkeep**: bundled in every release; prices refreshed **on every app launch** (user requirement), carrying no user data, with the bundled copy kept on failure and a setting to turn the refresh off. The source is researched in [Live pricing source for the curated list](22-live-pricing-source.md). A weekly drift check that opens a PR stays as the safety net for anything the live source doesn't cover.
- **Model leaves the list**: it keeps working from the user's settings, marked "No longer in Myna's list", "cost unknown" if its price is gone; never switched silently.
- **Which models**: chosen by the user, not by criteria research: [Curated model list from the user](21-user-model-list.md). Prices then filled in [Fill current prices for the curated list](23-fill-curated-prices.md). Revisit this ticket if the list raises shape questions.
