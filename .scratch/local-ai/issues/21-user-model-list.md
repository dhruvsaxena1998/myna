# Curated model list from the user

Type: task
Status: resolved
Blocked by: 

## Question

HITL: the user provides the models for the curated list: per Agent (Claude Code, Codex, OpenCode, Pi) and per cloud Endpoint preset, with their role (main, fast, decision, stt), plus the Decision models. Record the list here as given.

## What the user fills in

One line per model: `provider | exact model id | roles`. Roles: main (Summary, Chat), fast (cleanup, fallback decisions), decision, stt. 2–4 models per provider. Prices, windows and image support are filled by Fill current prices for the curated list, not here.

Installed here (2026-11-01): Claude Code 2.1.316, Codex 0.164.0, OpenCode 2.1.4, Pi 0.85.0.

```
# Agents
claude-code | ... | main, fast
codex       | ... | main, fast
opencode    | provider/model | main, fast
pi          | provider/model | main, fast

# Cloud Endpoints
openai      | ... | main / fast
anthropic   | ... | main / fast
openrouter  | vendor/model | main / fast / decision

# Decision APIs
jev         | ... | decision
openai-decisions | ... | decision

# Cloud STT (post-meeting only)
groq        | ... | stt
openai      | ... | stt
together    | ... | stt
```

## Answer

Given by the user (2026-11-01):

- **Decision models, via OpenRouter** (the only curated part): `~typesafe/jev-latest`, `typesafe/jev-1.14`, `microsoft/microsoft-decision-1`, `openai/gpt-6-luna-decisions`, `cloudflare/clef`, `cloudflare/clef-flash`, `togethercomputer/tev1-4b-experimental`, `liquid/d1.1`, `liquid/d1-ultra`, `perplexity/pplx-decider-v1.2`, `perplexity/pplx-decider-flash-v1.2`.
- **Claude Code, Codex, OpenCode, Pi**: every model the CLI itself offers, not a curated subset.
- **OpenRouter for Summary and Chat**: every model in OpenRouter's live model list; the user picks any.
- Not given: OpenAI and Anthropic Endpoint presets, direct Jev and OpenAI Decisions, cloud STT.

What the CLIs report on this Mac:

- `opencode models`: 451 ids as `provider/model` (openai 18, opencode 40, opencode-go 30, openrouter 363). The `openrouter/*` ones bill the user's OpenRouter key, not a plan. No price or window in the output.
- `pi --list-models`: a table with provider, model, context, max output, thinking and images (providers `openai-codex` and `openrouter` here). The only CLI that reports window and image support.
- `claude`: no list command; `--model` takes an alias for the latest model or a full model name.
- `codex`: no list command in `--help`; `-m` takes any id. The app-server protocol has a model list (research 17), unchecked here.

This overturns "cloud Endpoints and Agents are curated-only" from Curated model list: shape and upkeep. Follow-up: Live model lists for Agents and OpenRouter.
