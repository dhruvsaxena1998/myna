# Agents as an AI provider

Type: grilling
Status: resolved
Blocked by: 14, 17

## Question

Should a signed-in agent over ACP (Claude Code, Codex, OpenCode, Pi) be a kind of AI provider next to Endpoints, for which features (Summary, Chat, LLM cleanup, Decision), and with what safety rules (tools denied, sandbox directory) and failure behaviour? How is it discovered, named in the glossary and shown in settings?

## Answer

Copy T3 Code, not ACP: Myna spawns the user's installed agent CLI in its own headless mode with tools off. Recorded in [ADR 0002](../../../docs/adr/0002-agents-as-ai-provider.md).

- **Term**: an **Agent** is a coding-agent CLI the user has installed and signed in to (Claude Code, Codex, OpenCode, Pi), run headless with every tool off. An AI provider is an Endpoint or an Agent.
- **Features**: Summary, Chat, transcript cleanup, folder suggestion, muted-segment triage. Not dictation (it relies on fast STT and rules; agents are too slow per call). Not screenshot relevance or screenshots in AI for now (image input through these CLIs is unverified). Later: the user wants Agents to take screenshots if the CLI allows it with tools off; see [Image input through Agent CLIs](19-agent-image-input.md).
- **Safety, fixed by Myna and not user-adjustable**: Claude `claude -p --tools "" --strict-mcp-config --disable-slash-commands --permission-mode dontAsk` with JSON output; Codex `codex exec --ephemeral -s read-only` with MCP and plugins off; OpenCode with a config that denies all tools; Pi with `--no-builtin-tools`. Empty app-owned temp directory. If Myna can't confirm tools are off for an installed version, it refuses that agent and says why.
- **Credentials**: detection only. Find the binary (PATH or a set path), check its login, show the command to run ("Run `claude auth login`"). Never ask for or store a token. Codex's in-app ChatGPT OAuth is skipped in Phase 1.
- **Off-device**: an Agent always shows the "sends text off this Mac" pill.
- **Terms**: ship all four; Agent setup says "Uses your own <agent> login; your plan's terms apply." Ask Anthropic in writing ([Ask Anthropic about Claude Code plan use](18-ask-anthropic.md)); not blocking.
- **Failure**: not installed, signed out, plan limit or timeout shows the error and the fix. No silent switch to another provider (T3 Code does fall back on usage limits; Myna doesn't, because the fallback could be a cloud provider the user didn't pick for that feature).
- Handed on: folder suggestion uses a Decision provider first (curated OpenRouter list if a key is set), else the cheapest model the user's Agent offers. Goes to [Decision provider shape](08-decision-provider-shape.md).
