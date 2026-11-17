# 18: Settings v2

**Status:** ready
**GitHub:** milestone "18 · Settings v2", story #199
**Design:** `docs/settings.html` (open it in a browser; the Prototype panel switches theme, accent and scenarios), DESIGN.md › Settings, Grouped list, Toolbar history, Stage path
**Supersedes:** the unslop pass in milestone "UI polish (unslop)", #189 and #190–#198. Their findings are folded into the tickets below.
**Depends on:** nothing new. The Decision model row waits for story 10's Decision provider.

The Settings window is rebuilt around the path your voice takes through Myna. Voice typing becomes three stages, Listen, Understand and Write, followed by Meeting notes, General, AI providers and an opt-in Experimental page. Long lists move one level down into drill-ins, every page gets back and forward, and the row grammar is one shared set of rules instead of copies in each component.

## Stories
- As a user, I want to find a setting by where it acts on my voice, not by guessing which of eight pages owns it.
- As a user, I want long lists (models, words, styles, providers) out of the way until I open them.
- As a user, I want back and forward like any Mac window.
- As a user, I want a different speech-to-text model for meetings than for voice typing.
- As a user, I want experimental features on their own page, only when I've turned them on.

## Done when
- [ ] The sidebar reads Voice typing (Listen, Understand, Write), Meeting notes, General, AI providers, and Experimental while it's on
- [ ] Every setting from the old pages is on its new page (the coverage list below), with the v2 labels
- [ ] ‹ › and ⌘[ ⌘] work across pages and drill-ins
- [ ] The stage path heads the three voice pages with live values
- [ ] Meeting notes has its own speech-to-text model, defaulting to the voice typing one
- [ ] One row grammar in `design/tokens.css`; no component re-declares rows, buttons, pills, inputs or separators
- [ ] No jitter: every page's data is loaded before it paints, and state changes never move rows
- [ ] Dark and light, every accent, 880×520 and the 640×420 minimum, checked in the real app

## Decisions
- **Structure and order** are the user's call (2026-11-11). Decision provider is no longer a page; it becomes the "Decision model" row on AI providers, which ships with story 10's Decision provider ticket.
- **Labels may be technical** where a plain word blurs the meaning. The v2 labels are in `docs/settings.html`. Main renames: How it starts → Activation, Sounds → Start and stop sounds, Language you speak → Spoken language, Tidy up as I talk → Filler and repeat removal, Words to remember → Custom vocabulary, Never remove → Filler exceptions, How words are added → Insert method, Cleanup model → Cleanup model (Fast), Ask how it went → Rating prompt, Start or stop notes → Recording shortcut, Connection code → Pairing code, Telling voices apart → Speaker diarization, Small choices → Decision model, Highlight colour → Accent colour, See-through panels → Window tint, Where your data lives → Data folder, Opacity → Glass opacity. Settings keys don't change, only the words.
- **Meetings speech-to-text model:** a new task row `meeting_stt` in `active_model`. When it's missing, meetings use the `stt` pick, so existing installs behave as before and older builds ignore the row (AGENTS.md › Compatibility). One catalog and one download store serve both. Deleting a model forgets it for both, as `models::forget` already does.
- **Experimental:** the "Use experimental features" switch stays in General. The Experimental page is listed only while it's on. Its rows keep the normal row grammar so they can move to their real page later.
- **History:** one stack across pages and drill-ins. Changing a page from the sidebar pushes it; going back from a drill-in to its parent is ‹. A drill-in whose subject is deleted (a cleanup style) is dropped from the stack.
- **Kept from the unslop issues:** the shared grammar of #198, the Speech models row fix (#193: the narrow-window rules applying at every width), BrowserAddon loading its state before paint (#192), About's selectable data path (#197), one disabled rule (#196), "Coming later" everywhere (#195).

## Coverage (old → new page)
- Look & appearance → General (Theme, Accent colour, Window tint)
- Voice typing → Listen (Use voice typing, Microphone, Dictate, Dictate with cleanup, Activation, 🌐 key row, Start and stop sounds, Pill style and position) · Understand (Spoken language, Filler and repeat removal, Custom vocabulary, Filler exceptions, Suggested fixes) · Write (Insert method, Accessibility, Cleanup model (Fast), Cleanup styles, Rating prompt, Export ratings)
- Speech models → Understand (Speech-to-text model with drill-in, Unload model when idle) · Meeting notes (Speaker diarization drill-in, its own Speech-to-text model)
- Meeting notes → Meeting notes (Use meeting notes, Recording shortcut, Keep my voice when muted, Browser add-on, Pairing code, Microphone, Screen & System Audio Recording)
- AI provider → AI providers (Providers, Add a provider drill-in, Main model, Fast model)
- Decision provider → AI providers › Decision model (with story 10)
- Experimental features → General (master switch) + Experimental page (Liquid Glass, Glass opacity, Stats for nerds)
- About → General (Version, Update mode, Check frequency, Update channel, Data folder, License)

## Tickets
1. (#200) Shared settings grammar in tokens.css (supersedes #198)
2. (#201) Settings shell: new sidebar, back and forward, drill-ins, Experimental listing
3. (#202) Listen page
4. (#203) Understand page and the Speech-to-text drill-in
5. (#204) Write page and the Cleanup styles drill-in
6. (#205) Stage path on the voice pages
7. (#206) A separate speech-to-text model for meeting notes
8. (#207) Meeting notes page
9. (#208) General and Experimental pages
10. (#209) AI providers page and the Add a provider drill-in
11. (#210) Real app check (human)

## Open questions for you
- Close #189 and #190–#198 as superseded once 18.1 lands, or keep them open as references?
