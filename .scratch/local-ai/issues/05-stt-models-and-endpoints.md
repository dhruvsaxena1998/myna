# Uhm, Voz and cloud STT endpoints

Type: research
Status: resolved
Blocked by: 

## Question

Desert Ant Uhm and Voz: what their Swift SDKs take and return, model sizes, and how Voz compares to Parakeet TDT v3 on Apple Silicon (vendor claims labelled as such). Cloud STT: which services expose an OpenAI-compatible /audio/transcriptions (OpenAI, Groq, others), their file-size and duration limits, timestamp and word-level output, and price per minute.

## Answer

- Voz is NVIDIA Parakeet TDT 0.6B v3 converted to Core ML (337 MB). Same weights as Myna's default, different runtime. API is `try await Voz()` then `transcribe(url)`, returning text, word timestamps (80 ms frames, estimated ends) and a realtime factor.
- Vendor-reported Voz speed is 520x to 828x on M3 Ultra and M5, WER 6.43% against Whisper large-v3-turbo at 7.00%. There is no independent Voz versus parakeet-rs comparison and no Parakeet numbers on the card.
- Uhm (45 MB Core ML) returns filler spans (start, end, confidence, type) from `analyze(audioPath:)`. It publishes no precision or recall.
- Both need macOS 15+ and Swift 6.2+ per the SDK README. Weights download from Hugging Face on first use, under a source-available license.
- Cloud: Groq (100 MB dev tier, about $0.0007 to $0.0019/min) and Together (80 MB, 4 h, $0.0015/min, hosts Parakeet v3) return word timestamps. OpenAI's `gpt-4o-*` models return none, and `whisper-1` is scheduled to shut down 2027-02-26. Fireworks, Mistral and Deepgram are only partly verified.

Full findings: [research/05-stt-models-and-endpoints.md](../research/05-stt-models-and-endpoints.md)
