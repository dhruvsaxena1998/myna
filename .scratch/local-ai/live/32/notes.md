# 32 notes
A. run.sh <claude|codex> here; test.png 948x108 (mkimg.py). Outputs in /private/tmp/m32/<cli>/.
Claude haiku (claude-haiku-5-5), stream-json stdin, Seatbelt: works. Answer "LAUNCH NOV 14". usage input 2 + cache_create 2823 + cache_read 649 (~3.5k total incl ~3.3k system prompt; image not separable); cost $0.00071. Formula ceil(948/28)*ceil(108/28)=34*4=136 tokens.
stdin: {"type":"user","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"<b64>"}},{"type":"text","text":"What text is in this image?"}]}}
Codex gpt-6-luna: `codex exec ... "$Q" --image test.png` (prompt BEFORE --image; --image is multi-value and swallows the prompt). Works under profile (needs ~/.codex write). Answer "LAUNCH NOW 14" (my hand-drawn V is ambiguous). input_tokens 9286 (3200 cached), output 9.
B. URLs:
- Ollama: https://github.com/ollama/ollama/blob/main/docs/api.md  /api/show example has capabilities ["completion","vision"]
- LM Studio: https://lmstudio.ai/docs/developer/rest/endpoints  GET /api/v0/models: "type": "vlm" | "llm" | "embeddings"; no capabilities field. /v1/models not documented there.
- oMLX: https://github.com/jundot/omlx  types LLM/VLM/OCR/Embedding/Reranker auto-detected, but docs don't say API exposes it; /v1/models returns alias; admin API schema undocumented.
- Anthropic: https://platform.claude.com/docs/en/build-with-claude/vision  tokens=ceil(w/28)*ceil(h/28); long-edge/token caps: 2576px/4784 (4.7+), 1568px/1568 (others); max 8000x8000, 10MB base64 (5MB Bedrock), 600 imgs/request (100 older? see doc).
- OpenAI: https://developers.openai.com/api/docs/guides/images-vision  tile models (gpt-4o/4.1): low = base only (85), high = base + 170/512px tile after scale (fit 2048, short side 768); patch models: ceil(w/32)*ceil(h/32) * multiplier (1.2 gpt-6-astra), budget 2500 patches high, 512x512 low; reject >30000 patches.
