# How T3 Code discovers available models

Type: research
Status: resolved
Blocked by: 

## Question

How does T3 Code (latest source) build its model list for each provider it supports, especially Claude Code, Codex, OpenCode and Pi: which command, protocol call or API it uses per agent (e.g. Codex app-server `model/list`, `opencode models` or the `opencode serve` API, Pi RPC, Claude's aliases or a hardcoded list), whether it also lists models from the agent's configured providers (OpenRouter keys inside OpenCode/Pi), how it gets context window, price and image support per model (if at all), how often it refreshes, caching, and what it shows when discovery fails or a CLI is missing. Also how it handles custom models and searching a long list. Note where Myna could reuse the same approach from Rust.

## Answer

- Mechanism differs per agent. Codex: `codex app-server` JSON-RPC `model/list`, paged. Pi: `pi --mode rpc` request `get_available_models`. OpenCode: `opencode serve` HTTP (`model.list` on v2, `provider.list` connected-only on v1). Claude: no discovery, a bundled `model-manifest.json` of aliases and context windows.
- Price and image support: T3 shows neither. The agents do return them. Observed here: Pi RPC gives `cost`, `contextWindow`, `maxTokens`, `input`; OpenCode 2 `GET /api/model` gives `cost`, `limit.context`, `capabilities.input` for 440 models; Codex `model/list` gives `inputModalities` only. Myna can read those fields for free.
- OpenRouter appears only through OpenCode and Pi when the user has a key there (351 of 440 OpenCode models; Pi lists openrouter as one of its 2 providers). T3 has no direct OpenRouter listing, so Myna needs its own call for that.
- Refresh: health check every 5 minutes (configurable) respawns the CLI; Claude manifest fetched from GitHub raw with 1 hour TTL, 5 minute retry backoff, bundled copy as floor. Last result cached on disk per provider and merged at startup.
- Failure: keep the list, mark the provider `error` or `warning` with a CLI-specific message (`claude` not on PATH, `codex login`, no models yet). A failed or empty refresh keeps the last good list. OpenCode re-reads until two reads match to avoid partial cold catalogs.
- Custom models: free-text slug per provider, appended as `isCustom`. Search is client-side fuzzy ranking over name, sub-provider and provider, with no pagination.
- Reuse in Rust: Codex and Pi stdio JSON lines with a timeout, OpenCode `opencode api --standalone GET /api/model` (seen on 2.1.4 only), Claude alias list plus free-text. Refresh on picker open instead of every 5 minutes. Source: T3 Code ce48e45 (2026-10-31).

[research/30-t3-model-discovery.md](../research/30-t3-model-discovery.md)
- The user notes the 451 vs 440 OpenCode count gap doesn't matter (likely moved or decision-only OpenRouter models OpenCode doesn't support).
