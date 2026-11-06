# 12: Experimental features

**Status:** draft
**Spec stories:** 50, 51, 53, 56
**Depends on:** 04, 05, 08, 10

Off-by-default extras that are not reliable enough to be normal settings: Uhm filler detection, muted-segment triage and screenshot relevance. LLM cleanup and screenshots in AI have left this page (stories 04 and 16).

## Stories
- As a user, I want an Experimental section, off by default.
- As a user, I want Uhm filler detection with a Dictation / Meetings switch pair.
- As a user, I want possibly-muted segments triaged by my Decision provider, text only, with each inclusion marked and undoable.
- As a user, I want screenshot relevance to drop uninformative frames, through a provider that reads images.

## Decisions (detail in the linked tickets)
- Muted-segment triage and screenshot relevance are Decision tasks with their own toggles and per-task override; triage includes only at p ≥ 0.8 from a Decision API, marks "included by triage" with one-click undo, and when routed off-device the toggle reads "Sends possibly-muted speech off this Mac": [Decision provider shape](../../local-ai/issues/08-decision-provider-shape.md).
- Screenshot relevance is offered only when the routed model reads images (Agents deferred: [agents map](../../agents-provider/map.md)); manual captures skip it: [Screenshots in AI](../../local-ai/issues/32-screenshots-in-ai.md), [Image input through Agent CLIs](../../local-ai/issues/19-agent-image-input.md).
- Uhm needs macOS 15+ and publishes no accuracy numbers, so it stays here: [Uhm, Voz and cloud STT endpoints](../../local-ai/issues/05-stt-models-and-endpoints.md).
- Laya is not in Phase 1.

## Done when
- [ ] Experimental page renders on a fresh install with every feature off
- [ ] Triage marks and undoes inclusions; collapsed segments are sent only when triage is on (tested)
- [ ] Off-device pills where data leaves
- [ ] Checked on the real app from a fresh install

## Open questions for you
- none
