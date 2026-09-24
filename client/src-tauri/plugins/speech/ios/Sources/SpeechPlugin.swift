import AVFoundation
import FluidAudio
import Tauri
import UIKit
import WebKit

/// Recording and Parakeet, for the app's Rust side (`src/lib.rs`).
///
/// The phone's half of what `recorder.rs` and `asr.rs` do on the Mac. The
/// audio is written to a file as it arrives rather than held in memory, so
/// a long recording costs disk, not RAM, and a crash halfway leaves a file
/// rather than nothing.
class SpeechPlugin: Plugin {
  private let lock = NSLock()
  private var engine: AVAudioEngine?
  private var raw: AVAudioFile?
  private var rawURL: URL?

  private var asr: AsrManager?

  private var downloading = false
  private var fraction = 0.0
  private var downloadError: String?

  private static let version = AsrModelVersion.v3

  // MARK: - the Action Button

  /// What `RecordIntent.swift` in the app target leaves behind: when the
  /// button was pressed. Both sides spell these the same way; they cannot
  /// share a symbol, being compiled into different modules.
  private static let requestKey = "hippocampus.recordRequested"
  private static let requestNotice = Notification.Name("hippocampus.recordRequested")
  /// A request older than this is stale — the app was not opened in time,
  /// or was killed — and must not start a recording out of nowhere later.
  private static let requestLifetime: TimeInterval = 30

  override func load(webview: WKWebView) {
    super.load(webview: webview)
    // With the app already running, the intent's notification reaches
    // the webview at once; at a cold start nobody is listening yet, and
    // the webview asks for the request itself once it is up.
    NotificationCenter.default.addObserver(
      forName: Self.requestNotice, object: nil, queue: .main
    ) { [weak self] _ in
      self?.trigger("record", data: JSObject())
    }
  }

  @objc public func takeRecordRequest(_ invoke: Invoke) {
    let defaults = UserDefaults.standard
    let at = defaults.double(forKey: Self.requestKey)
    defaults.removeObject(forKey: Self.requestKey)
    let fresh = at > 0 && Date().timeIntervalSince1970 - at < Self.requestLifetime
    invoke.resolve(["requested": fresh])
  }

  // MARK: - recording

  @objc public func startRecording(_ invoke: Invoke) {
    Task {
      do {
        guard await Self.microphoneAllowed() else {
          invoke.reject(
            "Hippocampus is not allowed to use the microphone. Allow it in Settings, Privacy & Security, Microphone.")
          return
        }
        try self.begin()
        invoke.resolve()
      } catch {
        self.teardown()
        invoke.reject("could not start recording: \(error.localizedDescription)")
      }
    }
  }

  @objc public func stopRecording(_ invoke: Invoke) {
    guard let url = finish() else {
      invoke.reject("not recording")
      return
    }
    do {
      let wav = try Self.toParakeetWav(url)
      try? FileManager.default.removeItem(at: url)
      invoke.resolve(["path": wav.path])
    } catch {
      invoke.reject("could not read the recording back: \(error.localizedDescription)")
    }
  }

  @objc public func cancelRecording(_ invoke: Invoke) {
    if let url = finish() {
      try? FileManager.default.removeItem(at: url)
    }
    invoke.resolve()
  }

  private static func microphoneAllowed() async -> Bool {
    switch AVAudioApplication.shared.recordPermission {
    case .granted: return true
    case .denied: return false
    default: return await AVAudioApplication.requestRecordPermission()
    }
  }

  private func begin() throws {
    lock.lock()
    defer { lock.unlock() }
    if engine != nil { throw SpeechError.alreadyRecording }

    let session = AVAudioSession.sharedInstance()
    try session.setCategory(.record, mode: .default)
    try session.setActive(true)

    let engine = AVAudioEngine()
    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    guard format.sampleRate > 0 else { throw SpeechError.noMicrophone }

    // The hardware's own format, as it comes; converted once at the end.
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("capture-\(UUID().uuidString).caf")
    let file = try AVAudioFile(forWriting: url, settings: format.settings)

    input.installTap(onBus: 0, bufferSize: 4096, format: format) { buffer, _ in
      try? file.write(from: buffer)
    }
    engine.prepare()
    try engine.start()

    self.engine = engine
    self.raw = file
    self.rawURL = url
  }

