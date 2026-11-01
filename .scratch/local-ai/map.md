# Local AI for summaries and chat

Labels: wayfinder:map

## Destination

Stories 10 (AI provider, Summary and Chat), 03 (Models), 11 (Folders) and 12 (Experimental) rewritten with no open questions and ready to build, with an ADR for each runtime choice. The spec and PRODUCT.md updated to match.

## Notes

- Domain: Myna, macOS menubar app (Rust + Tauri 2, Swift bridge via swift-rs). Read `.scratch/myna/spec.md`, the four stories, `PRODUCT.md` and `GLOSSARY.md` before working a ticket.
- Use the glossary terms: AI provider, Endpoint, Agent, Preset, Decision provider, Off-device.
- Myna ships and runs no LLM ([ADR 0001](../../docs/adr/0001-no-bundled-llm-phase-1.md)); no Built-in models (Apple's model is out too). Ollama/LM Studio/oMLX and cloud services are Endpoints. The other kind is an Agent: the user's signed-in Claude Code, Codex, OpenCode or Pi, run headless with tools off under Seatbelt ([ADR 0002](../../docs/adr/0002-agents-as-ai-provider.md), [ADR 0003](../../docs/adr/0003-agents-under-seatbelt.md)).
- Providers are added once; a Main model and a Fast model (provider + model each) cover features, with per-feature overrides. Never silently rebound. See Provider configuration.
- Laya is out of Phase 1. Decision provider = a Decision API (Jev, OpenAI Decisions), an Endpoint (OpenRouter included), or an Agent; falls back to the Fast model. All bring-your-own.
- Local is the default and the recommendation everywhere, including STT. Cloud STT is opt-in, post-meeting only, never realtime (cost).
- Off-device pill: any host other than loopback is off-device, LAN included. 
- Cloud Endpoints and Agents offer only curated models; the curated list in the repo carries price, context window and image support. Local Endpoints take any model and show no price or window unless the user enters one. Cost = curated price × tokens (OpenRouter also returns cost directly).
- OpenRouter is the first real service tested against.
- In scope from story 12: screenshots in AI, screenshot relevance, muted-segment triage, Uhm. LLM cleanup leaves Experimental for the Dictation and Meetings settings (ticket 28).
- Research findings live in `.scratch/local-ai/research/`, linked from each ticket.
- Every grilling ticket: call the grilling and domain-modeling skills. One ticket per session.

## Decisions so far

- [Decision APIs and small classifier models](issues/03-decision-apis.md): Jev and OpenAI Decisions both exist, charge input only and handle 10–20 options easily; OpenRouter has no decision endpoint, only JSON-schema chat models.
- [Usage, cost and context window per response](issues/04-per-response-usage.md): only OpenRouter returns cost per response; others return tokens only, and context window has a different source per Endpoint (OpenAI exposes none).
- [Uhm, Voz and cloud STT endpoints](issues/05-stt-models-and-endpoints.md): Voz is Parakeet v3 on Core ML; Uhm and Voz SDKs need macOS 15+; Groq/Together give word timestamps, OpenAI gpt-4o transcribe doesn't; caps 25–80 MB.
- [Gemma on Apple Silicon](issues/01-gemma-on-apple-silicon.md): Gemma 4 (E2B–31B, Apache 2.0, vision) at 3.4–17.7 GB Q4; MLX is faster but only reachable via mlx-swift-lm (needs Xcode build); llama-cpp-2 works from Rust; Meetily uses llama.cpp + Ollama, not MLX.
- [How Myna runs Gemma](issues/06-gemma-runtime.md): it doesn't; no bundled LLM in Phase 1, AI features run through an Endpoint the user points at, local presets first ([ADR 0001](../../docs/adr/0001-no-bundled-llm-phase-1.md)).
- [Apple's on-device model](issues/02-apple-on-device-model.md): macOS 26+, M1+, Apple Intelligence on; 4K documented context (8K seen on macOS 27); images only on macOS 27; weak-links fine from a 14.2 target; @Generable enums suit classification; needs Xcode and a sync wrapper for swift-rs.
- [What Apple's on-device model can serve](issues/07-apple-model-role.md): nothing in Phase 1; dropped as too limited to summarize. Myna has no Built-in models; ACP subscriptions and hosted decisions raised as new tickets.
- [Agent Client Protocol for subscriptions](issues/14-acp-research.md): ACP reaches all five agents (Rust crate exists), but can't force-deny tools (per-agent hardening only); Anthropic and Google terms forbid plan use via third-party apps, OpenAI documents a route via `codex app-server`; latency unmeasured.
- [How T3 Code and Zed use Claude and Codex subscriptions](issues/17-prior-art-t3code-zed.md): T3 Code spawns the user's own CLIs per agent (`claude -p --tools ""`, `codex app-server`/`exec -s read-only`, `opencode serve`, `pi --mode rpc`), not ACP; Zed uses ACP adapters for all four. Neither cites vendor approval; Anthropic's billing and legal pages disagree.
- [Agents as an AI provider](issues/15-acp-provider.md): copy T3 Code, spawn the user's CLI headless with tools off (not ACP); Summary/Chat/transcript cleanup/Decision tasks only; detection-only sign-in; always off-device; no silent fallback ([ADR 0002](../../docs/adr/0002-agents-as-ai-provider.md)).
- [Provider configuration](issues/09-provider-configuration.md): providers added once; Main and Fast model defaults with per-feature overrides (LLM router now Phase 1); models picked from lists, curated OpenRouter JSON in repo; broken provider means "Not set"; STT is a separate selector reusing Endpoints; nothing set on fresh install.
- [Decision provider shape](issues/08-decision-provider-shape.md): three Decision tasks (folder suggestion, triage, screenshot relevance); new kind Decision API (Jev, OpenAI Decisions) beside Endpoints (OpenRouter included) and Agents; one Decision provider setting falling back to Fast; triage marks and can undo, needs p ≥ 0.8, warns about muted speech off-device.
- [Image input through Agent CLIs](issues/19-agent-image-input.md): all four take inline images with tools off (Claude stream-json base64 blocks, `codex exec --image`, OpenCode `files`/`--file`, Pi RPC `images`); non-vision models silently get a placeholder, so check vision support first; Claude path needs one live test.
- [Fitting long meetings into the context window](issues/11-long-meetings-context.md): whole transcript when it fits, else chunk-then-merge along transcription chunks; window and price come from a curated model list (cloud and Agents curated-only, local shows none unless set, 8K internal fallback); long Chat uses summary + chunk notes + FTS5 passages, no embeddings.
- [Curated model list: shape and upkeep](issues/20-curated-list-shape.md): one `models/curated.json` covering cloud presets, Decision APIs and Agents; roles main/fast/decision/stt; price, window, images, checked, source per entry; bundled plus refreshed on every launch; dropped models keep working, marked. The user supplies the models.
- [Live pricing source for the curated list](issues/22-live-pricing-source.md): vendor model APIs need a key; models.dev (MIT, minutes-fresh) covers chat and decision models, LiteLLM covers Groq/OpenAI STT, Together STT has no source; OpenRouter's terms bar automated copying. Recommends Myna's own curated.json built by a scheduled job from models.dev + LiteLLM.
- [Ask Anthropic about Claude Code plan use](issues/18-ask-anthropic.md): not asked; ship Claude Code like T3 Code and Conductor, fall back to the Anthropic API Endpoint if it becomes a problem.
- [Cost and context-window display](issues/10-cost-and-context-display.md): cost in Chat footer, under each Summary and per meeting; response cost > curated price × tokens > estimate (≈) > "cost unknown"; Agents show API-equivalent worth; Compact button at 60% context; Summary always chunked; Chat uses transcript tools, never the whole transcript.
- [Agents with only Myna's MCP server](issues/25-agent-single-mcp.md): all four can load one Myna MCP server, each with its own recipe; Claude cleanest (`--strict-mcp-config`, `--tools ""`, allow `mcp__myna__*`), Codex can't disable `apply_patch`, OpenCode merges user config, Pi still connects the user's MCP servers; local servers accept `tools` per model. Nothing run live.
- [Sandboxed Agent runs with shuru](issues/26-sandboxed-agent-runs.md): shuru is a fast Linux microVM (0.16 s warm boot, read-only mounts, host allow-list), but the user's macOS CLI and Keychain login can't run in it; Seatbelt keeps both and denied `$HOME` and network in a live test. Recommends Seatbelt plus per-CLI flags; shuru a later opt-in; cross-meeting Chat via one read-only text view or a loopback MCP server.
- [Transcript tools for Chat](issues/24-chat-transcript-tools.md): Summary and outline always sent; `search_transcript`, `read_transcript`, `get_screenshots(near)` bound to one meeting, ~2K tokens, 8 lookups; Endpoints via function calling with FTS5 fallback; Agents under Seatbelt with Myna's socket relay as the only tool source ([ADR 0003](../../docs/adr/0003-agents-under-seatbelt.md)). Live check per CLI is ticket 27.
- [LLM cleanup for dictation and transcripts](issues/28-llm-cleanup.md): dictation cleanup Endpoint-only with its own 3 s budget and a reply/drift guard, falling back to rules output; transcript cleanup per chunk by line id, raw kept, muted segments never sent; editable prompts in a fixed wrapper; app-bound Styles (Default, Code, Message, Email); moves out of Experimental.

## Not yet specified

- Screenshots in AI: curated models carry image support; how a local model is marked as image-capable, and how many frames are sent.
- Summary templates and chat persistence (including compacted history), as they meet the chunked-summary and transcript-tools answers. Per-folder transcript cleanup prompts are decided here too (ticket 28).
- Final rewrite of stories 10, 03, 11, 12 plus spec and PRODUCT.md (the principle "all STT runs on the Mac" becomes "local by default"; the out-of-scope "cloud public on demand" line becomes "Myna-hosted models: later").

## Out of scope

- Chunked and realtime meeting transcription (local realtime pass, then a chunked full pass after the meeting): Phase 1, but its own sibling wayfinder because it redesigns capture and transcript (stories 05, 09). This map assumes transcript text arrives in chunks.
- Laya and its managed Python sidecar: dropped from Phase 1.
- [Myna-hosted Decision provider](issues/16-hosted-decision-provider.md): not Phase 1. BYO key costs a single user almost nothing; hosting would add Myna's first server, abuse control for an account-less open-source client, and a privacy obligation. Later, as its own effort.
- VM isolation for Agents (shuru): Phase 2, as an experimental "Strict mode". Phase 1 uses Seatbelt; shuru would need Myna to install each Linux CLI and hold the user's tokens (ticket 26).
- Chat across all meetings: its own story after Phase 1; binds an Agent run to the library on the same socket (ticket 24).
- Host allow-list proxy for Agent runs: later; Phase 1 leaves the network open (ADR 0003).
