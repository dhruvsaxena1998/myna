# 04: Dictation

**Status:** ready
**GitHub:** milestone "04 · Dictation"
**Spec stories:** 6–12, 15
**Depends on:** 01, 03

Hold a hotkey, speak, release; text appears in the focused app.

> Amended by [Speech-to-text through an Endpoint](../../local-ai/issues/13-cloud-stt-endpoint.md) and [LLM cleanup for dictation and transcripts](../../local-ai/issues/28-llm-cleanup.md): Dictation is always local; two hotkeys, Dictate and Dictate with cleanup (LLM cleanup on an Endpoint, 3 s budget, guards, app-bound Styles), replacing a cleanup toggle. Dictation cleanup and Styles leave Experimental.

> Amended by the [rules cleanup map](../../rules-cleanup/map.md): ticket 5 is rules cleanup (filler words, repeats, spoken numbers, Words to remember) and ticket 10 is its settings and history UI.

## Stories
- As a user, I want hold-to-talk on a global hotkey, and a toggle mode, so that short and long dictation both work.
- As a user, I want to change the hotkey in Settings and have it work immediately.
- As a user, I want a small floating pill showing level and state.
- As a user, I want text pasted with my clipboard restored, and a typing fallback for apps that reject paste.
- As a user, I want filler words, stutters and repeats removed and spoken numbers written as digits by rules, and my Words to remember spelled my way.

## Done when
- [ ] Default hotkey works; changing it in Settings takes effect without restart
- [ ] Pill states: idle, recording, processing, error
- [ ] Paste into Notes, a browser field and Terminal; clipboard restored
- [ ] Rules cleanup and Words to remember covered by table tests on the model fixtures
- [ ] Checked on the real app from a fresh install

## Decisions
- Default hotkey is Option+Space, through tauri-plugin-global-shortcut. Onboarding (story 02) will ask which key to use, fn included.
- Tap to start and stop is capped at 5 minutes, which fits one Parakeet call with no chunking. The pill counts down the last 30 s, then stops and pastes.
- Tap mode has no Done button (04.3): a click on the pill would activate Myna and take focus from the app being typed into. Pressing the shortcut again stops it.
- The speech-to-text model unloads after 5 idle minutes by default, set under AI & privacy since meetings share it.
- Microphone and Accessibility permissions are rows on the Voice typing page (state, and Allow opens the System Settings pane). Onboarding reuses the same rows.

## Tickets
1. End to end: hold Option+Space → mic via cpal, resampled to 16 kHz mono → active speech-to-text model through `runtime::transcriber` → clipboard paste (arboard plus synthetic Cmd-V), clipboard restored. Voice typing page gets the Microphone and Accessibility rows. Command-layer test: a WAV plus the Fake transcriber gives the pasted text. The model unloads when idle (AI & privacy: right after use, 1, 5 (default), 15 or 60 minutes, or never)
2. Pill: a floating HUD that never takes focus, showing Listening with level bars, Writing…, Added, and errors (no model, no permission, nothing heard). Style (Capsule, Orb, Waveform, Card) and position (bottom centre, top centre, bottom right, top right) are picked on Voice typing
3. Shortcut and mode on Voice typing: the module's on/off switch, a shortcut recorder that re-registers without a restart, Hold to talk or Tap to start and stop, the 5-minute cap with its countdown
4. fn as a shortcut: event tap on modifier changes, plus a check of the 🌐-key setting with the steps to set it to Do Nothing
5. Rules cleanup: pure functions with table tests, between speech-to-text and paste (detail below)
6. Typing fallback: paste method setting (paste, or type it out) and paste delay; checked in Notes, a browser field and Terminal
7. Dictate with cleanup hotkey: starts unassigned and shows "Needs an Endpoint". Blocked by story 10
8. Fresh-install check (human)
9. Pill under the text cursor: a fifth position read through Accessibility, falling back to bottom centre where the app doesn't report a cursor
10. Your words on Voice typing, blocked by 5: the "Your words" group (Language you speak, Tidy up as I talk), heard-as aliases on the Words to remember chips, raw text in Voice typing history with "Copy what I said" (detail below)

## Open questions for you
- none

## Ticket 5: Rules cleanup

Decided in the [rules cleanup map](../../rules-cleanup/map.md); each point links the ticket that holds the why.

**Where it runs.** `dictation::text` today only joins the model's segments. After joining: `rules::clean(raw, &Settings) -> String`, where the settings are Dictation language, the Tidy up switch and Words to remember. Pure, no I/O, in its own `rules` module. The raw text and the cleaned text both go to Voice typing history. If the cleaned text is empty, it's "nothing heard": the pill shows the error, nothing is pasted, nothing is saved.

**Contract** ([How the rules work](../../rules-cleanup/issues/09-how-rules-work.md)):
- Precision over recall: a missed filler is fine, a deleted meant word is not.
- Four operations only: delete a listed word, write a spoken number as digits, respell from Words to remember, repair the seam (spacing, commas, sentence-start capital). Never add, reorder or rephrase.
- Works on the joined text split into words with punctuation as separate tokens; patterns read about ±2 tokens; no tagger, no model.
- Order: fillers → repeats → numbers → Words to remember → tidy-up.
- Idempotent: `clean(clean(x)) == clean(x)`, checked over every fixture.
- Lists are compiled in.

**Language and switch** ([Rules on non-English dictation](../../rules-cleanup/issues/06-non-english.md), [Rules cleanup switch](../../rules-cleanup/issues/07-rules-switch.md)):
- Fillers, repeats and numbers run only when the Dictation language is English and Tidy up as I talk is on (the default).
- Words to remember always run, in every language, switch on or off.

