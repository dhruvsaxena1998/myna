// A meeting's Them track: a Core Audio process tap on the apps using the microphone, never the whole Mac,
// so music in another app stays out. Kept thin: Rust resamples, lines it up and writes it.

import AppKit
import CoreAudio

private let system = AudioObjectID(kAudioObjectSystemObject)

private func address(_ sel: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(mSelector: sel, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
}

private func read<T>(_ obj: AudioObjectID, _ sel: AudioObjectPropertySelector, _ value: T) -> T? {
    var addr = address(sel)
    var v = value
    var size = UInt32(MemoryLayout<T>.size)
    let err = withUnsafeMutablePointer(to: &v) { AudioObjectGetPropertyData(obj, &addr, 0, nil, &size, $0) }
    return err == noErr ? v : nil
}

private typealias Responsible = @convention(c) (pid_t) -> pid_t
// The app a helper process works for (Chrome's audio helper is Google Chrome). Private, so only if it's there.
private let responsible = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid").map { unsafeBitCast($0, to: Responsible.self) }

/// The process objects to tap, with the app names: every process of each app (not Myna) whose mic is running.
/// Only regular apps count, so system listeners like Siri's are left out.
private func micApps() -> [AudioObjectID: String] {
    var addr = address(kAudioHardwarePropertyProcessObjectList)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr else { return [:] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(system, &addr, 0, nil, &size, &ids) == noErr else { return [:] }
    let me = getpid()
    var owner: [AudioObjectID: NSRunningApplication] = [:]
    var live = Set<pid_t>()
    for id in ids {
        guard let pid = read(id, kAudioProcessPropertyPID, pid_t(0)) else { continue }
        let app = responsible?(pid) ?? pid
        guard pid != me, app != me, let running = NSRunningApplication(processIdentifier: app), running.activationPolicy == .regular else { continue }
        owner[id] = running
        if read(id, kAudioProcessPropertyIsRunningInput, UInt32(0)) == 1 { live.insert(app) }
    }
    return owner.filter { live.contains($0.value.processIdentifier) }.mapValues { $0.localizedName ?? $0.bundleIdentifier ?? "An app" }
}

private func json(_ value: Any) -> String {
    String(decoding: (try? JSONSerialization.data(withJSONObject: value)) ?? Data("{}".utf8), as: UTF8.self)
}

public typealias Audio = @convention(c) (UnsafeMutableRawPointer, UnsafePointer<Float>, Int, UInt32, Double) -> Void
public typealias Event = @convention(c) (UnsafeMutableRawPointer, UnsafePointer<CChar>) -> Void

private final class Recording {
    let ctx: UnsafeMutableRawPointer
    let audio: Audio
    let event: Event
    var timer: DispatchSourceTimer?
    /// Only grows: an app that stops its mic (muted in Meet) stays tapped, so the other people stay in.
    var tapped: [AudioObjectID: String] = [:]
    var tap = AudioObjectID(kAudioObjectUnknown)
    var device = AudioObjectID(kAudioObjectUnknown)
    var proc: AudioDeviceIOProcID?

    init(_ ctx: UnsafeMutableRawPointer, _ audio: @escaping Audio, _ event: @escaping Event) {
        self.ctx = ctx
        self.audio = audio
        self.event = event
    }

    func say(_ value: [String: Any]) {
        json(value).withCString { event(ctx, $0) }
    }

    /// Picks up apps that started using the mic, and taps them all afresh: simpler than changing a live tap,
    /// and the gap is a few milliseconds that Rust fills with silence.
    func poll() {
        let now = micApps()
        guard !now.keys.allSatisfy({ tapped[$0] != nil }) else { return }
        tapped.merge(now) { a, _ in a }
        stop()
        do {
            try start(Array(tapped.keys))
            var names: [String] = []
            for n in tapped.values.sorted() where !names.contains(n) { names.append(n) }
            say(["apps": names])
        } catch {
            stop()
            say(["error": "\(error)"])
        }
    }

    struct Failed: Error, CustomStringConvertible {
        let description: String
    }

    func check(_ status: OSStatus, _ what: String) throws {
        if status != noErr { throw Failed(description: "Couldn't \(what) (Core Audio error \(status)).") }
    }

    func start(_ processes: [AudioObjectID]) throws {
        let desc = CATapDescription(stereoMixdownOfProcesses: processes)
        desc.isPrivate = true
        desc.muteBehavior = .unmuted
        try check(AudioHardwareCreateProcessTap(desc, &tap), "tap the call's audio")
        guard let format = read(tap, kAudioTapPropertyFormat, AudioStreamBasicDescription()) else { throw Failed(description: "Couldn't read the call audio's format.") }
        // shortcut: assumes Float32 like every tap seen so far; convert in Rust if one ever reports another format.
        guard format.mFormatID == kAudioFormatLinearPCM, format.mFormatFlags & kAudioFormatFlagIsFloat != 0, format.mBitsPerChannel == 32 else { throw Failed(description: "The call's audio comes in a format Myna can't read yet.") }
        // The default output device clocks the private aggregate device that carries the tap.
        guard let out = read(system, kAudioHardwarePropertyDefaultSystemOutputDevice, AudioObjectID(kAudioObjectUnknown)),
              let uid = read(out, kAudioDevicePropertyDeviceUID, "" as CFString) else { throw Failed(description: "Couldn't find the Mac's sound output.") }
        let spec: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Myna meeting",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: uid as String,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: uid as String]],
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: desc.uuid.uuidString]],
        ]
        try check(AudioHardwareCreateAggregateDevice(spec as CFDictionary, &device), "set up the call's audio")
        let (ctx, audio, rate) = (ctx, audio, format.mSampleRate)
        try check(AudioDeviceCreateIOProcIDWithBlock(&proc, device, nil) { _, input, _, _, _ in
            // Interleaved, or just the first channel when a tap ever hands them over separately.
            guard let b = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: input)).first, let data = b.mData else { return }
            audio(ctx, data.assumingMemoryBound(to: Float.self), Int(b.mDataByteSize) / MemoryLayout<Float>.size, b.mNumberChannels, rate)
        }, "listen to the call's audio")
        try check(AudioDeviceStart(device, proc), "start listening to the call")
    }

    /// Stops and removes everything; no audio comes after it returns.
    func stop() {
        if let p = proc {
            AudioDeviceStop(device, p)
            AudioDeviceDestroyIOProcID(device, p)
            proc = nil
        }
        if device != kAudioObjectUnknown {
            AudioHardwareDestroyAggregateDevice(device)
            device = AudioObjectID(kAudioObjectUnknown)
        }
        if tap != kAudioObjectUnknown {
            AudioHardwareDestroyProcessTap(tap)
            tap = AudioObjectID(kAudioObjectUnknown)
        }
    }
}

