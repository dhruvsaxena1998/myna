# How Myna runs Gemma

Type: grilling
Status: resolved
Blocked by: 01

## Question

Which runtime does Myna use for the Gemma Built-in model, which Gemma sizes does it offer, and what is the memory cut-off for each? Write the runtime choice as an ADR.

## Answer

Myna ships no Gemma, and no LLM of its own, in Phase 1. Recorded in [ADR 0001: No bundled LLM in Phase 1](../../../docs/adr/0001-no-bundled-llm-phase-1.md).

- Summary, Chat, LLM cleanup and folder suggestion go to an Endpoint the user points at: Ollama, LM Studio or oMLX (local presets, listed first) or a cloud service (OpenAI, Anthropic, OpenRouter, custom).
- AI features are off on a fresh install until an Endpoint is set. Summary's empty state links to the AI provider page.
- Myna never installs or manages a local server. It checks that the URL answers ("Ollama isn't running at localhost:11434").
- No MLX, no helper process, no Gemma tiers, no LLM slot on the Models page.
- Apple's on-device model stays on the map, and its role is decided in [What Apple's on-device model can serve](07-apple-model-role.md).
- Raised by the user and handed on: choosing a model per feature (Sonnet for Summary/Chat, Haiku for cleanup) goes to [Provider configuration](09-provider-configuration.md). Folder suggestion through the Decision provider with the AI provider as fallback goes to [Decision provider shape](08-decision-provider-shape.md).
