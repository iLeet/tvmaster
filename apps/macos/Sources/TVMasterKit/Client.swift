import Foundation

public enum TVMasterError: Error, LocalizedError, Equatable, Sendable {
    case unreachable
    case unauthorized
    case http(Int, String)
    case invalidResponse
    /// An HTTPS server whose certificate nothing vouches for. Carries what the user needs to decide.
    case untrustedCertificate(host: String, fingerprint: String)

    public var errorDescription: String? {
        switch self {
        case .unreachable: "Cannot reach the TVMaster server."
        case .unauthorized: "The server needs an access token, or the token is wrong."
        case .http(_, let message): message
        case .invalidResponse: "The server sent something unexpected."
        case .untrustedCertificate(let host, _): "\(host) uses a certificate this Mac doesn't trust yet."
        }
    }
}

/// A typed client for the TVMaster daemon's REST API.
public struct TVMasterClient: Sendable {
    public let baseURL: URL
    public let token: String?
    private let session: URLSession
    /// Set for HTTPS clients made by [`make`](x-source-tag://make): decides which certificates are accepted.
    public let trust: TrustDelegate?

    public init(baseURL: URL, token: String? = nil, session: URLSession = .shared, trust: TrustDelegate? = nil) {
        self.baseURL = baseURL
        self.token = token?.isEmpty == true ? nil : token
        self.session = session
        self.trust = trust
    }

    /// A client whose connection (HTTPS or not) honours `pins`: certificates the
    /// system trusts are accepted, and so are ones the user has pinned.
    public static func make(baseURL: URL, token: String? = nil, pins: PinStore) -> TVMasterClient {
        guard baseURL.scheme?.lowercased() == "https" else { return TVMasterClient(baseURL: baseURL, token: token) }
        let delegate = TrustDelegate(pins: pins)
        let config = URLSessionConfiguration.default
        config.waitsForConnectivity = false
        let session = URLSession(configuration: config, delegate: delegate, delegateQueue: nil)
        return TVMasterClient(baseURL: baseURL, token: token, session: session, trust: delegate)
    }

    /// The session this client uses, for things that need the same trust (the WebSocket, the proxy).
    public var urlSession: URLSession { session }

    public var isHTTPS: Bool { baseURL.scheme?.lowercased() == "https" }

    /// `host:port` of the server, the key certificates are pinned under.
    public var pinKey: String { PinStore.key(for: baseURL) }

    func mapped(_ error: Error) -> TVMasterError {
        if let trust, let r = trust.lastRejected, let code = (error as? URLError)?.code,
           [.cancelled, .serverCertificateUntrusted, .secureConnectionFailed, .serverCertificateHasUnknownRoot,
            .serverCertificateNotYetValid, .serverCertificateHasBadDate, .clientCertificateRejected].contains(code) {
            return .untrustedCertificate(host: r.host, fingerprint: r.fingerprint)
        }
        return .unreachable
    }

    public enum Reachability: Equatable, Sendable {
        case up
        case down
        case untrusted(host: String, fingerprint: String)
    }

    /// Is the server there, and (for HTTPS) is its certificate acceptable? The health endpoint is public.
    public func reachability(timeout: TimeInterval = 3) async -> Reachability {
        trust?.forgetRejection()
        var req = URLRequest(url: url("/api/v1/health"))
        req.timeoutInterval = timeout
        do {
            let (_, response) = try await session.data(for: req)
            return (response as? HTTPURLResponse)?.statusCode == 200 ? .up : .down
        } catch {
            if case .untrustedCertificate(let host, let fp) = mapped(error) { return .untrusted(host: host, fingerprint: fp) }
            return .down
        }
    }

    // MARK: URLs

