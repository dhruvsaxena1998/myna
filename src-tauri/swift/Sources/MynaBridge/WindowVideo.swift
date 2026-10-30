// Window video: ScreenCaptureKit frames of one window, encoded to H.264 in an MP4 by AVAssetWriter.
// Rust declares these entry points in src/meetings/video.rs.
import AVFoundation
import Foundation
import ScreenCaptureKit

private struct Failure: Error, CustomStringConvertible { let description: String }

/// Low on purpose: slides and shared screens barely move, and the file is only for looking back at.
private let fps = 5

/// An MP4 that frames are appended to as they arrive. Fragmented every 10 s, so a crash leaves a playable file.
final class H264File {
    private let writer: AVAssetWriter
    private let input: AVAssetWriterInput
    private let adaptor: AVAssetWriterInputPixelBufferAdaptor
    private let url: URL
    private var last = CMTime.invalid

    init(path: String, width: Int, height: Int, bitrate: Int) throws {
        url = URL(fileURLWithPath: path)
        try? FileManager.default.removeItem(at: url)
        writer = try AVAssetWriter(url: url, fileType: .mp4)
        writer.movieFragmentInterval = CMTime(seconds: 10, preferredTimescale: 1)
        input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [
                AVVideoAverageBitRateKey: bitrate,
                AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
                AVVideoExpectedSourceFrameRateKey: fps,
                AVVideoMaxKeyFrameIntervalDurationKey: 10,
            ],
        ])
        input.expectsMediaDataInRealTime = true
        adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: nil)
        writer.add(input)
        guard writer.startWriting() else { throw writer.error ?? Failure(description: "Could not start the video file.") }
        writer.startSession(atSourceTime: .zero)
    }

    /// A busy encoder drops the frame rather than stalling capture; times must only go forward.
    func append(_ pixels: CVPixelBuffer, at time: CMTime) {
        guard writer.status == .writing, input.isReadyForMoreMediaData, !last.isValid || time > last else { return }
        if adaptor.append(pixels, withPresentationTime: time) { last = time }
    }

    /// Closes the file. The video lasts until `end` even if the picture stopped changing before then.
    /// With no frames there is nothing to keep, so the file is removed.
    func finish(at end: CMTime) {
        guard last.isValid, writer.status == .writing else {
            writer.cancelWriting()
            try? FileManager.default.removeItem(at: url)
            return
        }
        input.markAsFinished()
        writer.endSession(atSourceTime: max(end, last))
        let done = DispatchSemaphore(value: 0)
        writer.finishWriting { done.signal() }
        done.wait()
    }
}

final class WindowVideo: NSObject, SCStreamOutput, SCStreamDelegate, @unchecked Sendable {
    private let queue = DispatchQueue(label: "myna.window-video")
    private let file: H264File
    private let size: (w: Int, h: Int)
    /// How far into the meeting the video began, so its clock matches the audio's.
    private let lead: CMTime
    private var stream: SCStream?
    private var window: UInt32 = 0
    /// False once the stream has stopped by itself, as when its window closes.
    private var live = false
    private var origin: CMTime?

    static func window(_ id: UInt32) async throws -> SCWindow {
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        guard let win = content.windows.first(where: { $0.windowID == id }) else { throw Failure(description: "The window is gone.") }
        return win
    }

    static func start(path: String, window id: UInt32, width: Int, bitrate: Int, leadMs: Int) async throws -> WindowVideo {
        let win = try await window(id)
        // H.264 wants even sizes. The size is fixed by the first window; a later one is scaled to fit.
        let (w, h) = (width & ~1, Int((Double(width) * win.frame.height / max(1, win.frame.width)).rounded()) & ~1)
        let video = WindowVideo(file: try H264File(path: path, width: w, height: max(2, h), bitrate: bitrate), size: (w, max(2, h)), lead: CMTime(value: CMTimeValue(leadMs), timescale: 1000))
        do { try await video.capture(win) } catch {
            video.file.finish(at: .zero) // no frames: removes the file
            throw error
        }
        return video
    }

