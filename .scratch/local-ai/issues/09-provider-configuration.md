# Provider configuration

Type: grilling
Status: resolved
Blocked by: 

## Question

Does one AI provider serve every feature, or can features pick their own? Settled requirement from the user: granular control, each feature (Summary, Chat, LLM cleanup, each Decision task) picks its own provider and model, e.g. Claude Sonnet for Summary and Chat, Haiku for cleanup. The open part is how that is configured and shown without becoming a wall of dropdowns. With the Decision provider taking small tasks, is the spec's out-of-scope 'LLM router' still needed? What does a provider config hold (Endpoint: preset, base URL, model, Keychain key; Agent: which CLI, binary path, model), and is the STT slot configured the same way?

## Answer

> Amended by [Summary templates and chat persistence](33-summary-templates-chat-persistence.md): a Chat digest row joins Customize, inheriting the Fast model.

> Amended by [Fitting long meetings into the context window](11-long-meetings-context.md): cloud Endpoints and Agents offer only curated models (no "All models"); local Endpoints accept any model.


Two layers, two defaults, overrides on demand.

- **AI providers list**: the user adds providers once. Endpoints are named by the user ("OpenRouter", "Ollama on this Mac"), each with its own Keychain key; several are allowed. Agents (Claude Code, Codex, OpenCode, Pi) are detected, one per CLI, with a "Use" button. A provider record holds: Endpoint = name, preset, base URL, Keychain key ref; Agent = which CLI, binary path.
- **Main model and Fast model**: each is one provider plus one model. Main serves Summary and Chat. Fast serves transcript cleanup and dictation cleanup, and is the Decision fallback when no Decision provider is set. Under "Customize", each feature row shows what it inherits ("Uses Main: Claude Code · Sonnet") and can override with its own provider and model. The spec's out-of-scope "LLM router" is now in Phase 1; remove that line in the final rewrite.
- **Models are picked, not typed**: Endpoints list `/v1/models`; Agents list the models their CLI accepts; OpenRouter shows a curated list first (a JSON file in the repo, shipped with each release) then "All models". Free text only for a custom Endpoint with no model list. Features that can't use an Agent (dictation cleanup, image features) don't offer Agents.
- **Broken or deleted provider**: every feature routed to it, directly or by inheriting Main or Fast, shows "Not set" and stops, with a notice naming them. Never rebound silently.
- **Off-device pill follows routing**: shown per feature row and wherever that feature appears, from the provider it is routed to.
- **Speech-to-text** is a separate selector on the Models page: a local model, or an Endpoint that supports audio transcription, reusing the same Endpoint records and keys. Agents can't do STT. Details in [Speech-to-text through an Endpoint](13-cloud-stt-endpoint.md).
- **Fresh install**: no providers, Main and Fast "Not set", AI features off. Detected Agents are listed but never auto-selected.
