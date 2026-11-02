# OpenRouter key for testing

Type: task
Status: resolved
Blocked by: 

## Question

The user provides an OpenRouter key so Endpoint, Decision provider and cost tickets can be tried against a real service. Record where it lives for local testing (never committed) and which models it was checked with.

## Answer

- **Where the key lives**: `OPENROUTER_API_KEY` in `.env.local` at the repo root, ignored by `.env*.local` in `.gitignore` (added 2026-11-01; it was not ignored before). Temporary key: $5 cap, expires 2026-11-08. Helper `live/12/or.py <path> [json]` reads it without printing it.
- **Checked 2026-11-01** (total spend about $0.0001):
  - `GET /api/v1/key`: limit, spend and expiry readable with the key itself.
  - `GET /api/v1/models`: 452 models; `?output_modalities=decisions`: 33.
  - Chat with function calling, `qwen/qwen3.7-flash`: returned a `search_transcript` tool call with sensible arguments; `usage.cost` present per response ($0.000031), with prompt/completion split in `cost_details` and reasoning tokens counted separately.
  - `POST /api/alpha/decisions`, `typesafe/jev-1.14` and `openai/gpt-6-luna-decisions`: one `choice` (folder) and one `noul` (muted-segment triage) in a single request; both picked "Product" (1.0 and 0.99) and scored the private aside low (0.15 and 0.01). Input-only billing confirmed: $0.000018 and $0.000035, `output_tokens` 0 on Luna.
- **Correction to research 31**: each question needs an `instructions` string (the question text); without it the request fails 400 with a Zod `invalid_union` on `questions.<name>.instructions`. The response `model` comes back dated (`typesafe/jev-1.14-20261014`), so store the requested id, not the returned one.