    private init(file: H264File, size: (w: Int, h: Int), lead: CMTime) {
        (self.file, self.size, self.lead) = (file, size, lead)
    }

    /// Captures `win` from now on: starts the stream, repoints it, or restarts it after its window closed.
    func capture(_ win: SCWindow) async throws {
        let filter = SCContentFilter(desktopIndependentWindow: win)
        if let stream, queue.sync(execute: { live }) {
            try await stream.updateContentFilter(filter)
        } else {
            let cfg = SCStreamConfiguration()
            cfg.width = size.w
            cfg.height = size.h
            cfg.minimumFrameInterval = CMTime(value: 1, timescale: CMTimeScale(fps))
            cfg.showsCursor = true
            let next = SCStream(filter: filter, configuration: cfg, delegate: self)
            try next.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
            try await next.startCapture()
            stream = next
            queue.sync { live = true }
        }
        window = win.windowID
    }

    func capture(window id: UInt32) async throws {
        if id == window, queue.sync(execute: { live }) { return }
        try await capture(try await WindowVideo.window(id))
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, buffer.isValid, let pixels = CMSampleBufferGetImageBuffer(buffer) else { return }
        // Idle and blank frames repeat the last picture; only these two carry new pixels.
        let info = (CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]])?.first
        guard let raw = info?[.status] as? Int, let status = SCFrameStatus(rawValue: raw), status == .complete || status == .started else { return }
        let pts = buffer.presentationTimeStamp
        let first = origin == nil
        let origin = self.origin ?? pts
        self.origin = origin
        // The first picture also stands in for the time before it, so the video starts at the meeting's start.
        if first && lead > .zero { file.append(pixels, at: .zero) }
        file.append(pixels, at: lead + (pts - origin))
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        queue.async { self.live = false }
    }

    /// Stops the stream and closes the file; the video runs to `endMs` into the meeting.
    func stop(endMs: Int) {
        let running = queue.sync { () -> SCStream? in
            let s = stream
            (stream, live) = (nil, false)
            return s
        }
        if let running {
            try? wait { try await running.stopCapture() }
            try? running.removeStreamOutput(self, type: .screen)
        }
        queue.sync {} // frames already delivered are written before the file closes
        file.finish(at: CMTime(value: CMTimeValue(endMs), timescale: 1000))
    }
}

/// Starts recording `window` into an MP4 at `path`, `width` px wide. Returns an owned handle, or NULL with `failure` set.
/// `leadMs` is how far into the meeting this is.
@_cdecl("myna_window_video_start")
public func mynaWindowVideoStart(
    _ path: UnsafePointer<CChar>,
    _ window: UInt32,
    _ width: Int,
    _ bitrate: Int,
    _ leadMs: Int,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> UnsafeMutableRawPointer? {
    let path = String(cString: path)
    do {
        let video = try wait { try await WindowVideo.start(path: path, window: window, width: width, bitrate: bitrate, leadMs: leadMs) }
        return Unmanaged.passRetained(video).toOpaque()
    } catch {
        failure.pointee = cString("\(error)")
        return nil
    }
}

/// Points the recording at `window`. Does nothing when it already is, and picks the capture back up after its window closed.
@_cdecl("myna_window_video_capture")
public func mynaWindowVideoCapture(
    _ handle: UnsafeMutableRawPointer,
    _ window: UInt32,
    _ failure: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>
) -> Bool {
    let video = Unmanaged<WindowVideo>.fromOpaque(handle).takeUnretainedValue()
    do {
        try wait { try await video.capture(window: window) }
        return true
    } catch {
        failure.pointee = cString("\(error)")
        return false
    }
}

/// Closes the file and frees the handle. The video runs to `endMs` into the meeting.
@_cdecl("myna_window_video_stop")
public func mynaWindowVideoStop(_ handle: UnsafeMutableRawPointer, _ endMs: Int) {
    Unmanaged<WindowVideo>.fromOpaque(handle).takeRetainedValue().stop(endMs: endMs)
}
