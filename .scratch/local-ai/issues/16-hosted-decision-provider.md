# Myna-hosted Decision provider

Type: grilling
Status: resolved
Blocked by: 

## Question

Should Myna host decision models (Jev, OpenAI Decisions, an OpenRouter small model) free for its users? This contradicts the spec's out-of-scope 'cloud public on demand beyond user-supplied keys' and its no-account stance. Decide: whether to do it in Phase 1, who pays, how abuse is limited without accounts, what the server sees and keeps, and how the off-device pill and privacy copy describe it.

## Answer

Out of scope for Phase 1. Decision providers are bring-your-own: Jev or OpenAI Decisions with the user's key, an OpenRouter small model, any Endpoint (a small model in Ollama is free and local), or an agent over ACP if that ticket allows it.

- Why: for one user the cost of their own key is negligible (text-only decisions are about $0.0003 per hour-long meeting; screenshots dominate at about $0.01–0.025). Hosting would have added Myna's first server, abuse control for an open-source client with no accounts, and a privacy obligation for transcripts passing through it.
- Hosting stays possible later as its own effort. In the final rewrite, the spec's out-of-scope line "Cloud public on demand beyond user-supplied keys" becomes "Myna-hosted models: later, separate effort", not "never".