  /// Closes the microphone. Returns the raw file, if there was a recording.
  private func finish() -> URL? {
    lock.lock()
    defer { lock.unlock() }
    guard let engine else { return nil }
    engine.inputNode.removeTap(onBus: 0)
    engine.stop()
    self.engine = nil
    raw = nil  // closes the file
    try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    let url = rawURL
    rawURL = nil
    return url
  }

  private func teardown() {
    if let url = finish() { try? FileManager.default.removeItem(at: url) }
  }

  /// 16 kHz mono 16-bit WAV: what Parakeet reads, and what the Mac sends
  /// to the backend, so a capture's audio looks the same whichever device
  /// made it.
  private static func toParakeetWav(_ url: URL) throws -> URL {
    let samples = try AudioConverter().resampleAudioFile(url)
    let out = FileManager.default.temporaryDirectory
      .appendingPathComponent("capture-\(UUID().uuidString).wav")
    let settings: [String: Any] = [
      AVFormatIDKey: kAudioFormatLinearPCM,
      AVSampleRateKey: 16_000,
      AVNumberOfChannelsKey: 1,
      AVLinearPCMBitDepthKey: 16,
      AVLinearPCMIsFloatKey: false,
      AVLinearPCMIsBigEndianKey: false,
    ]
    let file = try AVAudioFile(
      forWriting: out, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
    guard
      let format = AVAudioFormat(
        commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false),
      let buffer = AVAudioPCMBuffer(
        pcmFormat: format, frameCapacity: AVAudioFrameCount(max(samples.count, 1)))
    else { throw SpeechError.conversion }
    buffer.frameLength = AVAudioFrameCount(samples.count)
    samples.withUnsafeBufferPointer { source in
      buffer.floatChannelData![0].update(from: source.baseAddress!, count: samples.count)
    }
    try file.write(from: buffer)
    return out
  }

  // MARK: - the model

  private static var modelDirectory: URL { AsrModels.defaultCacheDirectory(for: version) }

  @objc public func modelStatus(_ invoke: Invoke) {
    lock.lock()
    let status: [String: Any?] = [
      "installed": AsrModels.modelsExist(at: Self.modelDirectory, version: Self.version),
      "downloading": downloading,
      "fraction": fraction,
      "error": downloadError,
    ]
    lock.unlock()
    invoke.resolve(status.compactMapValues { $0 })
  }

  @objc public func downloadModel(_ invoke: Invoke) {
    lock.lock()
    if downloading {
      lock.unlock()
      invoke.resolve()
      return
    }
    downloading = true
    fraction = 0
    downloadError = nil
    lock.unlock()
    invoke.resolve()

    Task {
      do {
        _ = try await AsrModels.download(version: Self.version) { [weak self] progress in
          guard let self else { return }
          self.lock.lock()
          self.fraction = progress.fractionCompleted
          self.lock.unlock()
        }
        // Loaded once straight away: the first load compiles the model for
        // the Neural Engine, which takes a while, and better now than on
        // the first thing you say.
        _ = try await self.loadedManager()
      } catch {
        self.lock.lock()
        self.downloadError = error.localizedDescription
        self.lock.unlock()
      }
      self.lock.lock()
      self.downloading = false
      self.lock.unlock()
    }
  }

  private func loadedManager() async throws -> AsrManager {
    if let asr { return asr }
    let models = try await AsrModels.load(from: Self.modelDirectory, version: Self.version)
    let asr = AsrManager()
    try await asr.loadModels(models)
    self.asr = asr
    return asr
  }

  // MARK: - transcription

  @objc public func transcribe(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(TranscribeArgs.self)
    Task {
      do {
        let manager = try await self.loadedManager()
        var state = TdtDecoderState.make(decoderLayers: Self.version.decoderLayers)
        let result = try await manager.transcribe(
          URL(fileURLWithPath: args.path), decoderState: &state)
        invoke.resolve(["text": result.text.trimmingCharacters(in: .whitespacesAndNewlines)])
      } catch {
        invoke.reject("transcription failed: \(error.localizedDescription)")
      }
    }
  }
}

class TranscribeArgs: Decodable {
  let path: String
}

enum SpeechError: LocalizedError {
  case alreadyRecording
  case noMicrophone
  case conversion

  var errorDescription: String? {
    switch self {
    case .alreadyRecording: return "already recording"
    case .noMicrophone: return "no microphone input is available"
    case .conversion: return "the recording could not be converted"
    }
  }
}

@_cdecl("init_plugin_speech")
func initPlugin() -> Plugin {
  return SpeechPlugin()
}
