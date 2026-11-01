# LLM cleanup for dictation and transcripts

Type: grilling
Status: resolved
Blocked by: 

## Question

What LLM cleanup does to dictation and to a meeting transcript, which providers may run it, the prompt and its inputs, the timeout and fallback for dictation, what "per chunk" means for transcripts, and how the result stays faithful to what was said (no rephrasing, answering, dropping or reordering). Also: what is stored, what the user sees, and defaults.

## Notes from grilling

- Round 1: Q1, Q3–Q7 taken as recommended (no objection). Q2 taken as recommended (dictation cleanup Endpoint-only; transcript cleanup any kind), with the user's clarification: the dictation pipeline is Uhm (if on) → STT → rules → LLM cleanup, and LLM cleanup has its own 3 s budget, not shared with the earlier steps.
- Q8 overturned: prompts for dictation and transcript cleanup are user-editable in Phase 1, and app-aware styles (code vs email) are Phase 1 too.
- Round 2 (Q9–Q14): all recommendations accepted.

## Answer

> Amended by [Summary templates and chat persistence](33-summary-templates-chat-persistence.md): folders can set their own transcript cleanup prompt, falling back to the global one.

> Amended by [Speech-to-text through an Endpoint](13-cloud-stt-endpoint.md): no dictation cleanup toggle; a separate "Dictate with cleanup" hotkey runs it (unassigned until it has an Endpoint), the plain hotkey never does. Styles apply only to that hotkey.


- Dictation: Uhm (if on) → STT → rules → LLM cleanup → paste. Cleanup fixes punctuation, capitals, grammar slips, missed fillers and self-corrections ("Tuesday, no wait, Wednesday" → "Wednesday"); never rephrases, shortens or replies.
- Dictation cleanup runs only on an Endpoint (Fast model, or a dictation override when the Fast model is an Agent; otherwise the toggle shows "Needs an Endpoint"). Its own 3 s budget starts when the hotkey is released; on timeout or error Myna pastes the rules output and the menubar briefly marks "pasted without cleanup".
- Dictation guard: the cleaned text is replaced by the rules output if it reads as a reply ("Sure, here's…", an answer to a dictated question). With the default prompt, also if length changes by more than about 40% or most words are new. Dictated text goes in data tags, never as instructions.
- Transcript: after the full transcription pass and before Summary, per transcription chunk. Lines go out with ids plus neighbouring lines for context; the model returns `{id: text}`; only sent ids are replaced, and missing, extra or drifted lines keep their original. Fixes punctuation, capitals, fillers and clear mishearings (dictionary, speaker names, title); never merges, splits, drops, reorders, or changes speaker or time. Self-corrections stay. Any provider kind, Agents included. Every guard applies whatever the prompt says.
- Collapsed possibly-muted segments are never sent; a segment restored later stays uncleaned.
- Raw and cleaned text both stored per line. The transcript shows cleaned with "Show original", changed words marked on hover; search covers both; copy and export follow what's shown.
- Prompts are editable: the instruction text only, inside a fixed wrapper (data tags, dictionary, line-id JSON for transcripts). "Reset to default"; an edited prompt is kept when Myna ships a new default, with "A newer default is available".
- Styles: named dictation prompts bound to apps, chosen by the frontmost app at hotkey press, Default otherwise. Ships Default, Code, Message and Email; editable, addable, apps reassignable; match by app only (no browser tab URL), and the app name is never sent. With dictation cleanup off, Styles do nothing.
- One editable transcript prompt for all meetings; per-folder prompts wait for Summary templates.
- Both toggles leave Experimental: dictation cleanup and Styles under Dictation, transcript cleanup and its prompt under Meetings. Off by default, need an AI provider, carry the off-device pill when not local; cost counts toward the meeting total.
