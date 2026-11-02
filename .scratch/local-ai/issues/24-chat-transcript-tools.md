# Transcript tools for Chat

Type: grilling
Status: resolved
Blocked by: 25, 26

## Question

Chat never gets the whole transcript; the model asks for it through tools. Which tools (search the transcript, get lines by time or speaker, get the summary, list screenshots?), what each returns and how much, and how they reach each provider kind: Endpoints via function calling, Agents via a read-only Myna MCP server as the only tool source. Does that amend ADR 0002's 'every tool and MCP server off', and what stops the tools from reaching anything beyond this one meeting? What do models without tool calling get (the FTS5 passage fallback)?

## Notes from grilling

- Round 1 settled. Q1: every Chat request carries title, date, duration, speakers, Summary and a chunk outline; transcript and screenshots only via tools. Q3: tools bound to one meeting id, included lines only, read-only; cross-meeting Chat handled separately. Q4: ~2K tokens per tool result, at most 8 lookups per question, UI shows "Looked up N passages" with clickable timestamps. Q5: curated models carry a `tools` flag; local models try once, then fall back to injected FTS5 passages.
- Q2 amended by the user: `get_screenshot(at)` becomes `get_screenshots(near, window?)`, returning the kept frames just before and after a timestamp, since a frame rarely sits on the exact second.
- Q6–Q8 (per-agent tool-off recipes, Codex `apply_patch`, Pi without tools) replaced by the user's idea: run the Agent in a sandbox that holds only this meeting's files. Waiting on research 26 (shuru). Cross-meeting Chat raised again: how a sandbox sees many meetings without copying them each time.
- Isolation decided (after research 26 and a side-by-side chart): Phase 1 runs every Agent under macOS Seatbelt, keeping the user's installed CLI and existing sign-in, with per-CLI flags as a second layer. shuru (Linux microVM) rejected for Phase 1: it can't run the user's macOS CLI or reach its Keychain sign-in, so Myna would have to install each CLI and hold long-lived tokens. Kept as a Phase 2 experimental "Strict mode".
- Round 2 settled (go with rec): Myna's relay over one Unix socket, bound to one meeting id; cross-meeting Chat a later story on the same socket; Codex `apply_patch` confined to the temp dir; Pi refused if user MCP servers load; network open in Phase 1, host allow-list proxy later; ADR 0003 written; live test is ticket 27.

## Answer

- Every Chat request carries title, date, duration, speakers, the Summary and a chunk outline. Transcript and screenshots come only through tools.
- Three tools: `search_transcript(query, speaker?)` (FTS5, with neighbouring lines), `read_transcript(from, to)`, `get_screenshots(near, window?)` (kept frames just before and after a time; image-capable models only).
- Bound to one meeting id, included lines only (collapsed possibly-muted segments never returned), read-only. About 2K tokens per result, at most 8 lookups per question; Chat shows "Looked up N passages" with clickable timestamps.
- Endpoints get the tools through function calling. Curated models carry a `tools` flag; a local model tries once, then falls back to injected FTS5 passages.
- Agents run under Seatbelt with their own tools off; Myna's MCP server is a stdio relay to the app over one Unix socket, and the only tool source. Runs that load anything besides `myna` are refused ([ADR 0003](../../../docs/adr/0003-agents-under-seatbelt.md)).
- Chat across all meetings: a later story, binding a run to the library on the same socket. shuru is Phase 2 "Strict mode" (research 26).
