# 10: AI providers and models

**Status:** draft
**Spec stories:** 40–42, 47, 49, 54
**Depends on:** 01

Where Myna sends text for an LLM to answer, and which model each feature uses. Myna runs no LLM itself ([ADR 0001](../../../docs/adr/0001-no-bundled-llm-phase-1.md)). Nothing is set on a fresh install; AI features stay off until the user adds a provider. Summary is story 16, Chat is story 17.

## Stories
- As a user, I want to add AI providers once: an **Endpoint** (OpenRouter preset, or any OpenAI-compatible URL such as Ollama, LM Studio or oMLX, local ones listed first), an **Agent** (my signed-in Claude Code, Codex, OpenCode or Pi), or a **Decision API** (Jev, OpenAI Decisions), with keys in the Keychain.
- As a user, I want a **Main model** and a **Fast model** (provider plus model each) that every feature inherits, with a per-feature override under Customize.
- As a user, I want to pick models from live lists with price, context window and image support shown, never typed by me.
- As a user, I want one **Decision provider** for Decision tasks, falling back to the Fast model.
- As a user, I want a "sends text off this Mac" pill on every feature routed off this Mac, and cost shown wherever AI ran.

## Decisions (detail in the linked tickets)
- Provider records, Main/Fast/Customize, broken provider shows "Not set" and stops, never rebound: [Provider configuration](../../local-ai/issues/09-provider-configuration.md).
- Live model lists for Endpoints and Agents, picker greys out models missing a feature's needs, `models/curated.json` for Claude's aliases and the OpenAI Decisions id: [Live model lists for Agents and OpenRouter](../../local-ai/issues/29-live-model-lists.md), [How T3 Code discovers available models](../../local-ai/issues/30-t3-model-discovery.md), [Write the Claude model list](../../local-ai/issues/23-fill-curated-prices.md).
- Agents: detection-only sign-in, one adapter per CLI with a minimum version, always off-device, no silent fallback ([ADR 0002](../../../docs/adr/0002-agents-as-ai-provider.md)). Every run under Seatbelt with Myna's MCP relay as the only tool source; the per-CLI profile paths and flags are in [Live Seatbelt run per Agent](../../local-ai/issues/27-live-seatbelt-run.md) ([ADR 0003](../../../docs/adr/0003-agents-under-seatbelt.md)). Refuse a run if any tool other than Myna's appears or succeeds; delete OpenCode's session after each run.
- Decision provider: three Decision tasks, Decision API / Endpoint / Agent, two wire adapters (Jev shape for Jev and OpenRouter's `/api/alpha/decisions`, OpenAI Decisions shape), every question carries `instructions`, failed calls skip with Retry: [Decision provider shape](../../local-ai/issues/08-decision-provider-shape.md), [Decision endpoints](../../local-ai/issues/31-decision-wire-formats.md), [OpenRouter key for testing](../../local-ai/issues/12-openrouter-key.md).
- Cost: response cost, else price × tokens, else estimate (≈), else "cost unknown"; local is "Free · runs on your server"; Agents show API-price worth: [Cost and context-window display](../../local-ai/issues/10-cost-and-context-display.md). Prices and windows from OpenRouter, Pi, OpenCode or models.dev.
- Off-device: any host other than loopback, LAN included.

## Done when
- [ ] Fresh install: no providers, Main and Fast "Not set", AI features off; detected Agents listed, never auto-selected
- [ ] Add an Ollama Endpoint and an OpenRouter Endpoint (key in Keychain); pick Main and Fast from live lists
- [ ] Each of the four Agents detected, signed-in state shown with the command to fix it; a Seatbelt run passes the tools-loaded check
- [ ] Deleting a provider turns every feature routed to it "Not set" with a notice naming them
- [ ] Off-device pill matches routing in every feature row (tested against a local HTTP stub)
- [ ] Checked with the real OpenRouter key and at least one signed-in Agent, on the real app from a fresh install

## Open questions for you
- none
