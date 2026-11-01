# 11: Folders

**Status:** draft
**Spec stories:** 62–64
**Depends on:** 05 (suggestion needs 10; per-folder template and cleanup prompt need 16)

Flat folders in the Meetings sidebar, an Inbox for unfiled meetings, an optional folder suggestion, and per-folder AI defaults.

## Stories
- As a user, I want to create, rename and delete folders and move meetings into them.
- As a user, I want unfiled meetings in an Inbox.
- As a user, I want a suggested folder for a new meeting, one click to accept.
- As a user, I want a folder to set its own Summary template and transcript cleanup prompt.

## Decisions (detail in the linked tickets)
- Folder suggestion is a Decision task: meeting title, opening transcript lines and folder names in, one folder or "none" out, shown as a chip until accepted or dismissed; routed to the Decision provider, else the Fast model; its own toggle; off-device pill when routed off this Mac: [Decision provider shape](../../local-ai/issues/08-decision-provider-shape.md).
- Per-folder Summary template and cleanup prompt fall back to the global ones; the folder at processing time decides; moving a meeting later never re-runs anything, a chip offers "Regenerate with <folder> template?": [Summary templates and chat persistence](../../local-ai/issues/33-summary-templates-chat-persistence.md).

## Done when
- [ ] Drag-and-drop and a Move to menu
- [ ] Deleting a folder returns its meetings to Inbox
- [ ] Suggestion only when a Decision provider or Fast model is set and the toggle is on; a failed call shows Retry
- [ ] A folder's template is used for a new meeting in it; moving a meeting shows the regenerate chip
- [ ] Checked on the real app from a fresh install

## Open questions for you
- none
