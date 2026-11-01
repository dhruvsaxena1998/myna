# Image input through Agent CLIs

Type: research
Status: resolved
Blocked by: 

## Question

Can each Agent CLI (Claude Code, Codex, OpenCode, Pi) take image input in its headless text mode while every tool stays off, as fixed in ADR 0002? For each: the flag or input format (e.g. `codex exec --image`, `claude -p --input-format stream-json` with image content blocks), limits on image count and size, whether any image path forces file-read tools on, and which models accept images. Myna wants to send up to ~20 downscaled JPEG screenshots for Summary/Chat, and a pair of frames for a keep/drop screenshot relevance check.

## Answer

> Claude's image path verified live in [Screenshots in AI](32-screenshots-in-ai.md): base64 image block over stream-json, tools off, under Seatbelt. Codex needs the prompt before `--image`.

- All four agents take images inline with tools off. No path needs a model-called read tool, except Claude's "give an image path" workflow, which Myna should not use.
- Formats: Claude takes base64 image blocks over `--input-format stream-json`. Codex takes `codex exec --image <file>`, which the CLI reads and inlines. OpenCode takes a `files` entry or `--file`. Pi takes `images` base64 in the RPC `prompt`.
- Claude is the only agent with a count threshold. Past 20 images per request, each side must stay under 3,000 px. The other three have no count cap in the CLI.
- Codex, OpenCode and Pi replace images with a placeholder on non-vision models and still answer, so Myna must check vision support first.
- Unverified: no image call was run. Claude with `--tools ""` and an image block needs one real test, and OpenCode 2.1.4 may not match the 2.2.5 source read.

Research: [19-agent-image-input.md](../research/19-agent-image-input.md)
