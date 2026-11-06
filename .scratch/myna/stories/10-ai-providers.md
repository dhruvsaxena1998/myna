# 10: AI providers and models

**Status:** draft
**Spec stories:** 40–42, 47, 49, 54
**Depends on:** 01

Where Myna sends text for an LLM to answer, and which model each feature uses. Myna runs no LLM itself ([ADR 0001](../../../docs/adr/0001-no-bundled-llm-phase-1.md)). Nothing is set on a fresh install; AI features stay off until the user adds a provider. Summary is story 16, Chat is story 17. This story is the core that LLM cleanup, Summary, Chat and the Decision tasks build on.

> Amended 2026-11-06: **Agents are deferred** out of this story. They come back later as an Experimental feature, as their own effort: [Agents as an AI provider (deferred)](../../agents-provider/map.md). Direct Jev and OpenAI Decisions also wait; OpenRouter's decision models cover the Decision provider for now.

## Stories
- As a user, I want to add AI providers once: an **Endpoint** (OpenRouter preset, or any OpenAI-compatible URL such as Ollama, LM Studio or oMLX, local ones listed first), with keys in the Keychain.
- As a user, I want a **Main model** and a **Fast model** (provider plus model each) that every feature inherits, with a per-feature override under Customize.
- As a user, I want to pick models from live lists with price, context window and image support shown, never typed by me.
- As a user, I want one **Decision provider** for Decision tasks, falling back to the Fast model.
- As a user, I want a "sends text off this Mac" pill on every feature routed off this Mac, and cost shown wherever AI ran.

## Decisions (detail in the linked tickets)
- Provider records, Main/Fast/Customize, broken provider shows "Not set" and stops, never rebound: [Provider configuration](../../local-ai/issues/09-provider-configuration.md).
- Live model lists, picker greys out models missing a feature's needs: [Live model lists for Agents and OpenRouter](../../local-ai/issues/29-live-model-lists.md). For now, model details come from OpenRouter's own list; loopback Endpoints show "Free · runs on your server"; any other Endpoint shows "cost unknown". models.dev waits for the first Endpoint that needs it.
- Decision provider: three Decision tasks, two wire adapters planned (Jev shape for Jev and OpenRouter's `/api/alpha/decisions`, OpenAI Decisions shape), every question carries `instructions`, failed calls skip with Retry: [Decision provider shape](../../local-ai/issues/08-decision-provider-shape.md), [Decision endpoints](../../local-ai/issues/31-decision-wire-formats.md), [OpenRouter key for testing](../../local-ai/issues/12-openrouter-key.md). Only the Jev shape through OpenRouter is built now, reusing the OpenRouter Endpoint's key; no separate "Decision API" provider kind until direct Jev or OpenAI Decisions arrives.
- Cost: response cost, else price × tokens, else estimate (≈), else "cost unknown"; local is "Free · runs on your server": [Cost and context-window display](../../local-ai/issues/10-cost-and-context-display.md).
- Off-device: any host other than loopback, LAN included.
- Every provider call and every key lives in Rust behind one provider interface (send a request, get text, usage and cost back), with one adapter per wire format (OpenAI-compatible chat, Jev-shape decisions) and a Fake for tests, like `runtime::transcriber`. A new provider is a new adapter, nothing else changes.
- Request/response only; streaming is added by story 17.
- Customize rows and per-feature off-device pills ship with each feature's own ticket (04 ticket 7, story 16, …), not here.

## Tickets
1. **Design.** Update `docs/index.html` to match this story: the AI provider page (provider list, add an Endpoint, Main and Fast, off-device pill, Test) and the Decision provider page. Built with the prototype skill; reviewed before ticket 2 starts.
2. **Endpoints.** Add Ollama and OpenRouter Endpoints (any OpenAI-compatible URL, local ones listed first), key in the Keychain, all calls from Rust. Models picked from the live `/v1/models` list, with OpenRouter's price, window and image support. Set Main and Fast. **Test** button per Endpoint: a short prompt, showing the reply, time taken and cost. Deleting a provider turns Main/Fast routed to it "Not set" with a notice. Fake provider plus a local HTTP stub for tests; a routing-lookup unit test with a fake feature.
3. **Decision provider on OpenRouter.** OpenRouter's decision models through `/api/alpha/decisions` (Jev shape), using the OpenRouter Endpoint's key. With no Decision provider set, falls back to a JSON-schema call on the Fast model's Endpoint. **Test** button: a fixed sample question with 5 options, showing the pick, confidence, time taken and cost. Failed calls skip with Retry.
4. **Fresh-install check (human).** Real OpenRouter key and a local Ollama, on the real app from a fresh install.

Later, not in this story's build: direct Jev and OpenAI Decisions (the "Decision API" kind), models.dev details, streaming (story 17), Agents ([deferred map](../../agents-provider/map.md)).

## Built app vs. the design (fix while building)
Ticket 1's mockup (`docs/index.html`, PR #107) is the target; approved 2026-11-06. Where the app or the older mockup parts differ:
- **Settings pages.** The app has one "AI & privacy" page (id `ai`, `src/routes/settings/+page.svelte`) holding speech-to-text, Unload after, telling voices apart and a "will live here too" note. The design splits it: **Speech models** (id `models`: speech-to-text, telling voices apart), **AI provider** (id `ai`), **Decision provider** (id `decide`), in that order after Meeting notes. Ticket 2 renames the page and adds AI provider; ticket 3 adds Decision provider. Drop the "will live here too" note. Keep the page comment that says the pages match `SET_PAGES`.
- **Unload after** is in the app but not in the mockup. Keep it on Speech models; the mockup is behind here.
- **Caution colour.** The mockup draws "Sends text off this Mac", "Not set" and the delete notice with its old `--alert` (red-ish). Build them with `--caution`, `--caution-text`, `--caution-soft` from `design/tokens.css` (amber), per DESIGN.md.
- **"Stays on this Mac" pill** for loopback providers is new in the design; build it as the status pill.
- **Onboarding.** The mockup's onboarding still has a "Summaries and answers" step (local Qwen/Llama download, OpenAI/OpenRouter key). The app doesn't, and shouldn't: a fresh install has no providers (ADR 0001). The mockup is stale there.
- **Feature surfaces in the mockup** (Summary line, Chat footer and cost, folder suggestion, Experimental "Tidy up with AI" and the picture rows) still read the old local-or-online setting and say "Sends your words online". None are built yet. When each feature's ticket builds them (04 ticket 7, stories 16, 17), route through Main or Fast and use "Sends text off this Mac".
- **Experimental "Decide on muted speech"** now says it uses the Decision provider (was "quick decisions (in AI & privacy)").

## Done when
- [ ] Fresh install: no providers, Main and Fast "Not set", AI features off
- [ ] Add an Ollama Endpoint and an OpenRouter Endpoint (key in Keychain); pick Main and Fast from live lists; Test shows reply, time and cost
- [ ] Decision provider Test returns a pick with confidence through OpenRouter; with none set, the same Test runs on the Fast model
- [ ] Deleting a provider turns Main, Fast and the Decision provider routed to it "Not set" with a notice naming them
- [ ] Off-device pill matches routing (tested against a local HTTP stub)
- [ ] Checked with the real OpenRouter key and a local Ollama, on the real app from a fresh install

## Open questions for you
- none
