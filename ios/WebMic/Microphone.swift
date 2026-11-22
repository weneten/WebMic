import AVFAudio
import Combine
import Foundation

@MainActor
final class Microphone: ObservableObject {
    @Published private(set) var isActive = false
    @Published private(set) var status = "Bereit"

    private var attempt: UUID?
    private var engine: AVAudioEngine?
    private var continuation: AsyncStream<Data>.Continuation?
    private var transmission: Task<Void, Never>?
    private var server: ServerLink?
    private var audioSessionActive = false

    func start(link: String) async {
        guard !isActive else { return }
        guard let server = ServerLink(link) else {
            status = "Füge den vollständigen HTTPS-Link aus dem PC-Terminal ein."
            return
        }

        let attempt = UUID()
        self.attempt = attempt
        isActive = true
        status = "Mikrofon wird geöffnet …"
        let permitted = await AVAudioApplication.requestRecordPermission()
        guard self.attempt == attempt else { return }
        guard permitted else {
            stop(message: "Erlaube WebMic den Mikrofonzugriff in den iPhone-Einstellungen.")
            return
        }

        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.mixWithOthers])
            try session.setActive(true)
            audioSessionActive = true

            let engine = AVAudioEngine()
            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            guard format.sampleRate > 0, format.channelCount > 0,
                  let encoder = PCMEncoder(input: format) else {
                throw MicrophoneError.noInput
            }

            let (packets, continuation) = AsyncStream<Data>.makeStream(bufferingPolicy: .bufferingNewest(1))
            input.installTap(onBus: 0, bufferSize: 960, format: nil) { buffer, _ in
                encoder.encode(buffer, yield: continuation)
            }
            self.engine = engine
            self.continuation = continuation
            self.server = server
            engine.prepare()
            try engine.start()

            status = "Verbindung zum PC wird aufgebaut …"
            transmission = Task {
                do {
                    try await server.transmit(packets) { [weak self] in
                        guard self?.attempt == attempt else { return }
                        self?.status = "Verbunden · Mikrofon aktiv"
                    }
                } catch {
                    guard self.attempt == attempt, !Task.isCancelled else { return }
                    self.stop(message: error.localizedDescription)
                }
            }
        } catch {
            stop(message: error.localizedDescription)
        }
    }

    func stop(message: String = "Bereit") {
        attempt = nil
        if let engine {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            self.engine = nil
        }
        continuation?.finish()
        continuation = nil
        let previousTransmission = transmission
        previousTransmission?.cancel()
        transmission = nil

        if let server {
            Task {
                await previousTransmission?.value
                await server.disconnect()
            }
            self.server = nil
        }

        if audioSessionActive {
            try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
            audioSessionActive = false
        }
        isActive = false
        status = message
    }

    func handleInterruption(_ notification: Notification) {
        guard isActive,
              let type = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
              type == AVAudioSession.InterruptionType.began.rawValue else { return }
        stop(message: "iOS hat die Aufnahme unterbrochen. Starte das Mikrofon erneut.")
    }

    func handleInputChange(_ notification: Notification) {
        guard let engine, notification.object as? AVAudioEngine === engine, !engine.isRunning else { return }
        stop(message: "Das Audiogerät hat gewechselt. Starte das Mikrofon erneut.")
    }
}

private enum MicrophoneError: LocalizedError {
    case noInput

    var errorDescription: String? { "Kein unterstütztes Mikrofon verfügbar." }
}

// The instance is owned by one audio tap and is only used by that tap.
private final class PCMEncoder: @unchecked Sendable {
    private let converter: AVAudioConverter
    private let output: AVAudioFormat
    private var converted: AVAudioPCMBuffer?
    private var packet = Data()
    private let packetBytes = 3840 // 40 ms of signed little-endian PCM, mono, 48 kHz.

    init?(input: AVAudioFormat) {
        guard let output = AVAudioFormat(commonFormat: .pcmFormatInt16, sampleRate: 48_000,
                                         channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: input, to: output) else { return nil }
        self.converter = converter
        self.output = output
        packet.reserveCapacity(packetBytes)
    }

    func encode(_ buffer: AVAudioPCMBuffer, yield continuation: AsyncStream<Data>.Continuation) {
        let capacity = AVAudioFrameCount(ceil(Double(buffer.frameLength) * output.sampleRate / buffer.format.sampleRate)) + 32
        if (converted?.frameCapacity ?? 0) < capacity {
            converted = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: capacity)
        }
        guard let converted else { return }
        converted.frameLength = 0
        var supplied = false
        var error: NSError?
        let result = converter.convert(to: converted, error: &error) { _, inputStatus in
            inputStatus.pointee = supplied ? .noDataNow : .haveData
            if supplied { return nil }
            supplied = true
            return buffer
        }
        guard result != .error, let samples = converted.int16ChannelData?[0] else { return }

        let bytes = UnsafeRawBufferPointer(start: samples, count: Int(converted.frameLength) * 2)
        var offset = 0
        while offset < bytes.count {
            let end = min(bytes.count, offset + packetBytes - packet.count)
            packet.append(contentsOf: bytes[offset..<end])
            offset = end
            if packet.count == packetBytes {
                continuation.yield(packet)
                packet.removeAll(keepingCapacity: true)
            }
        }
    }
}
