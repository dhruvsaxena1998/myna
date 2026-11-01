# How T3 Code handles chat history, compaction and model handoff

Type: research
Status: resolved
Blocked by: 

## Question

How T3 Code keeps a conversation going: what it stores per thread and resends per turn, whether it summarizes or compacts history itself or leaves that to each CLI (Claude Code, Codex, OpenCode, Pi), and what happens when the user switches the model or the agent mid-thread. The user recalls a handoff document written by the current model and passed to the new one; confirm how that works (who writes it, what it holds, when it runs). Also: how prompt caching behaves across a model or provider switch (vendor docs), so Myna can say what a switch costs. Feeds [Summary templates and chat persistence](33-summary-templates-chat-persistence.md), Q10.

## Answer

- T3 does not resend history. It keeps one native session per agent in the thread and resumes it, so a turn sends only the new message; full history lives in T3's SQLite store. Myna's fresh-run-per-turn Agents have no such session, so Myna must resend history itself.
- Compaction is delegated, not written by T3: it sends `/compact` (Claude, OpenCode, Pi) or `thread/compact/start` (Codex). Only extra: for Claude it can send `/compact` about 2 minutes before the prompt cache expires (above 200k tokens, beta).
- The handoff exists but no model writes it. On the first message after switching agent, T3's code picks saved messages under a budget (default 16,000 tokens): last user, last assistant, first user, then newest first, whole messages only, plus a header with "omitted M items" and a pointer to its thread-read tool. T3's own doc: "a budgeted selection, not an agent-written summary". Reasoning, tool state and attachments are not carried.
- Delivery: Codex gets history items via `thread/inject_items`; the others get it as text before the user message. Returning to an agent with an older session sends only the runs it missed. No room means a fresh session. Fork and merge-back reuse the same machinery.
- Caching (vendor docs): Anthropic prefix cache, 5 min or 1 h TTL, write 1.25x or 2x, read 0.1x (0.05x on Opus/Sonnet 5.5); OpenAI automatic, reads up to 95% off, writes 1.25x only on GPT-5.6+. Neither page says "per model" outright, so treat a model switch as a full miss until measured; a switch to another provider always re-sends uncached. T3 shows no switch cost to the user.
- Source: T3 Code 7195b69 (2026-11-01). Full notes: [research/34-t3-chat-compaction-handoff.md](../research/34-t3-chat-compaction-handoff.md)
