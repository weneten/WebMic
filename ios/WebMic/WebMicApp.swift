import AVFAudio
import AVFoundation
import Combine
import SwiftUI
import Vision
import VisionKit

@main
struct WebMicApp: App {
    var body: some Scene {
        WindowGroup {
            MicrophoneView()
        }
    }
}

struct MicrophoneView: View {
    @StateObject private var microphone = Microphone()
    @State private var link = ""
    @State private var showingScanner = false
    @State private var scannerError: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("WebMic-Server") {
                    Button {
                        Task { await openScanner() }
                    } label: {
                        Label("QR-Code scannen", systemImage: "qrcode.viewfinder")
                    }
                    .disabled(microphone.isActive)
                    TextField("HTTPS-Link vom PC", text: $link, axis: .vertical)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                        .disabled(microphone.isActive)
                    PasteButton(payloadType: String.self) { values in
                        if let value = values.first { link = value }
                    }
                    .disabled(microphone.isActive)
                }

                Section {
                    Label(microphone.status, systemImage: microphone.isActive ? "mic.fill" : "mic.slash")
                    if microphone.isActive {
                        Button("Mikrofon stoppen", role: .destructive) { microphone.stop() }
                    } else {
                        Button("Mikrofon starten") {
                            Task { await microphone.start(link: link) }
                        }
                        .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                } footer: {
                    Text("Wähle am PC WebMic als Audioeingang. Während der Übertragung kannst du den Bildschirm sperren.")
                }
            }
            .navigationTitle("WebMic")
        }
        .sheet(isPresented: $showingScanner) {
            QRScannerSheet { scannedLink in
                link = scannedLink
                showingScanner = false
            } onError: { message in
                showingScanner = false
                scannerError = message
            }
        }
        .alert("QR-Scanner", isPresented: Binding(
            get: { scannerError != nil },
            set: { if !$0 { scannerError = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(scannerError ?? "")
        }
        .onReceive(NotificationCenter.default.publisher(for: AVAudioSession.interruptionNotification).receive(on: RunLoop.main)) { event in
            microphone.handleInterruption(event)
        }
        .onReceive(NotificationCenter.default.publisher(for: .AVAudioEngineConfigurationChange).receive(on: RunLoop.main)) { event in
            microphone.handleInputChange(event)
        }
    }
}


extension MicrophoneView {
    @MainActor
    private func openScanner() async {
        guard DataScannerViewController.isSupported else {
            scannerError = "Auf diesem Gerät ist der QR-Scanner nicht verfügbar. Füge den Link manuell ein."
            return
        }
        guard await AVCaptureDevice.requestAccess(for: .video) else {
            scannerError = "Erlaube WebMic den Kamerazugriff in den iPhone-Einstellungen oder füge den Link manuell ein."
            return
        }
        guard DataScannerViewController.isAvailable else {
            scannerError = "Die Kamera ist gerade nicht verfügbar. Füge den Link manuell ein."
            return
        }
        guard !microphone.isActive else { return }
        showingScanner = true
    }
}

private struct QRScannerSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var invalidCode = false
    let onLink: (String) -> Void
    let onError: (String) -> Void

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                QRScanner { payload in
                    guard ServerLink(payload) != nil else {
                        invalidCode = true
                        return false
                    }
                    onLink(payload)
                    return true
                } onError: {
                    onError("Die Kamera konnte nicht verwendet werden. Füge den Link manuell ein.")
                }
                Text(invalidCode
                     ? "Das ist kein WebMic-Link. Scanne den QR-Code aus dem PC-Terminal."
                     : "Richte die Kamera auf den WebMic-QR-Code im PC-Terminal.")
                    .multilineTextAlignment(.center)
                    .padding()
            }
            .navigationTitle("QR-Code scannen")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Abbrechen") { dismiss() }
                }
            }
        }
    }
}

private struct QRScanner: UIViewControllerRepresentable {
    let onScan: (String) -> Bool
    let onError: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(scanner: self) }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let controller = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true,
            isGuidanceEnabled: false,
            isHighlightingEnabled: true
        )
        controller.delegate = context.coordinator
        Task { @MainActor [weak controller, weak coordinator = context.coordinator] in
            guard let controller, let coordinator, !coordinator.finished else { return }
            do {
                try controller.startScanning()
            } catch {
                coordinator.fail()
            }
        }
        return controller
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {
        context.coordinator.scanner = self
    }

    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) {
        coordinator.finished = true
        controller.stopScanning()
    }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        var scanner: QRScanner
        var finished = false

        init(scanner: QRScanner) { self.scanner = scanner }

        func dataScanner(_ controller: DataScannerViewController, didAdd addedItems: [RecognizedItem],
                         allItems: [RecognizedItem]) {
            guard !finished else { return }
            for item in addedItems {
                if case .barcode(let barcode) = item, let payload = barcode.payloadStringValue,
                   scanner.onScan(payload) {
                    finished = true
                    controller.stopScanning()
                    return
                }
            }
        }

        func dataScanner(_ controller: DataScannerViewController,
                         becameUnavailableWithError error: DataScannerViewController.ScanningUnavailable) {
            fail()
        }

        func fail() {
            guard !finished else { return }
            finished = true
            scanner.onError()
        }
    }
}
