# Myna

A macOS app with two modules, Dictation and Meetings, that turns speech into text and meetings into transcripts, summaries and chat, on the Mac by default.

## AI

**AI provider**:
Where Myna sends text for an LLM to answer: summaries, chat, cleanup, and classification prompts. It is an Endpoint, an Agent or a Decision API; Myna runs no LLM itself.
_Avoid_: cloud provider, LLM provider, backend

**Endpoint**:
Any OpenAI-compatible URL the user points Myna at, whether it is a cloud service (OpenAI, Anthropic, OpenRouter) or a server the user runs themselves (Ollama, LM Studio, oMLX). Myna does not run or manage what is behind it.
_Avoid_: cloud provider, local server, custom provider, built-in model

**Agent**:
A coding-agent CLI the user has installed and signed in to (Claude Code, Codex, OpenCode, Pi), which Myna runs headless under a macOS sandbox with its own tools off, so the work uses the user's own plan.
_Avoid_: ACP, subscription, harness, CLI provider

**Decision API**:
A typed classification service with its own wire format (Jev, OpenAI Decisions, or OpenRouter's decision models), used only for Decision tasks.

**Decision task**:
A short classification Myna asks while processing: folder suggestion, muted-segment triage, screenshot relevance.

**Preset**:
A named Endpoint with its base URL filled in.

**Main model**:
The AI provider and model that Summary and Chat use unless a feature overrides it.

**Fast model**:
The AI provider and model for quick text jobs (cleanup), and for Decision tasks when no Decision provider is set.
_Avoid_: cheap model, small model

**Decision provider**:
Where Myna sends short classification questions (keep this screenshot? include this segment? which folder?). It is a Decision API, an Endpoint, or an Agent, and falls back to the Fast model when unset. Has its own page in Settings.
_Avoid_: classifier, router

**Transcript tools**:
The read-only lookups Chat's model uses to see a meeting (search the transcript, read a time range, get screenshots near a time), bound to one meeting. Chat never receives the whole transcript.
_Avoid_: RAG, retrieval, MCP tools

**Summary template**:
The named instructions that shape a meeting's Summary (Recap, Action items, Decisions, or the user's own). A folder can set its own.
_Avoid_: prompt, format, preset

**Chunk notes**:
The template-free notes Myna takes from each part of a transcript (points, decisions, action items, with times), which a Summary template turns into the Summary and Chat reads too.
_Avoid_: partial summary

**Chat digest**:
A short summary of earlier Chat exchanges, sent to the model in their place so the conversation stays small. The exchanges themselves stay visible.
_Avoid_: summary, compaction, memory

**Dictation session**:
The part of Myna that runs one dictation, from the shortcut to the paste: the checks before recording, the recording, transcription, rules cleanup, the paste and what the pill shows. It also keeps the speech model loaded between dictations and frees it after the idle time. It reaches the Mac only through one port, so tests run it with a fake.
_Avoid_: dictation flow, dictation handler

**Rules cleanup**:
The deterministic pass that removes filler words, collapses repeated words, writes spoken numbers as digits and applies the user's words to remember. It only deletes listed words, writes a number in digits, respells words the user listed, and repairs the gap left behind; it never adds, reorders or rephrases. It is on by default and the user can switch it off for dictation; words to remember always apply. It needs no AI provider.

**Filler word**:
A hesitation sound with no meaning (um, uh, er, hmm) that rules cleanup removes from dictation. Words like "like", "you know" or "so" are not filler words; rules cleanup leaves them.
_Avoid_: filler for discourse words, disfluency

**Dictation language**:
The one language the user says they dictate in. It decides which filler words and repeats rules cleanup removes; a language without lists gets none removed.
_Avoid_: locale, detected language

**Word to remember**:
A name or special word the user lists so dictation spells it their way, optionally with the wrong form the model tends to write instead (heard as).
_Avoid_: dictionary, personal dictionary, vocabulary, custom words

**LLM cleanup**:
The optional pass after rules cleanup that has a model fix punctuation, capitals and slips in dictation or in a meeting transcript, without changing what was said. In dictation it runs only from the Dictate with cleanup shortcut, never the plain Dictate one.
_Avoid_: polish, rewrite, AI cleanup

**Style**:
A named dictation cleanup prompt tied to a set of apps, used when one of those apps is in front at the moment dictation starts.
_Avoid_: mode, tone, preset

**Off-device**:
True when a request goes to a host other than this Mac's loopback address, including another machine on the LAN. Off-device features carry the "sends text off this Mac" pill.
_Avoid_: cloud, remote

## Updates

**Channel**:
Which releases a Myna install is offered: stable gets stable releases only, beta gets betas and any newer stable release. A new install starts on the channel of the build it came from.
_Avoid_: track, ring, pre-release mode

**Update mode**:
How much Myna does by itself about a new release: check only, check and download, or off (only the "Check for updates" button checks).
_Avoid_: auto-update setting

## Stats

**Stats for nerds**:
An Experimental page that shows, for each dictation, the text after every step of the dictation pipeline and how long each step took, plus per-model and per-rule numbers, so the user can judge which speech model and which cleanup rules serve them.
_Avoid_: stats dashboard, analytics, debug page

**Step**:
One stage a dictation's text passes through on its way to being pasted: speech-to-text, rules cleanup, LLM cleanup or a Style. More steps can be added.
_Avoid_: stage, pass, phase

**Trace**:
The record of one dictation that reached speech-to-text, whether or not it was pasted: the speech model, the app in front, the audio length, how it ended, and the text after each step with the time that step took. It keeps no audio. Voice typing history and Stats for nerds both read it; deleting a history entry deletes its trace.
_Avoid_: log, event, debug info
