# No bundled LLM in Phase 1

Myna does not download or run an LLM for Summary, Chat, LLM cleanup or folder suggestion. Those features stay off until the user points Myna at an Endpoint: a server they run themselves (Ollama, LM Studio, oMLX), listed first, or a cloud service with their key. We considered shipping Gemma 4 through MLX (mlx-swift-lm in a helper process, with memory tiers and pinned weights). We rejected it because the user can already run Gemma, or any other model, behind a local Endpoint, and bundling it would have meant a 3–17 GB download, an Xcode-built MLX bridge nobody had proven through swift-rs, and a managed helper process. The `TextProvider` trait keeps a Built-in route open if this is revisited.

## Consequences

- Summary and Chat no longer work offline on a fresh install. They need a local server the user runs.
- PRODUCT.md's "summaries and chat run on-device by default" becomes "off until you choose an Endpoint, with local ones listed first".
- Myna never installs or manages a local server. It only checks that the URL answers.
