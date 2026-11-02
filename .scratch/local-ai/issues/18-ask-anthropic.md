# Ask Anthropic about Claude Code plan use

Type: task
Status: resolved
Blocked by: 

## Question

Get a written answer from Anthropic (the "permitted authentication methods" contact on code.claude.com/docs/en/legal-and-compliance) on whether an open-source local app may spawn the user's own signed-in `claude -p`, with tools off, for summaries and chat billed to the user's plan. HITL: the user sends the email; record the reply and date here. Not blocking: ADR 0002 ships Claude with a notice meanwhile.

## Answer

Not sending. User decision: Myna is personal software and ships Claude Code as an Agent the same way T3 Code and Conductor do, accepting the terms ambiguity. Fallback if Anthropic objects: disable Claude Code as an Agent; users keep Claude through the Anthropic API as an Endpoint with their own key. [ADR 0002](../../../docs/adr/0002-agents-as-ai-provider.md) updated to match.
