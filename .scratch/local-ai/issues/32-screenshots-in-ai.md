# Screenshots in AI

Type: grilling
Status: resolved
Blocked by: 

## Question

How meeting screenshots reach the AI. Which features send images (Summary, Chat via `get_screenshots(near)`, screenshot relevance as a Decision task), how many frames go per request and how they are picked, at what size, and what happens when the routed model can't take images. Image support is checked live, not curated ([Live model lists](29-live-model-lists.md)), so: how a local Endpoint model is marked image-capable (probe, `/v1/models` metadata, or the user ticks it), and what a non-vision model shows instead (Agents silently get a placeholder, [ticket 19](19-agent-image-input.md)). Also image cost in the cost display ([ticket 10](10-cost-and-context-display.md)), the off-device pill for images, and whether screenshots taken while muted are ever sent.

Read first: [08](08-decision-provider-shape.md), [19](19-agent-image-input.md), [24](24-chat-transcript-tools.md), [29](29-live-model-lists.md), story 08 (screenshots and video) and story 12 (Experimental).

## Notes from grilling

- Round 1 (Q1–Q9): all accepted. The user asked whether capture follows window focus and proposed capturing only while something is shared, plus a manual capture from the recording pill.
- Round 2 (Q10–Q13, capture): all accepted.
- Facts checked live (`live/32/`, notes in `live/32/notes.md`): Claude Code reads a base64 image block over `--input-format stream-json` with tools off under the ticket 27 Seatbelt profile ($0.0007, haiku), closing ticket 19's open test; Codex `exec "$Q" --image <file>` works too (prompt before `--image`, which swallows later arguments). Ollama `/api/show` lists `"vision"` in `capabilities`; LM Studio `/api/v0/models` gives `type: "vlm"`; oMLX detects VLMs but its API exposure is undocumented.

## Answer

**Into the AI**

- Summary: a chunk's kept frames go in the same call as its transcript lines, to the Summary model; chunk notes carry "On screen [00:12:30]: …". No separate vision pass. At most 6 frames per chunk, chosen by largest visual change spread across the chunk, manual captures first.
- Frames go as captured (1280 px JPEG, about 1.2K tokens each on Claude). No high-detail mode, no crops.
- Chat: `get_screenshots` returns at most 3 kept frames nearest the time; at most 6 images per question; image lookups count toward the 8. Earlier images become stubs ("saw 2 screenshots near 00:12:30"); Chat digests describe what was seen in words.
- A model that can't read images runs text-only with "Screenshots not used: <model> doesn't read images". Never rerouted. Agents count as image-capable only when their model listing says so (three of the four silently swap images for a placeholder).
- Image support for local models: server metadata first (Ollama `capabilities` "vision", LM Studio `type: "vlm"`), else a one-time **Test images** check (bundled image with a known word, cached per model), with a user override "This model reads images: Yes / No". Unknown counts as no.
- Mute doesn't apply to screenshots. Any kept frame may be sent; frames the user deletes, or retention removed, never are. Chat says "Screenshots from this meeting were deleted after 30 days"; Summary keeps its chunk notes.
- "Use screenshots in Summary and Chat" leaves Experimental for the Meetings settings, off by default. Off-device pill reads "Sends text and screenshots off this Mac". The spec's "supports images" checkbox is dropped.
- Image tokens count in the estimate and the context meter: Anthropic ⌈w/28⌉×⌈h/28⌉, OpenAI's own rule (patches or tiles), otherwise Anthropic's; reported usage after the reply.
- Window video, if kept in story 08: a frame every 15 s through the same near-duplicate filter, so the AI side is identical.

**Capture (amends story 08)**

- Capture follows the meeting window, not focus: other apps in front never appear. Another tab in the same browser window would, a minimized window gives no frames. (ScreenCaptureKit window capture; check on a real call in story 08.)
- Modes: Off / **When someone shares** (default) / Every 15 s / Only when I capture. "When someone shares" uses the companion extension's presenting signal for Meet and Teams in Chromium; elsewhere it falls back to every 15 s with the changed-screen filter, and screenshot relevance (if on) drops people-grid frames.
- **Capture** button on the recording pill, plus an optional hotkey: a frame now, always kept (skips duplicate filter and relevance), marked as the user's.
- Automatic capture pauses when the window isn't showing the meeting (active tab from the extension, else the window title), with "Screenshots paused: meeting tab not showing" on the pill; a manual Capture while paused asks "Capture this window anyway?".
- These capture decisions are recorded here for story 08's rewrite; story 08 is outside this map's destination.
