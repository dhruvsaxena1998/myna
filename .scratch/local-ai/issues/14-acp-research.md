# Agent Client Protocol for subscriptions

Type: research
Status: resolved
Blocked by: 

## Question

How can a macOS app (Rust + Tauri 2) use the Agent Client Protocol to send a plain text task (summarize this transcript, answer a question about it, clean up this dictation) to an agent the user already has installed and signed in: Claude Code, Codex, OpenCode, Pi, Gemini CLI? For each: is ACP native or via an adapter (and which), how the client launches and talks to it, how to pick the model, startup and first-token latency, context window, image input, and whether usage or cost is reported. Safety: can the client deny every tool call and permission request, run with no MCP servers and an empty working directory, so a hostile transcript can't make the agent act? Terms: what each vendor's subscription terms say about using the plan through a third-party app via its own CLI.

## Answer

- ACP works for all five over stdio with the official Rust crate (`agent-client-protocol` 3.6.0, v1). Gemini CLI and OpenCode speak it natively. Claude Code, Codex and Pi need adapters (`claude-agent-acp`, `codex-acp`, `pi-acp`).
- A client cannot force-deny tool calls. Permission requests are advisory and the spec says "an Agent can act without asking". Omitting `fs` and `terminal` capabilities, rejecting permission requests, an empty `cwd` and `mcpServers: []` are necessary but not sufficient.
- Hardening is per agent. Claude is tightest (`tools: []`, `disallowedTools: ["*"]`, `settingSources: []`). Gemini auto-allows file reads and web search and keeps user MCP servers. Codex is bounded by a read-only sandbox. Pi never asks and needs a `--no-tools` wrapper.
- Terms differ. Anthropic's text rules out the Agent SDK path, Google's calls third-party use of Gemini CLI OAuth a violation, OpenAI documents a plan-usage route for local open-source apps (via `codex app-server`, not ACP). OpenCode and Pi inherit the provider's terms.
- Latency, context window per model and cost reporting are unmeasured or unverified. Nothing was run live.

Research: [research/14-acp-research.md](../research/14-acp-research.md)
