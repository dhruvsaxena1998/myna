# 17: Chat

**Status:** draft
**Spec stories:** 46–48
**Depends on:** 16

One chat per meeting. The model looks things up in the meeting through transcript tools instead of receiving the whole transcript, and older exchanges are folded into Chat digests so the conversation stays small.

## Stories
- As a user, I want to ask follow-up questions about a meeting and see which passages the answer came from.
- As a user, I want my chat kept with the meeting, with Clear chat.
- As a user, I want tokens, cost and how full the context window is shown, with a Compact button when it gets full.
- As a user, I want to switch the Chat model mid-thread and be told what that costs.

## Decisions (detail in the linked tickets)
- Each request: meeting header (facts, Summary version showing, chunk outline) → Chat digests → up to the last 10 exchanges → question. Tools `search_transcript`, `read_transcript`, `get_screenshots`, bound to one meeting, included lines only, about 2K tokens per result, 8 lookups per question; Endpoints via function calling with FTS5 fallback, Agents through Myna's MCP relay: [Transcript tools for Chat](../../local-ai/issues/24-chat-transcript-tools.md).
- Chat digests every 10 exchanges, merged at 3, written in the background by the Chat digest row (inherits Fast); Compact digests early; model switch warns of the uncached resend; a regenerated Summary adds a divider: [Summary templates and chat persistence](../../local-ai/issues/33-summary-templates-chat-persistence.md), with background in [How T3 Code handles chat history, compaction and model handoff](../../local-ai/issues/34-t3-chat-compaction-handoff.md).
- Context meter and Compact at 60%; cost in the footer: [Cost and context-window display](../../local-ai/issues/10-cost-and-context-display.md).
- Screenshots: 3 per lookup, 6 per question, stubbed in later turns: [Screenshots in AI](../../local-ai/issues/32-screenshots-in-ai.md).

## Done when
- [ ] An answer cites "Looked up N passages" with clickable timestamps
- [ ] Requests never contain the whole transcript or collapsed segments (tested against a stub)
- [ ] After 10 exchanges a digest row exists and the next request sends it in place of those exchanges; originals stay visible
- [ ] Switching model shows the resend estimate before the first send; each reply is labelled with its model
- [ ] Works through an Endpoint with tools, a local model without tools (FTS5 fallback), and an Agent under Seatbelt
- [ ] Checked on the real app from a fresh install

## Open questions for you
- none
