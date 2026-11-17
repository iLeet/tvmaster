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
            // A request the app itself abandoned (the view changed under it) is not a server problem.
            if error is CancellationError || (error as? URLError)?.code == .cancelled && trust?.lastRejected == nil {
                throw CancellationError()
            }
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

    /// A channel exactly as the server wrote it (no key rewriting), for editing the
    /// parts of it the typed model does not carry and sending them back unchanged.
    public func channelRaw(_ id: String) async throws -> JSONValue {
        let data = try await send("GET", "/channels/\(id)")
        guard let any = try? JSONSerialization.jsonObject(with: data) else { throw TVMasterError.invalidResponse }
        return JSONValue(any: any)
    }

    public func auth() async throws -> AuthStatus { try await get("/auth") }
    public func security() async throws -> SecurityStatus { try await get("/security") }

    // MARK: time zone and channel controls

    public func timezone() async throws -> TimezoneState { try await get("/timezone") }

    /// Set the station's time zone (`nil` = automatic), optionally moving every channel's schedule into it.
    @discardableResult
    public func setTimezone(_ zone: String?, applyToChannels: Bool) async throws -> TimezoneState {
        let body: JSONValue = .object(["zone": zone.map { .string($0) } ?? .null, "apply_to_channels": .bool(applyToChannels)])
        let data = try await send("PUT", "/timezone", body: body)
        do { return try Self.decoder.decode(TimezoneState.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    /// `schedule`: re-plan from now. `progress`: every show starts again at its first episode.
    public func reset(channel id: String, what: String) async throws {
        try await send("POST", "/channels/\(id)/reset", body: .object(["what": .string(what)]))
    }

    /// `start`, `stop` or `restart` a channel's stream.
    public func stream(channel id: String, action: String) async throws {
        try await send("POST", "/channels/\(id)/stream", body: .object(["action": .string(action)]))
    }

    /// Stop the server cleanly. Docker, systemd and the Windows service start it again; a server started by hand just stops.
    public func restartServer() async throws { try await send("POST", "/server/restart", body: .object([:])) }

    // MARK: live control

    public func live() async throws -> LiveState { try await get("/live") }

    /// Put `channels` (empty = all) on the test screen, a blank, or an emergency alert.
    @discardableResult
    public func takeOver(channels: [String], mode: String, title: String? = nil, message: String? = nil,
                         sample: String? = nil, minutes: Int? = nil, style: String? = nil, theme: String? = nil,
                         severity: String? = nil, sound: String? = nil) async throws -> LiveState {
        var body: [String: JSONValue] = ["channels": .array((channels.isEmpty ? ["all"] : channels).map { .string($0) }), "mode": .string(mode)]
        if let title { body["title"] = .string(title) }
        if let message { body["message"] = .string(message) }
        if let sample { body["sample"] = .string(sample) }
        if let minutes, minutes > 0 { body["minutes"] = .number(Double(minutes)) }
        if let style { body["style"] = .string(style) }
        if let theme { body["theme"] = .string(theme) }
        if let severity { body["severity"] = .string(severity) }
        if let sound { body["sound"] = .string(sound) }
        let data = try await send("POST", "/live", body: .object(body))
        do { return try Self.decoder.decode(LiveState.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    /// The station's themes (the one list every screen and app uses) and the one chosen.
    public func themeCatalogue() async throws -> ThemeCatalogue { try await get("/theme") }

    /// The station's chosen theme, as the server wrote it.
    public func stationThemeJSON() async throws -> JSONValue? {
        let data = try await send("GET", "/theme")
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return JSONValue(any: any).get(["station"])
    }

    /// Choose the station theme.
    public func setStationTheme(_ choice: ThemeChoice) async throws {
        _ = try await send("PUT", "/theme", body: choice.json)
    }

    /// A PNG of one of the station's graphics (`card`, `bumper`, `guide`, `alert`, `bar`) in a theme.
    public func themePreview(what: String, choice: ThemeChoice, id: String? = nil, name: String? = nil, number: Int? = nil) async throws -> Data {
        var body: [String: JSONValue] = ["what": .string(what), "theme": choice.json, "t": .number(2)]
        if let id { body["id"] = .string(id) }
        if let name { body["name"] = .string(name) }
        if let number { body["number"] = .number(Double(number)) }
        return try await send("POST", "/theme/preview", body: .object(body))
    }

    /// A PNG of an alert as it would look (nothing goes on air).
    public func alertPreview(style: String, theme: String, severity: String, title: String, message: String) async throws -> Data {
        try await send("POST", "/live/preview", body: .object([
            "style": .string(style), "theme": .string(theme), "severity": .string(severity),
            "title": .string(title), "message": .string(message), "t": .number(1.2),
        ]))
    }

    /// Give `channels` (empty = all) back to their programming.
    @discardableResult
    public func giveBack(channels: [String]) async throws -> LiveState {
        let body: JSONValue = .object(["channels": .array((channels.isEmpty ? ["all"] : channels).map { .string($0) })])
        let data = try await send("POST", "/live/clear", body: body)
        do { return try Self.decoder.decode(LiveState.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    public func setOpenViewing(_ open: Bool) async throws {
        try await send("PUT", "/security", body: .object(["open_viewing": .bool(open)]))
    }

    // MARK: monitoring

    public func monitor() async throws -> MonitorState { try await get("/monitor") }

    @discardableResult
    public func setMonitor(_ s: MonitorSettings) async throws -> MonitorState {
        var body: [String: JSONValue] = [
            "watch_minutes": .number(Double(s.watchMinutes)), "alerts": .bool(s.alerts),
            "webhook_text": .bool(s.webhookText), "notify_recovery": .bool(s.notifyRecovery),
        ]
        body["webhook"] = (s.webhook?.isEmpty ?? true) ? .null : .string(s.webhook!)
        let data = try await send("PUT", "/monitor", body: .object(body))
        do { return try Self.decoder.decode(MonitorState.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }

    public func testWebhook() async throws { try await send("POST", "/monitor/test", body: .object([:])) }

    public func rescanAllLibraries() async throws { try await send("POST", "/monitor/rescan", body: .object([:])) }

    // MARK: pictures

    /// Upload a png, jpg, gif or webp (up to 12 MB); the result's `path` goes into a channel's options.
    public func uploadAsset(_ data: Data, name: String) async throws -> Asset {
        var req = try makeRequest("POST", "/assets", query: [URLQueryItem(name: "name", value: name)])
        req.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        req.httpBody = data
        req.timeoutInterval = 120
        let (body, response): (Data, URLResponse)
        do { (body, response) = try await session.data(for: req) } catch { throw mapped(error) }
        guard let http = response as? HTTPURLResponse else { throw TVMasterError.invalidResponse }
        if http.statusCode == 401 { throw TVMasterError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: body))?["error"] ?? "Server error \(http.statusCode)"
            throw TVMasterError.http(http.statusCode, message)
        }
        do { return try Self.decoder.decode(Asset.self, from: body) } catch { throw TVMasterError.invalidResponse }
    }

    public func assets() async throws -> [Asset] { try await get("/assets") }

    // MARK: guide preview

    /// A PNG of a guide as it would look with these settings (nothing needs saving).
    public func guidePreview(id: String, name: String, guide: JSONValue) async throws -> Data {
        try await send("POST", "/guide/preview", body: .object(["id": .string(id), "name": .string(name), "guide": guide, "t": .number(3)]))
    }

    /// The bytes of an uploaded picture, for a preview.
    public func assetData(id: String) async throws -> Data { try await send("GET", "/assets/\(id)") }

    /// The id of an uploaded picture from the path stored in a channel's options (nil for any other file).
    public static func assetID(forPath path: String) -> String? {
        let comps = path.split(whereSeparator: { $0 == "/" || $0 == "\\" })
        guard comps.count >= 2, comps[comps.count - 2] == "assets", let last = comps.last else { return nil }
        let id = String(last)
        let ok = id.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || "-_.".contains($0)) }
        guard ok, !id.contains(".."), ["png", "jpg", "gif", "webp"].contains(id.split(separator: ".").last.map(String.init) ?? "") else { return nil }
        return id
    }

    /// Turn the access token on or off, or replace it. Returns the state after the change.
    @discardableResult
    public func setSecurity(enabled: Bool, token: String? = nil, rotate: Bool = false) async throws -> SecurityStatus {
        var body: [String: JSONValue] = ["enabled": .bool(enabled), "rotate": .bool(rotate)]
        if let token, !token.isEmpty { body["token"] = .string(token) }
        let data = try await send("PUT", "/security", body: .object(body))
        do { return try Self.decoder.decode(SecurityStatus.self, from: data) } catch { throw TVMasterError.invalidResponse }
    }
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

    /// Put the channel on the plan for this moment, even part way through a show.
    public func refresh(channel id: String) async throws { try await send("POST", "/channels/\(id)/refresh", body: .object([:])) }

    /// Cut to something right now; the schedule carries on after it.
    public func play(channel id: String, _ target: AirTarget) async throws {
        try await send("POST", "/channels/\(id)/play", body: .object(target.body))
    }

    /// Change what airs between two times (until the next re-roll).
    public func editSchedule(channel id: String, from: Date, to: Date, _ target: AirTarget) async throws {
        var body = target.body
        body["from_ms"] = .number((from.timeIntervalSince1970 * 1000).rounded())
        body["to_ms"] = .number((to.timeIntervalSince1970 * 1000).rounded())
        try await send("POST", "/channels/\(id)/schedule/edit", body: .object(body))
    }

    public func episodes(channel id: String, show: String) async throws -> [Episode] {
        try await get("/channels/\(id)/episodes", query: [URLQueryItem(name: "show", value: show)])
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
    public func addLibrary(path: String, kind: LibraryKind, create: Bool = false) async throws -> Library {
        try await post("/libraries", body: .object(["path": .string(path), "kind": .string(kind.rawValue), "create": .bool(create)]))
    }

    /// Copy a file from this Mac into a library's folder on the server (streamed from disk, so a
    /// large video is fine). `path` is where it goes inside the library, like `Show/Season 1/Episode.mkv`.
    /// `progress` is called with 0...1 as it goes.
    @discardableResult
    public func uploadFile(library id: Int, path: String, from file: URL, overwrite: Bool = false, progress: @escaping @Sendable (Double) -> Void) async throws -> UploadResult {
        var req = try makeRequest("PUT", "/libraries/\(id)/files", query: [URLQueryItem(name: "path", value: path)] + (overwrite ? [URLQueryItem(name: "overwrite", value: "true")] : []))
        req.timeoutInterval = 3600
        let tracker = UploadProgress(progress)
        let (body, response): (Data, URLResponse)
        do { (body, response) = try await session.upload(for: req, fromFile: file, delegate: tracker) } catch { throw mapped(error) }
        guard let http = response as? HTTPURLResponse else { throw TVMasterError.invalidResponse }
        if http.statusCode == 401 { throw TVMasterError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            let message = (try? JSONDecoder().decode([String: String].self, from: body))?["error"] ?? "Server error \(http.statusCode)"
            throw TVMasterError.http(http.statusCode, message)
        }
        do { return try Self.decoder.decode(UploadResult.self, from: body) } catch { throw TVMasterError.invalidResponse }
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

    /// Make the station's own media folder on the server, with a library for each kind inside it.
    public func createStationFolder() async throws {
        _ = try await send("POST", "/media/station-folder")
    }

    /// Whether "season N" is split off folder names by itself.
    public func autoCleanNames() async throws -> Bool {
        let data = try await send("GET", "/shows/names")
        return (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["auto_clean"] as? Bool ?? true
    }

    public func setAutoCleanNames(_ on: Bool) async throws {
        _ = try await send("PUT", "/shows/auto-clean", body: .object(["enabled": .bool(on)]))
    }

    /// Call `shows` one show named `title` (a rename, or several shows put together).
    public func mergeShows(_ shows: [String], title: String) async throws {
        _ = try await send("POST", "/shows/merge", body: .object(["shows": .array(shows.map { .string($0) }), "title": .string(title)]))
    }

    /// Say which season a folder is.
    public func setFolderSeason(raw: String, title: String, season: Int?) async throws {
        _ = try await send("PUT", "/shows/folder", body: .object(["raw": .string(raw), "title": .string(title), "season": season.map { .number(Double($0)) } ?? .null]))
    }

    /// Forget a folder's own rule.
    public func resetFolder(raw: String) async throws {
        _ = try await send("POST", "/shows/folder/reset", body: .object(["raw": .string(raw)]))
    }

    public func setShowInfo(show: String, _ info: ShowInfo) async throws {
        _ = try await send("PUT", "/shows/info", body: .object(["show": .string(show), "info": info.json]))
    }

    /// Save the settings that follow `show` onto every channel (all blank = remove them).
    public func setShowSettings(show: String, _ settings: ShowDefaults) async throws {
        try await send("PUT", "/show-settings", body: .object(["show": .string(show), "settings": settings.json]))
    }
}
