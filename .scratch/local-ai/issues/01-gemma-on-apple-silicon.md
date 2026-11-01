# Gemma on Apple Silicon

Type: research
Status: resolved
Blocked by: 

## Question

How can a Rust/Tauri 2 app run Gemma on Apple Silicon (macOS 14.2+)? Compare MLX (mlx-rs, mlx-swift via the existing swift-rs bridge, mlx_lm.server), llama.cpp (llama-cpp-2 crate, llama-server) and a managed local server. For current Gemma variants: quantised download sizes, RAM needed, tokens/s on M1–M4, context length, image input, license and Hugging Face repos. Also: how does Meetly run its local LLM? The user leans towards MLX.

## Answer

- Gemma 4 (E2B, E4B, 12B, 26B A4B, 31B) is current, Apache 2.0 on Google's ungated HF repos, with image input on all sizes and audio on E2B, E4B and 12B. Context is 128K (E2B, E4B) or 256K.
- Q4 downloads: E2B 3.4 GB, E4B 5.2 GB, 12B 7.0 GB, 26B A4B 14.4 GB, 31B 17.7 GB, plus a 0.2 to 1.2 GB vision projector for GGUF. KV cache memory was not found.
- mlx-rs has no Gemma (its mlx-lm crate covers Llama and Qwen3 only). mlx-swift-lm has Gemma 4 LLM and VLM code, but its Metal shaders need Xcode or xcodebuild to build.
- llama-cpp-2 has Metal and an `mtmd` multimodal feature, and loads Google's QAT GGUFs. The only Gemma 4 speed data is community M4 Max: MLX 113.5 vs llama.cpp 80.5 tok/s on E4B. Nothing covers M1 to M3.
- Meetily (Zackriya) uses Ollama as its recommended local route, plus a built-in llama-cpp-2 child process (`llama-helper`, JSON over stdio) with Qwen 3.5 defaults and Gemma 3 as legacy. It does not use MLX.

Details and sources: [research/01-gemma-on-apple-silicon.md](../research/01-gemma-on-apple-silicon.md)
