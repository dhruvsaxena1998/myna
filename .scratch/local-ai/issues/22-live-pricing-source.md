# Live pricing source for the curated list

Type: research
Status: resolved
Blocked by: 

## Question

Where can a desktop app fetch current per-token prices and context windows for its curated models on every launch, without an API key or user data? Compare OpenRouter's public models API, models.dev's api.json, LiteLLM's model_prices_and_context_window.json, and vendor pages or APIs (OpenAI, Anthropic, Groq, Together, TypeSafe/Jev, OpenAI Decisions). For each: coverage of those vendors and of STT per-minute prices, freshness, format stability, rate limits and terms, licensing for redistribution, and how ids map to each vendor's native model ids. Also: is fetching a third-party file on every launch better than Myna's own `models/curated.json` on GitHub kept current by a scheduled job?

## Answer

- No vendor API gives prices without a key (OpenAI, Anthropic, Groq and Together `/v1/models` all return 401), so an aggregator is needed.
- models.dev (MIT, 620 KB gzipped, updated every few minutes, v1 compatibility promised) covers OpenAI, Anthropic, Groq and Together chat with native ids, and `?type=decision` includes Jev and `openai/gpt-6-luna-decisions`. It has no STT prices.
- LiteLLM (MIT file, 167 KB gzipped) is the only source with per-second Groq and OpenAI STT prices, and it lacks OpenAI Decisions. Together STT is in no source and needs a hand-entered price.
- OpenRouter `/models` prices only its own routes, has unlabelled STT units, and its terms bar automated copying of the site.
- Recommendation: Myna's own `models/curated.json`, built by a scheduled job from models.dev plus LiteLLM, fetched at launch. Decision entries need a host chosen per entry because context and price differ by host.

Details: [research/22-live-pricing-source.md](../research/22-live-pricing-source.md)

## Superseded

Model lists and pricing are replaced by [Live model lists for Agents and OpenRouter](29-live-model-lists.md): Agents and Endpoints list models live, prices come from OpenRouter, Pi, OpenCode and models.dev at runtime, and `models/curated.json` holds only the Claude model list.