**Filler words** ([Filler list](../../rules-cleanup/issues/03-filler-list.md)):
- English list: um, uh, uhm, er, erm, hmm, mm, mhm, ah, oh. Any case, any number of repeated letters ("Ummm", "errr").
- "oh" and "mm" stay when the word next to them is a number (word or digits): "four oh four", "5 mm".
- Never touched: uh-huh, mm-hmm, uh-uh; like, you know, I mean, so, well, basically, actually, right.
- Commas on either side of a removed word go with it; a full stop or question mark never does. A sentence of only filler words goes with its full stop. The next word is capitalised when a sentence-initial filler is removed.

**Repeats** ([Repeats and stutters](../../rules-cleanup/issues/04-repeats-and-stutters.md)):
- Collapse-list: I, we, you, she, they, it, my, our, your, the, a, an, to, of, in, on, at, for, with, and, but, or, if, can, would, should. A run of a listed word collapses to its first copy, case-insensitive.
- Two-word repeats collapse when their first word is listed ("I think I think"). Longer repeats never.
- Commas between copies go; a full stop or question mark between them blocks the collapse.
- A token ending in a hyphen that starts the next word is deleted ("th- the" → "the"), for any word.

**Numbers** ([Spoken numbers](../../rules-cleanup/issues/11-spoken-numbers.md)):
- `text2num` 2.8.0 (MIT, no AI), English, threshold 10: zero to nine on their own stay words.
- Guard: skip a match that has another number or "point" next to it, and "a hundred"; those stay words (LLM cleanup handles dates, times, phones, versions and decades).

**Words to remember** ([What a word to remember does](../../rules-cleanup/issues/05-words-to-remember.md)):
- Case-insensitive, whole words; multi-word entries match as a phrase; spaces and hyphens inside a match are ignored ("post gres" → "Postgres"); a trailing 's is kept. The entry's spelling always wins, sentence start included ("iPhone").
- Heard-as alias `Drove → Dhruv`: replaces the heard form, matched exactly as typed (case-sensitive).
- No fuzzy or sound-alike matching. No entry limit.
- Whisper also gets the entries as its initial prompt (`set_initial_prompt`), newest first, as many as fit in about 224 tokens. Parakeet and Apple Speech take none.

**Table tests.** One row per case; inputs are the fixtures in `.scratch/rules-cleanup/fixtures/{parakeet,whisper,apple}/` plus these:

| In | Out | Why |
|---|---|---|
| Uh so I think we should uh ship it on Friday. | So I think we should ship it on Friday. | fillers, capital |
| Um, so we ship. | So we ship. | comma goes |
| I think, uh, we should | I think we should | both commas go |
| It's very good, uh. | It's very good. | full stop stays |
| Hmm, um. | (nothing heard) | only filler words |
| Ummm, errr, ok | Ok | stretched spellings, capital |
| Error four oh four. | Error four oh four. | oh next to a number |
| It's 5 mm wide. | It's 5 mm wide. | mm next to a number |
| Uh-huh, that works. | Uh-huh, that works. | yes/no sound |
| It was like really fast, you know. | It was like really fast, you know. | discourse words stay |
| I I think the the build is broken | I think the build is broken | listed repeats |
| I, I, think the, the build | I think the build | Apple's commas |
| I think uh I think it works. | I think it works. | filler, then two-word repeat |
| We should, we should talk. | We should talk. | two-word repeat |
| He had had enough. I know that that is true. | He had had enough. I know that that is true. | unlisted repeats stay |
| I know that. That is true. | I know that. That is true. | full stop blocks |
| It's very very good. Bye bye. | It's very very good. Bye bye. | unlisted |
| one two one two | one two one two | first word unlisted |
| th- the build | the build | cut-off |
| e-mail me | e-mail me | not a cut-off |
| We need twenty five servers. | We need 25 servers. | number |
| about fifteen percent more | about 15 percent more | number |
| give me five minutes | give me five minutes | under threshold |
| March third twenty twenty seven at four thirty pm | March third twenty twenty seven at four thirty pm | guard: adjacent numbers |
| version two point four point one | version two point four point one | guard: point |
| a hundred things | a hundred things | guard |
| the one I want | the one I want | not a number |
| post gres and kubernetes's pods | Postgres and Kubernetes's pods | Words to remember |
| iphone is great | iPhone is great | entry spelling wins |
| Ask Drove to deploy. I drove home. | Ask Dhruv to deploy. I drove home. | alias, case-sensitive |
| Ich glaube, er, wir sollten das das machen. (German) | unchanged | not English |
| Uh I I think (switch off) | Uh I I think | switch off |
| kubernetes (switch off) | Kubernetes | Words to remember always |

**Not here:** the settings rows and history UI (ticket 10), LLM cleanup's prompt (story 16), multilingual lists (Phase 2), meeting transcripts.

## Ticket 10: Your words on Voice typing

Built to `docs/index.html` (Settings → Voice typing, Voice typing history).
- **"Your words" group**, above Words to remember:
  - **Language you speak**: a select, pre-filled from the Mac's language, listing the languages the active model understands. Help: "Myna tidies up English. In other languages it keeps your words as said." Apple Speech listens in this language (story 03).
  - **Tidy up as I talk**: a switch, on by default. Help when on: "Removes um and uh and repeated words, and writes numbers like 25 as digits." When off: "Pastes exactly what the model heard. Words to remember still apply."
- **Aliases**: typing `Drove -> Dhruv` in the Words to remember box makes a chip that reads **Drove → Dhruv**. Help adds: "If Myna keeps mishearing a word, type it like Drove -> Dhruv."
- **History**: each entry stores the raw text beside the cleaned text. "Copy what I said" (mic icon) sits next to "Copy again", only when the two differ.
