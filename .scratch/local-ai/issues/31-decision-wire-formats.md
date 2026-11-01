# Decision endpoints: OpenRouter, Jev and OpenAI Decisions

Type: research
Status: resolved
Blocked by: 

## Question

Compare the wire formats of OpenRouter's `/api/alpha/decisions`, Jev's own API and OpenAI Decisions: request shape (prompt, typed options, images), response (choice, probabilities or scores), auth, pricing and usage fields, limits, and how "alpha" OpenRouter's route is (docs, changelog, stability promises). Can one adapter in Myna serve all three, or one per format? Also: how Myna lists models for direct Jev and OpenAI Decisions (a models endpoint with the user's key, or a fixed set of ids).

## Answer

- OpenRouter's `POST /api/alpha/decisions` uses the Jev wire format: `state`, a `questions` object keyed by name, an `answers` object keyed by name. Non-Jev models (including `openai/gpt-6-luna-decisions`) sit behind the same shape. OpenRouter translates.
- OpenAI Decisions differs: `input` plus a `questions` array with `choices: [{value, description}]`, `predicate` instead of `noul`, and an `answers` array with `probabilities` as a list.
- Recommendation: two adapters behind one Decision provider interface. One Jev-shape adapter (base URL, key, path, model id) serves direct Jev and OpenRouter. An OpenAI-shape adapter is optional and can come later.
- Auth is a bearer key on all three. Output is free on Jev and OpenAI. OpenRouter passes through per-model input prices ($0.01 to $0.24 per 1M) and adds `usage.cost`.
- "Alpha" has no stated meaning. OpenRouter's versioning page promises changelog notes for breaking changes on `v1`, but the route sits under `/api/alpha/`. Its schema has changed since 2026-09-17 with no breaking entry. A `/api/v1/systemone` alias exists for Jev ids. I did not test it with other vendors' ids.
- List models for direct Jev with `GET https://api.typesafe.ai/v1/models` and the user's key. It returns `{models:[{name, description, release_date}]}`. Keyless calls get a 403.
- List models for direct OpenAI Decisions with a fixed id, `gpt-6-luna`. Its docs name no models endpoint and I could not verify the live list without a key.
- For OpenRouter, `GET /api/v1/models?output_modalities=decisions&input_modalities=image` works without a key (33 decision models on 2026-11-01).

[research/31-decision-wire-formats.md](../research/31-decision-wire-formats.md)