    public var isLocal: Bool {
        guard let host = baseURL.host?.lowercased() else { return false }
        return host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]"
    }

    private func url(_ path: String, query: [URLQueryItem] = []) -> URL {
        var c = URLComponents(url: baseURL, resolvingAgainstBaseURL: false) ?? URLComponents()
        let base = c.path.hasSuffix("/") ? String(c.path.dropLast()) : c.path
        c.path = base + path
        c.queryItems = query.isEmpty ? nil : query
        return c.url ?? baseURL
    }

    /// Where a channel's HLS stream is. The token rides along as a query item
    /// so players that cannot send headers (AVPlayer) can still fetch it; the
    /// server puts it on every segment URL as well.
    public func streamURL(channel: String) -> URL {
        url("/stream/\(channel)/index.m3u8", query: token.map { [URLQueryItem(name: "token", value: $0)] } ?? [])
    }

    public func webURL(_ path: String) -> URL {
        url(path)
    }

    public var playlistURL: URL { url("/iptv/playlist.m3u", query: token.map { [URLQueryItem(name: "token", value: $0)] } ?? []) }
    public var guideURL: URL { url("/iptv/guide.xml", query: token.map { [URLQueryItem(name: "token", value: $0)] } ?? []) }

    // MARK: transport

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    func makeRequest(_ method: String, _ path: String, query: [URLQueryItem] = [], body: JSONValue? = nil) throws -> URLRequest {
        var req = URLRequest(url: url("/api/v1" + path, query: query))
        req.httpMethod = method
        req.timeoutInterval = 30
        if let token { req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body {
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONEncoder().encode(body)
        }
        return req
    }

    @discardableResult
    func send(_ method: String, _ path: String, query: [URLQueryItem] = [], body: JSONValue? = nil) async throws -> Data {
        let req = try makeRequest(method, path, query: query, body: body)
        let data: Data, response: URLResponse
        do {
            (data, response) = try await session.data(for: req)
        } catch {
            throw mapped(error)
        }
        guard let http = response as? HTTPURLResponse else { throw TVMasterError.invalidResponse }
        if http.statusCode == 401 { throw TVMasterError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: data))?["error"]
                ?? "Server error \(http.statusCode)"
            throw TVMasterError.http(http.statusCode, message)
        }
        return data
    }

    func get<T: Decodable>(_ path: String, query: [URLQueryItem] = [], as type: T.Type = T.self) async throws -> T {
        let data = try await send("GET", path, query: query)
        do { return try Self.decoder.decode(T.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    func post<T: Decodable>(_ path: String, body: JSONValue = .object([:]), as type: T.Type = T.self) async throws -> T {
        let data = try await send("POST", path, body: body)
        do { return try Self.decoder.decode(T.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    // MARK: API

    /// Whether anything answers at `baseURL` (the health endpoint is public).
    public static func isReachable(_ baseURL: URL, session: URLSession = .shared) async -> Bool {
        var req = URLRequest(url: baseURL.appendingPathComponent("api/v1/health"))
        req.timeoutInterval = 1.5
        guard let (_, response) = try? await session.data(for: req) else { return false }
        return (response as? HTTPURLResponse)?.statusCode == 200
    }

    public func auth() async throws -> AuthStatus { try await get("/auth") }
    public func system() async throws -> SystemInfo { try await get("/system") }

    public func channels() async throws -> [Channel] { try await get("/channels") }

    /// Create or update a channel. Updates are partial: fields you leave out are kept.
    @discardableResult
    public func saveChannel(_ fields: [String: JSONValue]) async throws -> Channel {
        try await post("/channels", body: .object(fields))
    }

    public func deleteChannel(_ id: String) async throws { try await send("DELETE", "/channels/\(id)") }

    @discardableResult
    public func reroll(channel id: String) async throws -> Int {
        struct R: Decodable { var items: Int }
        return try await post("/channels/\(id)/reroll", as: R.self).items
    }

    public func schedule(channel id: String, hours: Int = 6) async throws -> [Program] {
        try await get("/channels/\(id)/schedule", query: [URLQueryItem(name: "hours", value: String(hours))])
    }

    public func scheduleItems(channel id: String, hours: Int = 3) async throws -> [ScheduleItem] {
        try await get("/channels/\(id)/schedule", query: [
            URLQueryItem(name: "hours", value: String(hours)), URLQueryItem(name: "detail", value: "items"),
        ])
    }

    public func tvs() async throws -> [Tv] { try await get("/tvs") }

    @discardableResult
    public func registerTv(id: String, name: String) async throws -> Tv {
        try await post("/tvs", body: .object(["id": .string(id), "name": .string(name)]))
    }

    @discardableResult
    public func tune(tv id: String, number: Int) async throws -> Tv {
        try await post("/tvs/\(id)/channel", body: .object(["number": .number(Double(number))]))
    }

    @discardableResult
    public func step(tv id: String, up: Bool) async throws -> Tv {
        try await post("/tvs/\(id)/channel", body: .object(["step": .string(up ? "up" : "down")]))
    }

    @discardableResult
    public func power(tv id: String, on: Bool) async throws -> Tv {
        try await post("/tvs/\(id)/power", body: .object(["on": .bool(on)]))
    }

    @discardableResult
    public func volume(tv id: String, level: Int? = nil, muted: Bool? = nil) async throws -> Tv {
        var body: [String: JSONValue] = [:]
        if let level { body["level"] = .number(Double(level)) }
        if let muted { body["muted"] = .bool(muted) }
        return try await post("/tvs/\(id)/volume", body: .object(body))
    }

    public func deleteTv(_ id: String) async throws { try await send("DELETE", "/tvs/\(id)") }

    public func libraries() async throws -> [Library] { try await get("/libraries") }

    @discardableResult
    public func addLibrary(path: String, kind: LibraryKind) async throws -> Library {
        try await post("/libraries", body: .object(["path": .string(path), "kind": .string(kind.rawValue)]))
    }

    @discardableResult
    public func scan(library id: Int) async throws -> ScanResult { try await post("/libraries/\(id)/scan") }

    public func analyze(library id: Int) async throws {
        struct R: Decodable { var queued: Int }
        _ = try await post("/libraries/\(id)/analyze", as: R.self)
    }

    public func setEnabled(library id: Int, _ enabled: Bool) async throws {
        _ = try await send("PATCH", "/libraries/\(id)", body: .object(["enabled": .bool(enabled)]))
    }

    public func deleteLibrary(_ id: Int) async throws { try await send("DELETE", "/libraries/\(id)") }

    public func shows() async throws -> [ShowSummary] { try await get("/shows") }
}
