# Decision provider shape

Type: grilling
Status: resolved
Blocked by: 03, 15

## Question

Settled by [Provider configuration](09-provider-configuration.md): with no Decision provider set, Decision tasks fall back to the Fast model.


Which tasks go to the Decision provider (screenshot relevance, muted-segment triage, folder suggestion, others), which kinds it can be (decision API with the user's key, Endpoint with a curated small-model list, Agent), and what happens when none is set (fall back to the AI provider, a deterministic rule, or feature off)? User direction: folder suggestion goes through a Decision provider first (a curated OpenRouter list if a key is set); otherwise the cheapest model the user's Agent offers, or their Endpoint.

## Answer

- **Decision tasks** (Phase 1, each with its own toggle): folder suggestion (meeting title, opening transcript lines and folder names in; one folder or "none" out, shown as a chip to accept), muted-segment triage (experimental, include yes/no), screenshot relevance (experimental, keep yes/no). No others.
- **Kinds**: a third provider kind, **Decision API** (Jev with its own Keychain key; OpenAI Decisions reusing the key of an OpenAI Endpoint the user added), offered only in Decision selectors. Endpoints and Agents also qualify. OpenRouter works as an ordinary Endpoint: it lists `openai/gpt-6-luna` (OpenAI Decisions' model) and `typesafe/jev-router` through chat completions with a JSON schema; output is billed there and no probability comes back (checked against the live models API, 2026-10-31).
- **Routing**: one Decision provider setting (provider + model) for all three tasks, per-task override under Customize. Unset falls back to the Fast model; if Fast is unset too, the task toggle is disabled with the reason. No hand-written fallback rules: without a provider, triage leaves segments collapsed and screenshots keep only the existing near-duplicate filter.
- **Images**: screenshot relevance is offered only when the routed provider is known to take images. User direction: Agents may take screenshots, for relevance and for Summary, if their CLI accepts images with tools off; researched in [Image input through Agent CLIs](19-agent-image-input.md). Detection for other Endpoints stays in the fog under screenshots in AI.
- **Triage safety**: segments included by triage get an "included by triage" marker and a one-click undo. Decision APIs include only at probability ≥ 0.8; a chat model's yes counts as given.
- **Triage off-device warning**: when routed off-device, the toggle reads "Sends possibly-muted speech off this Mac", not the generic pill.
