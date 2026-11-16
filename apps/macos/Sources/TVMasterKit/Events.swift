import Foundation

/// Live updates over the daemon's WebSocket (`/api/v1/ws`).
public final class EventStream: @unchecked Sendable {
    private let client: TVMasterClient
    private let session: URLSession
    private let lock = NSLock()
    private var playerOf: String?

    /// Uses the client's own session, so a pinned certificate is honoured on `wss://` too.
    public init(client: TVMasterClient, session: URLSession? = nil) {
        self.client = client
        self.session = session ?? client.urlSession
    }

    /// Announce that this connection is the player for TV `id`, so the server
    /// shows it online and a remote can control it. Sent on every (re)connect.
    public func announce(tv id: String?) {
        lock.lock(); playerOf = id; lock.unlock()
    }

    private var announced: String? {
        lock.lock(); defer { lock.unlock() }
        return playerOf
    }

    /// Turn one pushed message into an event. Unknown message types are ignored.
    public static func parse(_ data: Data) -> ServerEvent? {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        struct Envelope: Decodable { var type: String }
        guard let type = (try? decoder.decode(Envelope.self, from: data))?.type else { return nil }
        switch type {
        case "snapshot":
            struct M: Decodable { var tvs: [Tv] }
            return (try? decoder.decode(M.self, from: data)).map { .snapshot(tvs: $0.tvs) }
        case "tv.state":
            struct M: Decodable { var tv: Tv }
            return (try? decoder.decode(M.self, from: data)).map { .tvState($0.tv) }
        case "tv.removed":
            struct M: Decodable { var id: String }
            return (try? decoder.decode(M.self, from: data)).map { .tvRemoved($0.id) }
        case "channel.now":
            struct M: Decodable { var channel: String; var now: Program?; var next: Program? }
            return (try? decoder.decode(M.self, from: data)).map { .channelNow(channel: $0.channel, now: $0.now, next: $0.next) }
        case "schedule.changed":
            struct M: Decodable { var channel: String }
            return (try? decoder.decode(M.self, from: data)).map { .scheduleChanged(channel: $0.channel) }
        case "live.changed":
            return .liveChanged
        case "alert.raised", "alert.cleared", "library.changed":
            return .monitorChanged
        default:
            return nil
        }
    }

    func socketRequest() -> URLRequest? {
        guard var c = URLComponents(url: client.baseURL, resolvingAgainstBaseURL: false) else { return nil }
        c.scheme = c.scheme == "https" ? "wss" : "ws"
        c.path = (c.path.hasSuffix("/") ? String(c.path.dropLast()) : c.path) + "/api/v1/ws"
        guard let url = c.url else { return nil }
        var req = URLRequest(url: url)
        if let token = client.token { req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        return req
    }

    /// Events as they arrive. Reconnects with backoff until the consumer stops listening.
    public func events() -> AsyncStream<ServerEvent> {
        AsyncStream { continuation in
            let task = Task { [session] in
                var delay: UInt64 = 1_000_000_000
                while !Task.isCancelled {
                    guard let req = self.socketRequest() else { break }
                    let socket = session.webSocketTask(with: req)
                    socket.resume()
                    if let tv = self.announced,
                       let data = try? JSONSerialization.data(withJSONObject: ["type": "hello", "tv": tv]),
                       let hello = String(data: data, encoding: .utf8) {
                        try? await socket.send(.string(hello))
                    }
                    var gotMessage = false
                    do {
                        while !Task.isCancelled {
                            let message = try await socket.receive()
                            if !gotMessage { gotMessage = true; delay = 1_000_000_000; continuation.yield(.connection(true)) }
                            let data: Data?
                            switch message {
                            case .string(let s): data = s.data(using: .utf8)
                            case .data(let d): data = d
                            @unknown default: data = nil
                            }
                            if let data, let event = Self.parse(data) { continuation.yield(event) }
                        }
                    } catch {
                        // fall through to reconnect
                    }
                    socket.cancel(with: .goingAway, reason: nil)
                    if gotMessage { continuation.yield(.connection(false)) }
                    try? await Task.sleep(nanoseconds: delay)
                    delay = min(delay * 2, 15_000_000_000)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
