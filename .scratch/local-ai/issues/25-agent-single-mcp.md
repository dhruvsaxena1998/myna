# Agents with only Myna's MCP server

Type: research
Status: resolved
Blocked by: 

## Question

For Claude Code (`claude -p`), Codex (`codex exec`), OpenCode and Pi in headless mode: can Myna pass exactly one MCP server (its own, read-only, local stdio) while every built-in tool and every user-configured MCP server stays off? Exact flags or config (e.g. `--mcp-config` with `--strict-mcp-config` and `--tools ""`/`--allowedTools mcp__myna__*`), whether built-in tools can still run, and whether the user's own MCP servers or plugins leak in. Also: which local Endpoint servers (Ollama, LM Studio, oMLX) support OpenAI function calling, and for which model families.

## Answer

- Feasible for all four Agents, but each needs its own recipe. No single flag turns tools off and also drops the user's MCP servers.
- Claude Code: `--mcp-config` + `--strict-mcp-config` + `--tools ""` + `--allowedTools 'mcp__myna__*'` (with `dontAsk`). `--tools` does not touch MCP tools. Whether strict mode also excludes plugin MCP servers and claude.ai connectors is undocumented. The `system/init` event lists tools, servers and plugins, so Myna can check at run time.
- Codex: `--ignore-user-config`, `features.plugins/apps/shell_tool/view_image=false`, `web_search="disabled"`, and `mcp_servers.myna.*`. `apply_patch` has no off switch, so only `-s read-only` guards it. `codex exec` denies MCP tools that lack `readOnlyHint: true`, so set that or `default_tools_approval_mode="approve"`.
- OpenCode: inline config merges with the user's, so use `permission {"*":"deny","myna_*":"allow"}` and move `XDG_CONFIG_HOME` to keep user servers from loading. The installed 2.1.4 has no `serve` subcommand, which ADR 0002 assumes.
- Pi: native MCP only since 0.99.0 (installed 0.85.0 has none). `--tools 'mcp__myna__*'` filters, but the user's `mcp.json` servers still connect.
- Ollama, LM Studio and oMLX all accept `tools` on chat completions. Support is per model, via the chat template, so gate Chat tool use on a per-model check.

Full findings with sources: [research/25-agent-single-mcp.md](../research/25-agent-single-mcp.md)
