// C entry points for Apple-only runtimes. Rust declares them in src/voz.rs.
// Strings that cross over are malloc'd here and handed back to `myna_string_free`.
import Foundation
#if canImport(Voz)
import Voz
import Who
#endif

private func cString(_ s: String) -> UnsafeMutablePointer<CChar> { strdup(s)! }

@_cdecl("myna_string_free")
public func mynaStringFree(_ s: UnsafeMutablePointer<CChar>?) { free(s) }

#if canImport(Voz)
/// Runs async SDK calls to completion; the Rust caller is already on its own thread.
private final class Outcome<T>: @unchecked Sendable { var value: Result<T, Error>? }

private struct MissingFiles: Error, CustomStringConvertible {
    var description: String { "Voz's model files aren't in the models folder." }
}

private func wait<T>(_ body: @escaping @Sendable () async throws -> T) throws -> T {
    let outcome = Outcome<T>()
    let done = DispatchSemaphore(value: 0)
    Task.detached {
        do { outcome.value = .success(try await body()) } catch { outcome.value = .failure(error) }
        done.signal()
    }
    done.wait()
    return try outcome.value!.get()
}
#endif

/// NULL when Voz can run on this Mac with this build, else the reason it can't.
@_cdecl("myna_voz_unavailable")
public func mynaVozUnavailable() -> UnsafeMutablePointer<CChar>? {
    #if canImport(Voz)
    // The model's compiled graphs declare macOS 15.
    if #available(macOS 15, *) { return nil }
    return cString("Voz needs macOS 15 or newer.")
    #else
    return cString("This build of Myna doesn't include the Desert Ant SDK.")
    #endif
}

/// Loads Voz from a folder of its model files. Returns an owned handle, or NULL with `failure` set.
@_cdecl("myna_voz_load")
public func mynaVozLoad(
    _ dir: UnsafePointer<CChar>,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutableRawPointer? {
    #if canImport(Voz)
    let path = String(cString: dir)
    do {
        // Handed an incomplete folder the SDK would download the model itself. Downloads are Myna's job.
        guard Voz.isDownloaded(directory: path) else { throw MissingFiles() }
        let voz = try wait { try await Voz(directory: path) }
        return Unmanaged.passRetained(voz).toOpaque()
    } catch {
        failure.pointee = cString("\(error)")
        return nil
    }
    #else
    failure.pointee = mynaVozUnavailable()
    return nil
    #endif
}

/// Mono 16 kHz samples in, transcript out (owned). NULL with `failure` set on failure.
@_cdecl("myna_voz_transcribe")
public func mynaVozTranscribe(
    _ handle: UnsafeMutableRawPointer,
    _ samples: UnsafePointer<Float>,
    _ count: Int,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutablePointer<CChar>? {
    #if canImport(Voz)
    let voz = Unmanaged<Voz>.fromOpaque(handle).takeUnretainedValue()
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    do {
        return cString(try wait { try await voz.transcribe(samples: audio).text })
    } catch {
        failure.pointee = cString("\(error)")
        return nil
    }
    #else
    failure.pointee = mynaVozUnavailable()
    return nil
    #endif
}

@_cdecl("myna_voz_free")
public func mynaVozFree(_ handle: UnsafeMutableRawPointer) {
    #if canImport(Voz)
    Unmanaged<Voz>.fromOpaque(handle).release()
    #endif
}

/// NULL when Who can run on this Mac with this build, else the reason it can't.
@_cdecl("myna_who_unavailable")
public func mynaWhoUnavailable() -> UnsafeMutablePointer<CChar>? {
    #if canImport(Voz)
    if #available(macOS 15, *) { return nil }
    return cString("Who needs macOS 15 or newer.")
    #else
    return cString("This build of Myna doesn't include the Desert Ant SDK.")
    #endif
}

/// Loads Who from a folder of its model files. Returns an owned handle, or NULL with `failure` set.
@_cdecl("myna_who_load")
public func mynaWhoLoad(
    _ dir: UnsafePointer<CChar>,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutableRawPointer? {
    #if canImport(Voz)
    let path = String(cString: dir)
    // Handed an incomplete folder the SDK would download the model itself. Downloads are Myna's job.
    guard Who.isDownloaded(directory: path) else {
        failure.pointee = cString("Who's model files aren't in the models folder.")
        return nil
    }
    return Unmanaged.passRetained(Who(directory: path)).toOpaque()
    #else
    failure.pointee = mynaWhoUnavailable()
    return nil
    #endif
}

/// Mono 16 kHz samples in; one "start end speaker" line per turn out (seconds, speakers from 0), owned.
/// NULL with `failure` set on failure.
@_cdecl("myna_who_diarize")
public func mynaWhoDiarize(
    _ handle: UnsafeMutableRawPointer,
    _ samples: UnsafePointer<Float>,
    _ count: Int,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutablePointer<CChar>? {
    #if canImport(Voz)
    let who = Unmanaged<Who>.fromOpaque(handle).takeUnretainedValue()
    let audio = Array(UnsafeBufferPointer(start: samples, count: count))
    do {
        let result = try wait { try await who.diarize(samples: audio, sampleRate: 16_000) }
        return cString(result.turns.map { "\($0.start) \($0.end) \($0.speaker)\n" }.joined())
    } catch {
        failure.pointee = cString("\(error)")
        return nil
    }
    #else
    failure.pointee = mynaWhoUnavailable()
    return nil
    #endif
}

@_cdecl("myna_who_free")
public func mynaWhoFree(_ handle: UnsafeMutableRawPointer) {
    #if canImport(Voz)
    Unmanaged<Who>.fromOpaque(handle).release()
    #endif
}
