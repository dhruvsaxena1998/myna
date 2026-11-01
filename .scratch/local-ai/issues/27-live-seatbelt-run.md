# Live Seatbelt run per Agent

Type: task
Status: resolved
Blocked by: 24

## Question

Needs the user at their Mac, signed in to each CLI. For Claude Code, Codex, OpenCode and Pi: run one real Chat request under the Seatbelt profile from ADR 0003, with Myna's relay MCP server as the only tool source. Check that the Keychain or file sign-in works, a tool call reaches the socket and returns, nothing besides `myna` loads, and `$HOME` reads and writes outside the temp directory fail. Record the working profile and flags per CLI.

## Answer

Run live on 2026-11-01 with Claude Code 2.1.316, Codex 0.164.0, OpenCode 2.2.5 and Pi 1.4.0. Harness in `live/27/`: `app.py` stands in for the Myna app (answers one fixture meeting over a Unix socket and logs each call), `relay.py` is the stdio MCP relay, `profile.sb` is the base Seatbelt profile, and `run.sh <cli>` adds the per-CLI paths, runs the probes and then one real Chat question. The answer ("November 14, because the payments review slipped") was only reachable through the tools. All four answered correctly through the socket.

**Base profile**: `(allow default)`, deny every write except the per-run directory and tty devices, deny reading file contents anywhere under `$HOME` except the run directory and the paths below. Network and mach services stay open, as ADR 0003 says. `TMPDIR` points into the run directory. Under it, reading `~/.zshrc`, `~/.ssh/id_ed25519` and the other CLIs' `auth.json`, and writing to `~` or to `/tmp` outside the run directory, all fail.

| | Read | Write outside the run dir | Sign-in | Tool calls | Run |
|---|---|---|---|---|---|
| Claude Code | `~/.local/bin`, `~/.local/share/claude`, `~/.claude`, `~/.claude.json`, `~/Library/Keychains` | none | Keychain | 2 | 4 s, $0.0012 (haiku) |
| Codex | `~/.codex`, `~/Library/Keychains` | `~/.codex` (state, goals and log SQLite, models cache, `tmp/arg0`, shell snapshots; `auth.json` refresh) | `~/.codex/auth.json` | 1 | 7 s, 28.7K input tokens (gpt-6-luna) |
| OpenCode | `~/.opencode`, `~/.local/share/opencode`, `~/.cache/opencode`, `~/Library/Keychains` | `~/.local/share/opencode` (database, logs), `~/.cache/opencode` | `auth.json` in the data dir | 1–2 | 9 s (openai/gpt-6-luna) |
| Pi | node install dir, `~/.pi/agent` | `~/.pi/agent` (lock files beside `auth.json` and `settings.json`) | `~/.pi/agent/auth.json` | 1 | 7 s, 722 input tokens (gpt-6-luna) |

**Flags per CLI** (exact commands in `live/27/run.sh`):

- **Claude Code**: `-p --output-format stream-json --verbose --mcp-config <myna> --strict-mcp-config --tools "" --allowedTools 'mcp__myna__*' --permission-mode dontAsk --disable-slash-commands --no-session-persistence --settings '{"disableAllHooks":true,"disableClaudeAiConnectors":true}'`, env `ENABLE_CLAUDEAI_MCP_SERVERS=false`. The `system/init` event listed only the three `mcp__myna__*` tools and one MCP server (`myna`). User plugins (rust-analyzer-lsp and built-ins) still load but add no tools, so the run-time check must look at `tools` and `mcp_servers`, not `plugins`. `--safe-mode` is unusable: it drops `myna` and keeps the plugins.
- **Codex**: `exec --json --ephemeral --skip-git-repo-check --ignore-user-config --ignore-rules -s read-only`, with features `plugins`, `apps`, `shell_tool`, `view_image` and `sleep_tool` off, `web_search="disabled"`, and `mcp_servers.myna.{command,args,required=true,default_tools_approval_mode="approve"}`. The user's own `rea` MCP server did not load. Codex 0.164.0 still shows the model `exec` (a JavaScript isolate with no filesystem or network; Myna's tools are reached through it), `wait`, `request_user_input` and `collaboration.*` (subagents with the same tools). Turning off `multi_agent` and `code_mode_host` did not remove them. `apply_patch` was not offered. Codex's own skill scan of `~/.agents/skills` is blocked by the profile. `codex mcp list` rejects `--ignore-user-config`, so ADR 0003's "check what loaded with `codex mcp list`" does not work; Myna checks the `mcp_tool_call` events instead.
- **OpenCode**: `run --standalone --format json`, env `XDG_CONFIG_HOME=<run>/cfg`, `OPENCODE_DISABLE_PROJECT_CONFIG=1`, `OPENCODE_DISABLE_CLAUDE_CODE=1`, `OPENCODE_CONFIG_CONTENT={"mcp":{"myna":{"type":"local","command":[...]}},"permission":{"*":"deny","skill":"deny","myna_*":"allow"}}`. The `skill` tool stays visible but returns "Permission denied: skill".
- **Pi**: `-p --mode json --no-session -ne -e builtin:mcp -e <run>/myna.js --tools 'mcp__myna__*' -nc -ns -np --no-themes`, where `myna.js` calls `pi.registerMcpServer("myna", {command, args, exposure: "direct"})`. Only the three Myna tools were visible. No user `mcp.json` existed on this Mac, so whether the user's own servers connect was not tested.

**Findings that change the design**:

- ADR 0003's "write only to an empty per-run temp directory" holds only for Claude Code. Codex, OpenCode and Pi refuse to start unless they can write their own state directory, which also holds the sign-in they refresh. Writes become: the run directory plus the CLI's own state directory.
- Seatbelt matches resolved paths. `~/.pi` here is a symlink into `~/Dev/dotfiles`, and the allow rule only worked once the real path was added too. Myna must `realpath` every allowed path.
- **OpenCode keeps Myna's meeting text in the user's own OpenCode history** (`opencode.db`, with generated titles such as "Priya's launch date rationale", which is an extra model call). `opencode session delete <id> --standalone` works under the same profile, so Myna must delete the session after every run. The test sessions were deleted. Claude (`--no-session-persistence`), Codex (`--ephemeral`) and Pi (`--no-session`) left no meeting text behind.
- **OpenCode ignores invalid `OPENCODE_CONFIG_CONTENT` without any error.** With a malformed value, Myna's server and the deny rules vanished and the model got OpenCode's own `skill` and `execute` tools. Myna must check the tool events and refuse any run where a tool other than `myna_*` succeeds.

**Not tested**: token refresh under the profile (Claude writes it to the Keychain, which this profile only reads), a user `mcp.json` for Pi, images through `get_screenshots`, and Keychain sign-in for Codex or OpenCode (both used file sign-in here).
