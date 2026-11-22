import Foundation

struct ServerLink: Sendable {
    private let endpoint: URL
    private let origin: String
    private let token: String
    private let sessionID = UUID().uuidString

    init?(_ link: String) {
        guard var url = URLComponents(string: link.trimmingCharacters(in: .whitespacesAndNewlines)),
              url.scheme == "https", let host = url.host, !host.isEmpty,
              url.user == nil, url.password == nil, url.query == nil,
              url.path.isEmpty || url.path == "/",
              let token = url.fragment, token.utf8.count == 32,
              token.utf8.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { return nil }
        url.fragment = nil
        url.path = ""
        guard let origin = url.string else { return nil }
        url.path = "/audio"
        guard let endpoint = url.url else { return nil }
        self.origin = origin
        self.endpoint = endpoint
        self.token = token
    }

    @MainActor
    func transmit(_ packets: AsyncStream<Data>, onConnected: () -> Void) async throws {
        let config = URLSessionConfiguration.ephemeral
        config.waitsForConnectivity = true
        config.timeoutIntervalForRequest = 7
        config.timeoutIntervalForResource = 15
        let session = URLSession(configuration: config, delegate: NoRedirects(), delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        var connected = false
        for await packet in packets {
            try Task.checkCancellation()
            var request = request(method: "POST")
            request.httpBody = packet
            do {
                let (_, response) = try await session.data(for: request)
                guard let status = (response as? HTTPURLResponse)?.statusCode else {
                    throw ServerError.response
                }
                switch status {
                case 204:
                    if !connected {
                        connected = true
                        onConnected()
                    }
                case 403: throw ServerError.link
                case 409: throw ServerError.busy
                default: throw ServerError.response
                }
            } catch let error as URLError {
                switch error.code {
                case .serverCertificateUntrusted, .serverCertificateHasUnknownRoot,
                     .serverCertificateHasBadDate, .serverCertificateNotYetValid,
                     .secureConnectionFailed:
                    throw ServerError.certificate
                default:
                    throw ServerError.connection
                }
            }
        }
    }

    func disconnect() async {
        let session = URLSession(configuration: .ephemeral, delegate: NoRedirects(), delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        _ = try? await session.data(for: request(method: "DELETE"))
    }

    private func request(method: String) -> URLRequest {
        var request = URLRequest(url: endpoint, timeoutInterval: 7)
        request.httpMethod = method
        request.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        request.setValue(origin, forHTTPHeaderField: "Origin")
        request.setValue(token, forHTTPHeaderField: "X-WebMic-Token")
        request.setValue(sessionID, forHTTPHeaderField: "X-WebMic-Session")
        return request
    }
}

private final class NoRedirects: NSObject, URLSessionTaskDelegate {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

private enum ServerError: LocalizedError {
    case link, busy, connection, certificate, response

    var errorDescription: String? {
        switch self {
        case .link: "Der Link ist abgelaufen oder ungültig. Kopiere den aktuellen Link vom PC."
        case .busy: "Ein anderes Gerät sendet bereits. Trenne es und versuche es erneut."
        case .connection: "PC nicht erreichbar. Prüfe WLAN, den Server und den Zugriff auf das lokale Netzwerk."
        case .certificate: "Vertraue dem WebMic-Zertifikat des PCs in den iPhone-Einstellungen."
        case .response: "Der PC hat die Audioübertragung abgelehnt."
        }
    }
}
