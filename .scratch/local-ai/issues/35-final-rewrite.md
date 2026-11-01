# Rewrite stories 10, 03, 11, 12, the spec and PRODUCT.md

Type: task
Status: resolved
Blocked by: 

## Question

Fold every decision on this map into stories 10 (AI provider, Summary and Chat), 03 (Models), 11 (Folders) and 12 (Experimental), the spec and PRODUCT.md, so each story has no open questions and is ready to build. "All STT runs on the Mac" becomes "local by default"; the out-of-scope "cloud public on demand" line becomes "Myna-hosted models: later"; the out-of-scope LLM router line goes. Stories link the decision tickets for detail rather than restating them. HITL: the user reviews each document.

## Answer

Reviewed and accepted by the user on 2026-11-01.

- Story 10 is now [AI providers and models](../../myna/stories/10-ai-providers.md); Summary and transcript cleanup split out as [story 16](../../myna/stories/16-summary-and-cleanup.md) and Chat as [story 17](../../myna/stories/17-chat.md), so each is buildable on its own.
- [Story 03](../../myna/stories/03-models.md) drops Gemma and adds meeting speech-to-text on an Endpoint; Parakeet stays the default and custom models ship in Phase 1 under Advanced (from the spec).
- [Story 11](../../myna/stories/11-folders.md) routes folder suggestion to the Decision provider and adds per-folder template and cleanup prompt; [story 12](../../myna/stories/12-experimental.md) keeps only Uhm, triage and screenshot relevance. Stories 04 and 08 carry amendment notes.
- Spec: "local by default", rewritten AI and Experimental user stories (new 65), pipelines, AI provider and chat, storage, settings, testing and out-of-scope lines. PRODUCT.md matches.
