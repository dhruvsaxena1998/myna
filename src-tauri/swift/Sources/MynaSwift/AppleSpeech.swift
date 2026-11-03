// Apple Speech: SpeechTranscriber (macOS 26+) in the Mac's language.
// Each function blocks its caller and returns a JSON C string from strdup; Rust frees it.

import AVFoundation
import Foundation
import Speech

private final class Box: @unchecked Sendable { var value = "" }

/// Runs async work to completion. Safe on the main thread: the work runs on Swift's pool, not the main actor.
private func block(_ work: @escaping @Sendable () async -> String) -> UnsafeMutablePointer<CChar> {
    let done = DispatchSemaphore(value: 0)
    let box = Box()
    Task {
        box.value = await work()
        done.signal()
    }
    done.wait()
    return strdup(box.value)
}

private func json(_ value: Any) -> String {
    String(decoding: (try? JSONSerialization.data(withJSONObject: value)) ?? Data("{}".utf8), as: UTF8.self)
}

@available(macOS 26, *)
private func locale() async -> Locale? {
    // shortcut: a Mac without SpeechTranscriber support reads as an unsupported language; split it if Apple ships such Macs.
    guard SpeechTranscriber.isAvailable else { return nil }
    return await SpeechTranscriber.supportedLocale(equivalentTo: Locale.current)
}

/// `{"language": "English (India)" | null, "installed": bool}`
@_cdecl("myna_apple_stt_status")
public func appleSttStatus() -> UnsafeMutablePointer<CChar> {
    guard #available(macOS 26, *) else { return strdup(json(["language": NSNull(), "installed": false])) }
    return block {
        guard let loc = await locale() else { return json(["language": NSNull(), "installed": false]) }
        // AssetInventory.status says "supported" even for preinstalled English, so ask installedLocales.
        let installed = await SpeechTranscriber.installedLocales.contains { $0.identifier(.bcp47) == loc.identifier(.bcp47) }
        let name = Locale.current.localizedString(forIdentifier: loc.identifier) ?? loc.identifier
        return json(["language": name, "installed": installed])
    }
}

/// 16 kHz mono samples in; `{"segments": [{"start", "end", "text"}]}` or `{"error": "…"}` out.
@_cdecl("myna_apple_stt_transcribe")
public func appleSttTranscribe(_ samples: UnsafePointer<Float>, _ count: Int) -> UnsafeMutablePointer<CChar> {
    guard #available(macOS 26, *) else { return strdup(json(["error": "Apple Speech needs macOS 26."])) }
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    return block {
        do {
            return json(["segments": try await transcribe(audio)])
        } catch {
            return json(["error": error.localizedDescription])
        }
    }
}

private struct Failure: LocalizedError {
    let errorDescription: String?
    init(_ s: String) { errorDescription = s }
}

@available(macOS 26, *)
private func transcribe(_ audio: [Float]) async throws -> [[String: Any]] {
    guard let loc = await locale() else { throw Failure("Apple Speech doesn't support your Mac's language.") }
    let transcriber = SpeechTranscriber(locale: loc, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    let have = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!
    let want = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) ?? have
    let input = try convert(pcm(audio, have), to: want)

    let results = Task {
        var out: [[String: Any]] = []
        for try await r in transcriber.results {
            let text = String(r.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
            if !text.isEmpty {
                out.append(["start": r.range.start.seconds, "end": r.range.end.seconds, "text": text])
            }
        }
        return out
    }
    let (stream, feed) = AsyncStream<AnalyzerInput>.makeStream()
    feed.yield(AnalyzerInput(buffer: input))
    feed.finish()
    if let end = try await analyzer.analyzeSequence(stream) {
        try await analyzer.finalizeAndFinish(through: end)
    } else {
        await analyzer.cancelAndFinishNow()
    }
    return try await results.value
}

private func pcm(_ audio: [Float], _ format: AVAudioFormat) -> AVAudioPCMBuffer {
    let buf = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(max(audio.count, 1)))!
    buf.frameLength = AVAudioFrameCount(audio.count)
    audio.withUnsafeBufferPointer { buf.floatChannelData![0].update(from: $0.baseAddress!, count: audio.count) }
    return buf
}

private func convert(_ buf: AVAudioPCMBuffer, to format: AVAudioFormat) throws -> AVAudioPCMBuffer {
    if buf.format == format { return buf }
    guard let converter = AVAudioConverter(from: buf.format, to: format) else { throw Failure("Apple Speech can't take this audio.") }
    let frames = AVAudioFrameCount(Double(buf.frameLength) * format.sampleRate / buf.format.sampleRate) + 1024
    let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)!
    var given = false
    var error: NSError?
    converter.convert(to: out, error: &error) { _, status in
        if given {
            status.pointee = .endOfStream
            return nil
        }
        given = true
        status.pointee = .haveData
        return buf
    }
    if let error { throw error }
    return out
}

/// Asks macOS to download the language's model, calling `progress(ctx, 0…1)` along the way. `{}` or `{"error": "…"}`.
@_cdecl("myna_apple_stt_download")
public func appleSttDownload(_ ctx: UnsafeMutableRawPointer, _ progress: @convention(c) (UnsafeMutableRawPointer, Double) -> Void) -> UnsafeMutablePointer<CChar> {
    guard #available(macOS 26, *) else { return strdup(json(["error": "Apple Speech needs macOS 26."])) }
    nonisolated(unsafe) let ctx = ctx
    return block {
        do {
            guard let loc = await locale() else { throw Failure("Apple Speech doesn't support your Mac's language.") }
            let transcriber = SpeechTranscriber(locale: loc, transcriptionOptions: [], reportingOptions: [], attributeOptions: [])
            guard let req = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else { return "{}" }
            let watch = Task {
                while !Task.isCancelled {
                    progress(ctx, req.progress.fractionCompleted)
                    try? await Task.sleep(for: .milliseconds(250))
                }
            }
            defer { watch.cancel() }
            try await req.downloadAndInstall()
            return "{}"
        } catch {
            return json(["error": error.localizedDescription])
        }
    }
}
