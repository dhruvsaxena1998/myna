# 16: Summary and transcript cleanup

**Status:** ready
**GitHub:** milestone "16 · Summary", #218
**Spec stories:** 43–45, 52, 57
**Depends on:** 10

After a meeting is transcribed, Myna can clean the transcript and write a Summary, each through the model the user picked for it.

## Stories
- As a user, I want a Summary written automatically when a meeting finishes processing, in my chosen **Summary template**, with timestamps that jump to the transcript.
- As a user, I want to regenerate with another template, model or edited prompt, keep every version, and edit the text myself.
- As a user, I want optional **LLM cleanup** of the transcript that fixes punctuation and mishearings without changing what was said, with "Show original".
- As a user, I want to pick the model for Summary and for transcript cleanup myself.
- Later (not this story's build): a folder's own Summary template and cleanup prompt (needs 11); kept screenshots used in the Summary (needs 08).

## Decisions (detail in the linked tickets)
- Templates, chunk notes, auto-run, versions and edits, per-folder defaults: [Summary templates and chat persistence](../../local-ai/issues/33-summary-templates-chat-persistence.md).
- Always chunk-then-merge; only included lines are sent: [Fitting long meetings into the context window](../../local-ai/issues/11-long-meetings-context.md), [Cost and context-window display](../../local-ai/issues/10-cost-and-context-display.md).
- Transcript cleanup per chunk by line id, guards, raw kept, collapsed segments never sent, editable prompt in a fixed wrapper, off by default under Meetings: [LLM cleanup for dictation and transcripts](../../local-ai/issues/28-llm-cleanup.md).
- Screenshots ride with their chunk (6 per chunk, manual captures first); image support from server metadata or a Test images check; "Use screenshots in Summary and Chat" off by default: [Screenshots in AI](../../local-ai/issues/32-screenshots-in-ai.md). Deferred with story 08.
- The LLM cleanup prompt asks for dates, times, phone numbers, versions and decades in digits (rules cleanup leaves those as words), and carries one line, "Spell these exactly as written: …", listing Words to remember (an alias's correct form). Styles don't change that line: [rules cleanup map](../../rules-cleanup/map.md).
- Cost line under each Summary version; Regenerate shows the estimate above $1.

Decided 2026-11-17:
- **Design first.** Ticket 1 updates the mockup; it is reviewed before ticket 2 starts.
- **Speaker names are sent to the AI**, as the transcript shows them (Me, Priya, Speaker 2). A Summary that says who owns an action item needs them. Story 09's "never sent" line is dropped.
- **Each feature has its own model.** Settings › Meeting notes has a Summary model row and a Transcript cleanup model row (the Customize rows from [Provider configuration](../../local-ai/issues/09-provider-configuration.md)). Summary defaults to "Uses Main", cleanup to "Uses Fast"; either can be set to any model of any configured provider. An override whose provider is removed turns "Not set" with the notice, never falls back to Main or Fast.
- **Chunks** are fixed 4-minute windows over the included lines' start times (a line belongs to the window its start falls in). A window is split further, at line boundaries, only when it exceeds the model's window: its context from the provider's model list, else 8K tokens, minus 25% held for the reply and minus the prompt. Tokens are characters ÷ 4. Summary and transcript cleanup use the same chunker. One meeting is always chunked, so the Summary always says how many parts it read.
- **Chunk notes are keyed by a hash of their input** (the lines' text and speaker names as sent, the model, the chunk prompt). A changed line, an included line, a cleanup re-run or a speaker rename changes the hash, so the next run re-does only those chunks. No separate invalidation.
- **Merge.** All chunk notes plus the template go to the model in one call. If the notes don't fit its window, they are merged in batches first, then the batches merged (a decision for the rare very long meeting on a small local model).
- **Regenerate** opens a pop-up: template, the template's instruction text (editable for this run only), a model menu (the cached model lists of every configured provider; default is the feature's model), and the estimate when it is above $1. Picking a model there doesn't change the setting. The version keeps the template name, prompt text, provider, model, time, parts and cost.
- **Versions.** Every run adds a version; versions are never overwritten. The newest shows unless the user picks another; the pick is kept. Delete removes a version; the last one can't be deleted. Edit opens the showing version's Markdown; Save adds an "Edited" version.
- **Defaults.** "Summarize when a meeting finishes" on; transcript cleanup off. Templates: Recap (default: summary, decisions, action items with owners), Action items, Decisions, plus the user's own. An edited template or cleanup prompt keeps "A newer default is available" when Myna ships a new default (the cleanup styles pattern in `cleanup.rs`).
- **No transcript search in this story.** Whoever builds transcript search (17 or later) covers raw and cleaned text.
- **Order after Stop:** transcribe → transcript cleanup (if on) → Summary (if on). A failed cleanup leaves the raw text and the Summary still runs; a failed Summary shows its reason with Try again.
- **Later:** screenshots in Summary (needs 08), per-folder template and cleanup prompt with its "Regenerate with Hiring template?" chip (needs 11), Chat's use of chunk notes (17).

## Tickets
Done: 1 (design) and 2 (engine, #216, in 0.0.2-beta.13). Ticket 2 shipped with `summary_auto` defaulting **off**, since nothing in the UI could show or stop it; ticket 4 turns it back on.
Issues: 3 → #219, 4 → #220, 5 → #221, 6 → #222, 7 → #223. Build them in that order.

Added while writing the issues (2026-11-17):
- Feature model overrides are the settings `summary_model` and `transcript_cleanup_model`: missing means Uses Main or Uses Fast, `{"provider", "model"}` means that model, and `"none"` means Not set. One resolver per feature. `ai::remove` sets any override pointing at the removed provider to `"none"` and names it.
- Regenerate's estimate comes from a `summary_estimate` command: characters ÷ 4, plus the prompt, plus 25% for the reply, at the cached list's prices. When the price is unknown, there's no estimate.
- Transcript cleanup lives in `transcript.rs`. It adds the nullable `segment.clean` and `meeting.cleanup` (JSON: provider, model, cost) columns and sends `meeting-cleanup` progress events. The Summary sends `clean` when a line has one.
- Header Copy copies the Summary; the transcript panel's Copy copies what it shows.

1. **Design.** Update `docs/index.html` and `docs/settings.html` to this story: the Summary as the meeting's document with versions, Edit, cost line, progress and empty states; the Regenerate pop-up; the transcript side panel with Show original; Settings › Meeting notes rows for Summary and transcript cleanup. Reviewed before ticket 2 starts.
2. **Summary engine (Rust).** New `summary.rs`. Add-only migrations: `chunk_note` (meeting, input hash, provider, model, text, tokens, cost, created) and `summary` (meeting, template name, prompt text, provider, model, created, parts, cost, text, edited), plus a nullable `meeting.summary` for the version picked. Settings `summary_auto` (default on), `summary_template` (default Recap) and `summary_templates` (shipped and user templates, `text`/`base` like `cleanup_styles`). The chunker over the included lines (`meeting::lines` minus hidden, cleaned text when there is one), the fixed wrapper (data tags, Markdown out, `[hh:mm:ss]` timestamps), chunk notes reused by hash, merge, cost summed with `ai::cost_of`. Auto-run after `MeetingSession::write_up` succeeds, in the background with progress ("part 2 of 6", "writing the Recap"); Main "Not set" runs nothing. Tests with a counting fake `Backend`: switching template re-runs only the merge; a changed line re-runs only its chunk; hidden lines never appear in any request; an edit is never overwritten.
3. **Summary on the meeting page.** The Summary is the document (rendered with markdown-it, links off); the transcript moves to a side panel. `[hh:mm:ss]` opens the panel at that line. Versions menu ("3 versions ▾") with delete, Edit → "Edited" version, cost line under the version, "Summarized in N parts" progress, failure with Try again, "Set a Main model to summarize" empty state. Regenerate pop-up with template, editable prompt, model menu and the estimate above $1. Off-device pill from the routing.
4. **Summary settings.** Settings › Meeting notes: "Summarize when a meeting finishes" switch, Summary model row (Uses Main, or any model; Not set with the notice when its provider goes), Summary templates list and editor (Recap, Action items, Decisions, add your own, Reset to default, A newer default is available), off-device pill. Feature overrides are new settings, not new `route` slots, so removing a provider can name them.
5. **Transcript cleanup engine (Rust).** Nullable `segment.clean` column. Per chunk, lines go out with ids plus the neighbouring lines for context; the model returns `{id: text}`; only sent ids are replaced. A line that is missing, extra, empty, reads as a reply or drifted (the dictation guard, per line) keeps the original. Hidden lines are never sent; a line included later stays uncleaned. The fixed wrapper carries the digits rule and "Spell these exactly as written: …". Settings `transcript_cleanup` (default off) and its prompt (`text`/`base`). Runs before Summary. Tests with a counting fake.
6. **Transcript cleanup UI.** Settings › Meeting notes: the switch, its model row (Uses Fast), the prompt editor, off-device pill. Transcript panel shows the cleaned text with changed words marked (the heard words on hover) and Show original; Copy follows what's shown; the panel shows the cleanup's model and cost.
7. **Fresh-install check (human).** A real meeting with a local Endpoint and with OpenRouter, on the real app from a fresh install.

## Done when
- [ ] With Main "Not set" the tab says "Set a Main model to summarize" and nothing runs
- [ ] A finished meeting gets a Recap automatically; switching template re-runs only the merge step (tested by request count against a stub)
- [ ] Versions list, delete a version, edit creates an "Edited" version that Regenerate never overwrites
- [ ] Collapsed possibly-muted segments never appear in any request (tested)
- [ ] Transcript cleanup: drifted or missing lines keep the original; "Show original" works
- [ ] Summary and transcript cleanup each run on the model picked for them, and Regenerate's model menu doesn't change that setting
- [ ] Checked on a real meeting with a local Endpoint and with OpenRouter, on the real app from a fresh install

## Open questions for you
- none
