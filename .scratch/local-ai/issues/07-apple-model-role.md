# What Apple's on-device model can serve

Type: grilling
Status: resolved
Blocked by: 02

## Question

Given its limits, which features can Apple's on-device model serve (Summary, Chat, LLM cleanup, Decision provider), and is it worth its cost (Xcode build step, macOS 26+ only, Apple Intelligence on) now that Myna ships no other Built-in model? If it is dropped, Myna has no Built-in models at all.

## Answer

Apple's on-device model is out of Phase 1. It can't summarize (4K context, guardrails on meeting text), and classification alone didn't justify the Xcode macro build, the macOS 26 gating and the async bridge. Myna now has no Built-in models.

- The user's replacement idea: use their existing AI subscription (Claude, Codex, OpenCode, Pi) through ACP, so they pay no API cost. Researched in [Agent Client Protocol for subscriptions](14-acp-research.md), decided in [Agents over ACP as an AI provider](15-acp-provider.md).
- Decision models (Jev, OpenAI Decisions, OpenRouter) hosted by Myna for free: decided in [Myna-hosted Decision provider](16-hosted-decision-provider.md).
- Per-feature routing (which feature goes to which provider and model) was confirmed as a requirement and handed to [Provider configuration](09-provider-configuration.md).
