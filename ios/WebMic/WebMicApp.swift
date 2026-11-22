import AVFAudio
import Combine
import SwiftUI

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

    var body: some View {
        NavigationStack {
            Form {
                Section("WebMic-Server") {
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
        .onReceive(NotificationCenter.default.publisher(for: AVAudioSession.interruptionNotification).receive(on: RunLoop.main)) { event in
            microphone.handleInterruption(event)
        }
        .onReceive(NotificationCenter.default.publisher(for: .AVAudioEngineConfigurationChange).receive(on: RunLoop.main)) { event in
            microphone.handleInputChange(event)
        }
    }
}
