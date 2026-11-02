# Usage, cost and context window per response

Type: research
Status: resolved
Blocked by: 

## Question

What does each Endpoint return per response, streamed and not: token usage, cost, model id? Cover OpenRouter (usage accounting), OpenAI, Anthropic's OpenAI-compatible endpoint, Ollama, LM Studio, oMLX. Which of them expose a model's context window, and from where (a /v1/models field, a model-info endpoint)? The user wants session cost summed from responses, not from a maintained price list.

## Answer

- Only OpenRouter returns cost in the response (`usage.cost`, plus `usage.cost_details.upstream_inference_cost`). OpenAI, Anthropic compat, Ollama, LM Studio and oMLX return tokens only, so cost there needs an outside per-token price.
- Common fields are `usage.prompt_tokens`, `completion_tokens` and `total_tokens`. Treat `*_details` as optional (Anthropic's are always empty, Ollama has only `cached_tokens`).
- Streaming usage needs `stream_options: {"include_usage": true}` on OpenAI, Ollama, LM Studio (0.3.18+) and oMLX. It arrives as a last chunk with an empty `choices` array. OpenRouter always sends it, and a cancelled stream may never deliver it.
- Context window has no shared source. OpenRouter `/models` has `context_length`, oMLX `/v1/models` has `max_model_len`, Anthropic native `/v1/models` has `max_input_tokens`, Ollama needs `/api/show`, LM Studio needs its native REST `max_context_length`, and OpenAI exposes none.
- LM Studio and oMLX report the configured or loaded length, not the model maximum.

Full findings with sources: [research/04-per-response-usage.md](../research/04-per-response-usage.md)
