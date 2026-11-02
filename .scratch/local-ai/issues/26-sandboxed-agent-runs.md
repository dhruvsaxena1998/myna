# Sandboxed Agent runs with shuru

Type: research
Status: resolved
Blocked by: 

## Question

Instead of trusting each Agent CLI's tool-off flags (research 25), can Myna run the Agent inside a sandbox that holds only one meeting's files (transcript, summary, screenshots), so even a misbehaving tool can't reach anything else? Look at shuru (https://shuru.run): what it is, how it isolates (microVM, container, Seatbelt?), macOS 14.2+ Apple Silicon support, start-up time and memory per run, how files get in (mount, copy, read-only?), how network is limited (the Agent still needs its vendor API and the user's sign-in), whether the user's CLI and its credentials work inside, licence, and how Myna would drive it from Rust. Also: for Chat across all meetings, how does a sandbox see many meetings without copying every file per conversation (read-only mount of the library, a query-only Myna server over a socket, an index-only view)? Compare with plain macOS `sandbox-exec`/Seatbelt as the no-dependency baseline.

## Answer

- Shuru is a microVM (Apple Virtualization.framework), Apache-2.0, macOS 14+ on Apple Silicon, Rust SDK `shuru-sdk` plus a CLI. Measured here: 0.16 s warm boot, 25 MB host RSS idle, mounts are read-only VirtioFS with a tmpfs overlay, and a `--allow-host` list works (SNI and DNS checked).
- Dealbreaker for the main case: the guest is Linux. The host `claude` is a Mach-O binary and fails with `Exec format error`. The Keychain login is not there. Claude Code would need a Linux install (693 MB checkpoint) plus `claude setup-token`, an extra user step. Placeholder handling for that token is untested. Every CLI would need its own Linux install and checkpoint.
- Upside: a fresh guest has no user config, MCP servers or plugins, so the research 25 leaks disappear by construction.
- Seatbelt keeps the host binary and login. Measured: host `claude --version` ran under a profile denying reads of `$HOME` and all network (0.35 s). It is weaker (shared kernel, `sandbox-exec` marked deprecated, but Codex and Claude Code both use it). Domain allow-listing needs a local proxy.
- Cross-meeting Chat: keep one read-only text-only agent view (transcripts and summaries, no video or SQLite). No per-conversation copies. Tighter option: a read-only MCP server on host loopback (shuru `expose_host` measured working).
- Recommendation: Seatbelt as the isolation layer, per-CLI flags kept as a second layer, shuru only as a later opt-in. Open follow-up: a vendor-host allow-list proxy, since prompt injection from transcript text is the real threat.
- Not verified: any real Agent run, Keychain access under a deny-default profile, refresh-token rotation in a copied login.

Full findings with sources: [research/26-sandboxed-agent-runs.md](../research/26-sandboxed-agent-runs.md)
