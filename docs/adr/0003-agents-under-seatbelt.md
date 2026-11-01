# Agents run under Seatbelt; Myna's tools reach them over a socket

Chat needs the model to look things up in the transcript, so an Agent can't run with every tool off (ADR 0002). Instead, every Agent run (Summary, Chat, cleanup, Decision tasks) starts the user's installed CLI under a macOS Seatbelt profile. The profile lets it read only the CLI's own files and sign-in, write only to an empty per-run temp directory, and reach Myna through one Unix socket. No meeting files are in the sandbox. For Chat, the CLI's only tool source is Myna's MCP server, a thin stdio relay that forwards calls over that socket to the Myna app, which answers from SQLite for the one meeting id the run is bound to. Endpoints get the same tools through function calling, with the same limits.

We rejected a Linux microVM (shuru) for Phase 1. It is the stronger wall, but it can't run the user's macOS CLI or reach the Keychain sign-in, so Myna would have to install each CLI and store long-lived tokens. That is the ops and credential burden ADRs 0001 and 0002 avoid. It stays a Phase 2 experimental "Strict mode".

## Consequences

- ADR 0002's "every tool, MCP server and slash command off" becomes: the CLI's own tools off, no MCP server except Myna's, no shell or web, writes confined by the sandbox. Summary, cleanup and Decision tasks still get no tools at all.
- Per-CLI flags stay as a second layer. Before each run Myna checks what loaded (Claude's `system/init` event, `codex mcp list`, `pi mcp list`) and refuses the Agent if anything besides `myna` appears.
- Codex's `apply_patch` can't be turned off; it can only write into the empty temp directory.
- What an Agent can see is decided by which meeting id the app answers for, not by which files were copied. Cross-meeting Chat later binds a run to the library on the same socket.
- Network stays open in Phase 1; Seatbelt can't allow-list by host without a proxy. With only Myna's read-only tools, nothing in the run can make requests except the CLI itself.
- `sandbox-exec` has been marked deprecated since 2017, but Codex and Claude Code both rely on it. If Apple removes it, revisit shuru.
- Unverified until a live run per CLI: Keychain sign-in under the profile, and Myna's tools answering through the relay.
