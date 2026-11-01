# Cost and context-window display

Type: grilling
Status: resolved
Blocked by: 04

## Question

Settled by [Fitting long meetings into the context window](11-long-meetings-context.md): curated models carry a researched price and context window; local models show neither unless the user enters one.


How does Myna show tokens, cost and context use for Chat and Summary when cost comes from responses: what shows for Endpoints that return no cost, for Agents, and before the first response?

## Answer

- **Where**: Chat footer running total ("≈ 18K tokens · $0.02"); a line under each Summary ("Claude Sonnet via OpenRouter · 14K tokens · $0.02"); the meeting inspector total ("AI for this meeting: $0.03"), which includes background work (cleanup, Decision tasks). No global dashboard in Phase 1.
- **Cost, first match wins**: the response's own cost (OpenRouter `usage.cost`); curated price × reported tokens; curated price × estimated tokens (characters ÷ 4, shown with ≈); otherwise "cost unknown" (off-device Endpoint with no curated price). Local Endpoints show "Free · runs on your server".
- **Agents show what the request is worth at API prices**, as T3 Code and OpenCode do: the CLI's reported cost if it gives one, else curated price × tokens, labelled so it reads as value, not a charge (e.g. "≈ $0.04 at API prices · on your Claude plan").
- **Context in Chat**: a meter beside the token count ("12K of 200K context"), estimated before the first send and exact after each reported reply. At **60%** of the window a compact **Compact** button appears; it replaces older turns with a short summary of the chat so far. No meter for local models with no known window.
- **Warnings**: none and no caps in Phase 1, except the Regenerate button shows the estimate when a single Summary is estimated above $1.
- **Amends [Fitting long meetings into the context window](11-long-meetings-context.md)** (user decision): Summary is always chunk-then-merge, transcript chunks plus screenshot notes per chunk, at any length. Chat never receives the whole transcript; it looks things up through transcript tools: [Transcript tools for Chat](24-chat-transcript-tools.md).
