# Live Seatbelt run per Agent

Type: task
Status: open
Blocked by: 24

## Question

Needs the user at their Mac, signed in to each CLI. For Claude Code, Codex, OpenCode and Pi: run one real Chat request under the Seatbelt profile from ADR 0003, with Myna's relay MCP server as the only tool source. Check that the Keychain or file sign-in works, a tool call reaches the socket and returns, nothing besides `myna` loads, and `$HOME` reads and writes outside the temp directory fail. Record the working profile and flags per CLI.
