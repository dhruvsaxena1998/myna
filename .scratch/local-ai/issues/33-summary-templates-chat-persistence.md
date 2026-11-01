# Summary templates and chat persistence

Type: grilling
Status: resolved
Blocked by: 

## Question

How Summary templates work now that Summary is always chunked ([ticket 10](10-cost-and-context-display.md), [ticket 11](11-long-meetings-context.md)): what a template holds (sections, prompt, output shape), where it applies in chunk-then-merge, built-in versus user templates, and per-folder defaults. Per-folder transcript cleanup prompts are decided here too ([ticket 28](28-llm-cleanup.md)). Regenerating a Summary with another template or model: replace or keep versions.

For Chat: what is persisted per meeting (turns, tool calls and their results from the [transcript tools](24-chat-transcript-tools.md), provider and model per turn, cost), what Compact at 60% context does to stored history (summary of earlier turns kept, originals kept or dropped), what is resent on each turn, and what happens when the user switches the Chat model mid-thread or the Summary is regenerated under an existing Chat.

Read first: [10](10-cost-and-context-display.md), [11](11-long-meetings-context.md), [24](24-chat-transcript-tools.md), [28](28-llm-cleanup.md), story 10 (AI summary and chat) and story 11 (Folders).

## Notes from grilling

- Round 1 (Q1–Q11): accepted except Q8, where the user proposed rolling Chat digests, and Q10, where the user asked for research on how T3 Code hands a thread to another model ([How T3 Code handles chat history, compaction and model handoff](34-t3-chat-compaction-handoff.md)).
- Round 2 (Q12–Q16, digests) and Q17 (model switch, after the research): all accepted.

## Answer

**Summary**

- A **Summary template** is a name plus instruction text inside a fixed wrapper (data tags, included lines only, Markdown out, `[00:12:30]` timestamps linking to the transcript). No section schema. Ships Recap (default: summary, decisions, action items), Action items and Decisions; all editable with "Reset to default"; users add more.
- Chunk notes are template-independent (points, decisions, action items, owners, dates, with timestamps) and stored per meeting and model; rebuilt only when the transcript changes (segment included, cleanup re-run, speaker renamed) or the model changes. The template applies only at the merge step, so switching template re-runs only the merge.
- Summary runs automatically after transcription (and cleanup, if on) when a Main model is set; a Meetings toggle, on by default. Main model "Not set" shows "Set a Main model to summarize".
- Folders can set a Summary template and a transcript cleanup prompt, each falling back to the global one ("Uses default: Recap"). The folder at processing time decides. Moving a meeting later never re-runs anything; a chip offers "Regenerate with Hiring template?".
- Every run is kept as a version (template, prompt text, model, time, cost); the tab shows the latest with "3 versions ▾"; versions can be deleted. Editing saves an "Edited" version; a later Regenerate adds a newer version and never overwrites the edit. Chat and export use the version showing.

**Chat**

- One thread per meeting, with Clear chat; never auto-deleted. Each turn stores text, transcript tool calls with arguments and results, provider and model, tokens, cost, time.
- **Chat digests**: every 10 exchanges (question plus reply), those 10 become one digest row; when 3 digests pile up they merge into one. Each request is meeting header (ticket 24, with the Summary version showing) → digests → up to the last 10 raw exchanges → new question, in that fixed order so the prefix stays cached between digests. Earlier tool results go as stubs ("looked up 00:12:00–00:15:00"). Originals stay stored and visible; digests never replace them on screen.
- Digests run in the background after the 10th exchange through a new **Chat digest** row under Customize (inherits the Fast model, overridable), never delaying a reply. On failure the next turn sends raw history and the digest retries. Cost counts toward the Chat total.
- Compact (button at 60% of the window, ticket 10) runs the digest step now on everything except the last 2 exchanges. One mechanism, not two.
- Agents get the full request every turn as a fresh headless run; no CLI session resume (fits ticket 27's no-persistence flags).
- Switching the Chat model is allowed from the footer and applies from the next turn. Before the first send on the new model: "New model: the conversation so far is sent again without cache, about 9K tokens ≈ $0.03". Replies are labelled with their model. If the request doesn't fit the new window, Myna digests first. No model-written handoff document: digests are the handoff (T3 Code uses a budgeted message selection, not a model-written one).
- A regenerated Summary under an existing Chat: the Chat continues, the header uses the version now showing from the next turn, and a "Summary updated" divider marks it.
