# Apple's on-device model

Type: research
Status: resolved
Blocked by: 

## Question

What does Apple's on-device model (Foundation Models framework) offer a macOS app: minimum macOS version, hardware and Apple Intelligence requirements, context window, image input, guided generation / structured output for classification, rate or usage limits, and how to call it from Rust through a swift-rs static library.

## Answer

- Foundation Models needs macOS 26.0+, an M1-or-newer Mac, Apple Intelligence enabled, and a supported region and language. Availability is `.available` or `.unavailable(deviceNotEligible | appleIntelligenceNotEnabled | modelNotReady)`.
- With a 14.2 deployment target the linker weak-links the framework on its own (measured). Every use must sit behind `#available(macOS 26.0, *)`. Launch on macOS 14.2 to 25 was not tested.
- Documented context is 4096 tokens (8192 measured on one macOS 27 Mac), so long transcripts need chunk-then-combine. Image input is macOS 27 only. On-device usage is unlimited, with an undocumented background rate limit.
- `@Generable` enums with greedy sampling give a documented classifier, but guardrails still throw on sensitive text. Hindi was not in the supported languages.
- swift-rs gives synchronous `@_cdecl` calls only, so async generation needs a blocking wrapper or callback. The build likely needs full Xcode for the `@Generable` macro.

Full findings: [research/02-apple-on-device-model.md](../research/02-apple-on-device-model.md)
