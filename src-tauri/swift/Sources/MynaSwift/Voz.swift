// Desert Ant Voz (Parakeet TDT v3 on the Neural Engine) through its SDK.
// The SDK downloads, verifies and loads the weights in the folder Myna gives it (models/voz).
// Each function blocks its caller and returns a JSON C string from strdup; Rust frees it.

import Foundation
import Voz

private final class Box: @unchecked Sendable { var value = "" }

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

private func say(_ error: Error) -> String { json(["error": error.localizedDescription]) }

/// The loaded model. Rust loads it once, uses it from one thread at a time, and frees it.
nonisolated(unsafe) private var loaded: Voz?

@_cdecl("myna_voz_installed")
public func vozInstalled(_ dir: UnsafePointer<CChar>) -> Bool {
    Voz.isDownloaded(directory: String(cString: dir))
}

/// Downloads into `dir`, calling `progress(ctx, 0…1)`. `{}` or `{"error": "…"}`.
@_cdecl("myna_voz_download")
public func vozDownload(_ dir: UnsafePointer<CChar>, _ ctx: UnsafeMutableRawPointer, _ progress: @convention(c) (UnsafeMutableRawPointer, Double) -> Void) -> UnsafeMutablePointer<CChar> {
    let dir = String(cString: dir)
    nonisolated(unsafe) let ctx = ctx
    return block {
        do {
            try await Voz.download(directory: dir) { progress(ctx, $0.fraction) }
            return "{}"
        } catch {
            return say(error)
        }
    }
}

/// Loads the model from `dir`. The first load after a download takes 10-30 s while Core ML prepares it.
@_cdecl("myna_voz_load")
public func vozLoad(_ dir: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let dir = String(cString: dir)
    return block {
        do {
            guard Voz.isDownloaded(directory: dir) else { return json(["error": "Voz is missing. Download it again."]) }
            loaded = try await Voz(directory: dir)
            return "{}"
        } catch {
            return say(error)
        }
    }
}

@_cdecl("myna_voz_free")
public func vozFree() {
    loaded = nil
}

/// Words grouped into sentence-ish segments: a new one after a pause over 0.8 s or a word that ends a sentence.
/// No word timestamps: the whole text as one segment.
private func sentences(_ words: [Word], fallback: String, duration: Double) -> [[String: Any]] {
    var out: [[String: Any]] = []
    var run: [Word] = []
    func flush() {
        guard let first = run.first, let last = run.last else { return }
        let text = run.map { $0.text.trimmingCharacters(in: .whitespaces) }.joined(separator: " ")
        if !text.isEmpty { out.append(["start": first.start, "end": last.end, "text": text]) }
        run = []
    }
    for w in words {
        if let last = run.last, w.start - last.end > 0.8 { flush() }
        run.append(w)
        if let c = w.text.trimmingCharacters(in: .whitespaces).last, ".?!".contains(c) { flush() }
    }
    flush()
    let text = fallback.trimmingCharacters(in: .whitespacesAndNewlines)
    return out.isEmpty && !text.isEmpty ? [["start": 0, "end": duration, "text": text]] : out
}

/// 16 kHz mono samples and a JSON array of words to prefer in; `{"segments": [...]}` or `{"error": "…"}` out.
@_cdecl("myna_voz_transcribe")
public func vozTranscribe(_ samples: UnsafePointer<Float>, _ count: Int, _ vocabulary: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    let words = (try? JSONSerialization.jsonObject(with: Data(String(cString: vocabulary).utf8))) as? [String] ?? []
    return block {
        guard let voz = loaded else { return json(["error": "Voz isn't loaded."]) }
        do {
            let r = try await voz.transcribe(samples: audio, vocabulary: words)
            return json(["segments": sentences(r.words, fallback: r.text, duration: r.duration)])
        } catch {
            return say(error)
        }
    }
}
