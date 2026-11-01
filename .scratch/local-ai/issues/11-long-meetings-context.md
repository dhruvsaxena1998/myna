# Fitting long meetings into the context window

Type: grilling
Status: resolved
Blocked by: 

## Question

How do Summary and Chat handle a transcript bigger than the Endpoint model's context window (chunked summaries then merge, retrieval for chat, truncation), given the transcript arrives in chunks from the sibling chunked-transcription map?

## Answer

> Amended by [Cost and context-window display](10-cost-and-context-display.md): Summary is always chunk-then-merge (transcript chunks plus screenshot notes per chunk), never whole. Chat never receives the whole transcript; it uses transcript tools ([Transcript tools for Chat](24-chat-transcript-tools.md)), with the FTS5 passage approach kept as the fallback for models that cannot call tools.


A 1-hour meeting is about 12–13K tokens. Agents and cloud models fit it whole; local servers often load a 4K–32K window and don't.

- **Summary**: send the whole transcript when it fits (window minus 25% held back for the reply, minus the prompt). Otherwise summarize chunk by chunk along the boundaries the chunked-transcription map produces, carrying the previous chunk's notes forward, then merge the notes into the template. Only included transcript lines are sent.
- **Knowing the window: the curated model list.** User decision: Myna ships a curated list of models for cloud Endpoints and Agents, researched and maintained in the repo, carrying each model's context window and price (and whether it takes images). Cloud Endpoints and Agents offer only curated models. Local Endpoints (Ollama, oMLX, LM Studio, custom) accept any model; Myna shows no window or price for them unless the user enters one. Internally it uses the user's value, else what the server reports (oMLX `max_model_len`, LM Studio's loaded length, Ollama `/api/show`), else 8K. A low guess only means earlier chunking. The Ollama preset says how to raise its context length. Tokens are estimated as characters ÷ 4.
- **Chat on a long meeting**: if the transcript fits, send transcript + summary + history. If not, send the summary, the per-chunk notes, and transcript passages matching the question via SQLite FTS5. No embedding model. The oldest turns drop first.
- **Chunk notes are stored** and reused by Chat and by re-runs with the same model. Summary shows a quiet line when it chunked: "Summarized in 4 parts: this meeting is longer than llama3.1:8b's 8K window."
- **Amends [Provider configuration](09-provider-configuration.md)**: cloud Endpoints and Agents no longer offer "All models", only the curated list. **Feeds [Cost and context-window display](10-cost-and-context-display.md)**: cost for curated models comes from the curated price × tokens, so it no longer depends on the Endpoint returning a cost.
