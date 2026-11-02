# Agents as an AI provider: the user's own CLI, headless, tools off

Myna can send Summary, Chat, transcript cleanup and Decision tasks to a coding agent the user already has installed and signed in to (Claude Code, Codex, OpenCode, Pi), so the work runs on their existing plan instead of an API key. Myna spawns each agent's own headless mode with every tool, MCP server and slash command off, in an empty app-owned directory, following T3 Code's text-generation jobs. We rejected ACP: permission requests in ACP are advisory, so a client can't guarantee that a hostile transcript won't make the agent read files or run commands. Each agent's own flags can guarantee it.

Amended by [ADR 0003](0003-agents-under-seatbelt.md): runs are sandboxed with Seatbelt, and Chat gets Myna's MCP server as its only tool source.

## Consequences

- Myna never asks for or stores agent credentials. It detects the binary and its login, and tells the user which command to run.
- If Myna can't confirm that tools are off for an installed agent version, it refuses to use that agent.
- Terms risk: Anthropic's legal page bars third-party apps from routing requests through plan credentials, while allowing users to sign in to the unmodified `claude` binary, and its support page says `claude -p` draws from the plan. We ship Claude anyway, with a notice that the user's plan terms apply, as T3 Code and Conductor do, and did not ask Anthropic for a ruling. If it becomes a problem, Claude Code is disabled as an Agent and Claude stays available through the Anthropic API as an Endpoint with the user's key; the other three Agents are unaffected.
- One adapter per agent, since each has its own headless protocol (`claude -p`, `codex exec`, OpenCode, `pi --mode rpc`). OpenCode 2.1.4 has no `serve` and Pi has native MCP only from 0.99.0, so each adapter sets a minimum version.
