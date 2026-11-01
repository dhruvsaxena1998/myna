# Write the Claude model list

Type: task
Status: resolved
Blocked by: 29

## Question

AFK: write `models/curated.json` holding only Claude Code's model list (aliases and full ids, context window, minimum `claude` version each), bundled and refreshed on launch, as decided in Live model lists for Agents and OpenRouter. Also the one OpenAI Decisions id (`gpt-6-luna`). Prices come from models.dev at runtime, not this file.

## Answer

- `models/curated.json` written: four Claude entries (`id`, `alias`, `name`, `context_window`, `min_claude_version`, `source`), one OpenAI Decisions entry (`gpt-6-luna`), and a top-level `checked` date. No prices; they come from models.dev at runtime ([Live model lists for Agents and OpenRouter](29-live-model-lists.md)).
- Claude entries, all 1M-token windows: Fable 5.1 `claude-fable-5-1` (alias `fable`, `claude` 2.1.257), Opus 5.5 `claude-opus-5-5` (`opus`, 2.1.280), Sonnet 5.5 `claude-sonnet-5-5` (`sonnet`, 2.1.284), Haiku 5.5 `claude-haiku-5-5` (`haiku`, 2.1.293).
- Sources: Claude Code [model config](https://code.claude.com/docs/en/model-config) (aliases, minimum versions, alias resolution on the Anthropic API) and Anthropic [models overview](https://platform.claude.com/docs/en/about-claude/models/overview) (ids, 1M windows). T3 Code's manifest at `pingdotgg/t3code` agrees on ids and minimum versions.
- Older models left out: Claude Code's alias table resolves only to the 5.5 family and Fable 5.1. T3's manifest marks Fable 5, Opus 5 and earlier as `legacy`; they stay usable through the custom model id.
- Aliases follow the Anthropic API only. On Bedrock, Google Cloud and Foundry the same aliases resolve to older models, so a window or version gate may not match there.
- Not verified: the `gpt-6-luna` context window (OpenRouter lists 1,050,000 for its `-decisions` variant; OpenAI's guide gives none, so the file has none); the minimum versions came from docs text and T3, not from running old CLIs; the `best`, `opusplan` and `[1m]` aliases are left out.
