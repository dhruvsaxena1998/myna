# 16: Summary and transcript cleanup

**Status:** draft
**Spec stories:** 43–45, 52, 57
**Depends on:** 09, 10

After a meeting is transcribed, Myna can clean the transcript and writes a Summary through the Main model, using the meeting's kept screenshots when the model reads images.

## Stories
- As a user, I want a Summary written automatically when a meeting finishes processing, in my chosen **Summary template**, with timestamps that jump to the transcript.
- As a user, I want to regenerate with another template, model or edited prompt, keep every version, and edit the text myself.
- As a user, I want a folder to set its own Summary template and transcript cleanup prompt.
- As a user, I want optional **LLM cleanup** of the transcript that fixes punctuation and mishearings without changing what was said, with "Show original".
- As a user, I want kept screenshots used in the Summary when my model reads images.

## Decisions (detail in the linked tickets)
- Templates, chunk notes, auto-run, versions and edits, per-folder defaults: [Summary templates and chat persistence](../../local-ai/issues/33-summary-templates-chat-persistence.md).
- Always chunk-then-merge along transcription chunks; only included lines are sent: [Fitting long meetings into the context window](../../local-ai/issues/11-long-meetings-context.md), [Cost and context-window display](../../local-ai/issues/10-cost-and-context-display.md).
- Transcript cleanup per chunk by line id, guards, raw kept, collapsed segments never sent, editable prompt in a fixed wrapper, off by default under Meetings: [LLM cleanup for dictation and transcripts](../../local-ai/issues/28-llm-cleanup.md).
- Screenshots ride with their chunk (6 per chunk, manual captures first); image support from server metadata or a Test images check; "Use screenshots in Summary and Chat" off by default: [Screenshots in AI](../../local-ai/issues/32-screenshots-in-ai.md).
- Cost line under each Summary version; Regenerate shows the estimate above $1.

## Done when
- [ ] With Main "Not set" the tab says "Set a Main model to summarize" and nothing runs
- [ ] A finished meeting gets a Recap automatically; switching template re-runs only the merge step (tested by request count against a stub)
- [ ] Versions list, delete a version, edit creates an "Edited" version that Regenerate never overwrites
- [ ] Collapsed possibly-muted segments never appear in any request (tested)
- [ ] Transcript cleanup: drifted or missing lines keep the original; "Show original" works; search covers both
- [ ] A non-image model shows "Screenshots not used: <model> doesn't read images"
- [ ] Checked on a real meeting with a local Endpoint and with OpenRouter, on the real app from a fresh install

## Open questions for you
- none
