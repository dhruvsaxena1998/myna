// Desert Ant Who (speaker diarization on the Neural Engine) through its SDK, like Voz.swift.
// The SDK downloads, verifies and loads the weights in the folder Myna gives it (models/who).

import Foundation
import Who

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
nonisolated(unsafe) private var loadedWho: Who?

@_cdecl("myna_who_installed")
public func whoInstalled(_ dir: UnsafePointer<CChar>) -> Bool {
    Who.isDownloaded(directory: String(cString: dir))
}

/// Downloads into `dir`, calling `progress(ctx, 0…1)`. `{}` or `{"error": "…"}`.
@_cdecl("myna_who_download")
public func whoDownload(_ dir: UnsafePointer<CChar>, _ ctx: UnsafeMutableRawPointer, _ progress: @convention(c) (UnsafeMutableRawPointer, Double) -> Void) -> UnsafeMutablePointer<CChar> {
    let dir = String(cString: dir)
    nonisolated(unsafe) let ctx = ctx
    return block {
        do {
            try await Who(directory: dir).download { progress(ctx, $0) }
            return "{}"
        } catch {
            return say(error)
        }
    }
}

/// Loads the model from `dir`. The SDK builds it lazily, so this prepares it now instead of at the first meeting.
@_cdecl("myna_who_load")
public func whoLoad(_ dir: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let dir = String(cString: dir)
    return block {
        guard Who.isDownloaded(directory: dir) else { return json(["error": "Who is missing. Download it again."]) }
        do {
            let who = Who(directory: dir)
            try await who.download() // already on disk: loads it, fetches nothing
            loadedWho = who
            return "{}"
        } catch {
            return say(error)
        }
    }
}

@_cdecl("myna_who_free")
public func whoFree() {
    loadedWho = nil
}

/// 16 kHz mono samples in; `{"turns": [{"start", "end", "speaker"}]}` or `{"error": "…"}` out.
@_cdecl("myna_who_diarize")
public func whoDiarize(_ samples: UnsafePointer<Float>, _ count: Int) -> UnsafeMutablePointer<CChar> {
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    return block {
        guard let who = loadedWho else { return json(["error": "Who isn't loaded."]) }
        do {
            let r = try await who.diarize(samples: audio, sampleRate: 16_000)
            return json(["turns": r.turns.map { ["start": $0.start, "end": $0.end, "speaker": $0.speaker] }])
        } catch {
            return say(error)
        }
    }
}
