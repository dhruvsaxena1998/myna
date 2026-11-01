# Decision APIs and small classifier models

Type: research
Status: resolved
Blocked by: 

## Question

What are Jev and OpenAI Decisions exactly: wire format, auth, pricing (is output free?), latency, and limits on the number of options per question? Is picking one of 10–20 folders within what they handle well? Which small, cheap OpenRouter models suit yes/no and pick-one classification through plain chat-completions, as a basis for a curated list?

## Answer

- Both exist as named. Jev is TypeSafe's typed "System One" API (`POST api.typesafe.ai/v1/systemone`). OpenAI Decisions is `POST /v1/decisions`, model `gpt-6-luna` only, generally available.
- Pricing is input-only: Jev $0.042 per 1M tokens, OpenAI $0.10 per 1M. Output is free on both.
- Jev allows 255 options per choice question, 64k context by default. OpenAI allows 1 to 400 questions per request and does not document an option limit. 10 to 20 folders is well inside both.
- Neither publishes latency or accuracy for this size of choice. OpenAI claims "about 10x faster than Responses", unbaselined. Measure ourselves.
- Wire formats differ (answers keyed by name versus ordered array). Neither is reachable through OpenRouter chat completions. OpenRouter's `typesafe/jev-router` is a model router, not Jev's typed API.
- Cheap OpenRouter candidates with JSON-schema support are listed in the research file, untested for accuracy.

Full findings: [research/03-decision-apis.md](../research/03-decision-apis.md)