private let queue = DispatchQueue(label: "myna.tap")
nonisolated(unsafe) private var current: Recording?

/// Starts listening for apps using the mic, every 2 s, and taps them. `audio(ctx, samples, count, channels, rate)`
/// gets interleaved Float32; `event(ctx, json)` gets `{"apps": [names]}` when they change, or `{"error": "…"}`.
@_cdecl("myna_tap_start")
public func tapStart(_ ctx: UnsafeMutableRawPointer, _ audio: @escaping Audio, _ event: @escaping Event) {
    queue.sync {
        let r = Recording(ctx, audio, event)
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: .seconds(2))
        timer.setEventHandler { r.poll() }
        r.timer = timer
        current = r
        timer.resume()
    }
}

/// Stops the tap. Neither callback runs after this returns.
@_cdecl("myna_tap_stop")
public func tapStop() {
    queue.sync {
        current?.timer?.cancel()
        current?.stop()
        current = nil
    }
}

// What macOS lets Myna record from other apps. There's no public way to check or ask, so this uses TCC's private
// calls when they're there; without them the answer is "can't tell" (-1) and the first tap asks instead.
private typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int
private typealias Request = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void
private let tcc = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW)
private let service = "kTCCServiceAudioCapture" as CFString

/// 0 allowed, 1 denied, 2 not asked yet, -1 can't tell.
@_cdecl("myna_audio_capture_state")
public func audioCaptureState() -> Int {
    guard let f = dlsym(tcc, "TCCAccessPreflight") else { return -1 }
    let r = unsafeBitCast(f, to: Preflight.self)(service, nil)
    return (0...2).contains(r) ? r : -1
}

/// Shows macOS's pop-up and waits for the answer; false if it can't be asked.
@_cdecl("myna_audio_capture_ask")
public func audioCaptureAsk() -> Bool {
    guard let f = dlsym(tcc, "TCCAccessRequest") else { return false }
    let done = DispatchSemaphore(value: 0)
    unsafeBitCast(f, to: Request.self)(service, nil) { _ in done.signal() }
    done.wait()
    return true
}
