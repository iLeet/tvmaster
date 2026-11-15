import Foundation
import Testing
@testable import TVMasterKit

// MARK: - a stub transport so the client can be tested without a server

final class StubProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var handler: (@Sendable (URLRequest) -> (Int, Data))?
    nonisolated(unsafe) static var lastRequest: URLRequest?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        Self.lastRequest = request
        let (status, data) = Self.handler?(request) ?? (500, Data())
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

func stubSession() -> URLSession {
    let c = URLSessionConfiguration.ephemeral
    c.protocolClasses = [StubProtocol.self]
    return URLSession(configuration: c)
}

func json(_ s: String) -> Data { Data(s.utf8) }

let channelJSON = """
{"id":"retro","number":4,"name":"Retro TV","strategy":"block_grid","run_out":"keep_going","autopilot":true,"chaos":10,
 "seed":1794719864216,"epoch_ms":1794719864216,"always_on":false,"library_ids":[1,2],
 "lineup":{"timezone":"America/Toronto","grid_minutes":30,"blocks":[{"name":"Prime"},{"name":"Late"}],"seasons":[],"events":[{"name":"x"}],"ads":{"pod_seconds":120}},
 "kind":"programming","group":"Sitcoms","guide":{"hours":3,"title":null,"ticker":[],"music_library_ids":[]},
 "on_air":true,
 "now":{"media_id":7,"show":"Cheers","title":"Pilot","season":1,"episode":1,"start_ms":1000,"end_ms":1801000,"runtime_end_ms":1321000,"label":"Prime"},
 "next":null}
"""

// MARK: - models

@Suite("Models")
struct ModelTests {
    @Test func decodesAChannelWithWhatIsOnAir() throws {
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase
        let c = try d.decode(Channel.self, from: json(channelJSON))
        #expect(c.id == "retro" && c.number == 4 && c.strategy == .blockGrid && c.runOut == .keepGoing)
        #expect(c.libraryIds == [1, 2] && c.group == "Sitcoms" && c.onAir == true)
        #expect(c.now?.headline == "Cheers" && c.now?.subtitle == "Pilot" && c.now?.episodeCode == "S01E01")
        #expect(c.next == nil)
        #expect(c.timeZone?.identifier == "America/Toronto")
        #expect(c.lineupSummary == "2 blocks · 1 event · 30 min grid · ads on")
    }

    @Test func programsWithoutShowsUseTheirTitle() {
        let p = Program(mediaId: 1, show: nil, title: "A Film", season: nil, episode: nil, startMs: 0, endMs: 1000, runtimeEndMs: 1000, label: nil)
        #expect(p.headline == "A Film" && p.subtitle == nil && p.episodeCode == nil)
    }

    @Test func jsonValueRoundTripsAndFindsSnakeOrCamelKeys() throws {
        let raw = json(#"{"grid_minutes":30,"nested":{"a":[1,"two",null,true]}}"#)
        let v = try JSONDecoder().decode(JSONValue.self, from: raw)
        #expect(v["grid_minutes"]?.intValue == 30)
        #expect(v["gridMinutes"]?.intValue == 30)
        #expect(v["nested"]?["a"]?.arrayValue?.count == 4)
        let again = try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(v))
        #expect(again == v)
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase
        #expect(try d.decode(JSONValue.self, from: raw)["grid_minutes"]?.intValue == 30, "survives the snake-case strategy")
    }

    @Test func decodesTvsLibrariesAndSystemInfo() throws {
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase
        let tv = try d.decode(Tv.self, from: json(#"{"id":"den","name":"Den","channel":null,"power":false,"volume":40,"muted":true,"online":true}"#))
        #expect(tv.channel == nil && tv.online && tv.muted)
        let lib = try d.decode(Library.self, from: json(#"{"id":2,"path":"/Users/me/TV/Shows","kind":"shows","enabled":true,"items":120,"analysis":{"total":10,"done":4,"running":true}}"#))
        #expect(lib.name == "Shows" && lib.kind == .shows && lib.analysis?.running == true)
        let sys = try d.decode(SystemInfo.self, from: json(#"{"version":"0.0.1","os":"macos","arch":"aarch64","timezone":"America/Toronto","ffmpeg":{"version":"9.0.2","encoder":"VideoToolbox","has_libx264":true,"has_videotoolbox":true,"has_nvenc":false,"has_drawtext":false},"on_air":["retro"],"auth_required":true}"#))
        #expect(sys.ffmpeg?.hasVideotoolbox == true && sys.authRequired && sys.onAir == ["retro"])
    }
}

@Suite("Formatting")
struct FormatTests {
    @Test func durations() {
        #expect(Format.duration(45_000) == "45s")
        #expect(Format.duration(22 * 60_000) == "22m")
        #expect(Format.duration(65 * 60_000) == "1h 05m")
        #expect(Format.duration(2 * 3_600_000) == "2h")
        #expect(Format.duration(-5) == "0s")
    }

    @Test func slugsAreValidIDs() {
        #expect(Format.slug("Retro TV") == "retro-tv")
        #expect(Format.slug("  Café  Noir!! ") == "cafe-noir")
        #expect(Format.slug("日本語") == "channel")
        #expect(Format.slug(String(repeating: "a", count: 50)).count == 32)
        for name in ["Toon Town", "Café", "A  B", "x"] { #expect(Format.isValidID(Format.slug(name))) }
        #expect(!Format.isValidID("Bad Id") && !Format.isValidID("") && !Format.isValidID("-x"))
    }

    @Test func nextFreeChannelNumber() {
        #expect(Format.nextFreeNumber(taken: []) == 2)
        #expect(Format.nextFreeNumber(taken: [2, 3, 5]) == 4)
    }
}

// MARK: - client

@Suite("Client", .serialized)
struct ClientTests {
    let base = URL(string: "http://tv.local:8787")!

    @Test func buildsURLsIncludingTheTokenForPlayers() {
        let c = TVMasterClient(baseURL: base, token: "secret-token-1")
        #expect(c.streamURL(channel: "retro").absoluteString == "http://tv.local:8787/stream/retro/index.m3u8?token=secret-token-1")
        #expect(c.webURL("/admin/#/channels/retro").absoluteString.hasPrefix("http://tv.local:8787/admin/"))
        let open = TVMasterClient(baseURL: base)
        #expect(open.streamURL(channel: "retro").absoluteString == "http://tv.local:8787/stream/retro/index.m3u8")
        #expect(open.playlistURL.absoluteString == "http://tv.local:8787/iptv/playlist.m3u")
        #expect(!open.isLocal)
        #expect(TVMasterClient(baseURL: URL(string: "http://127.0.0.1:8787")!).isLocal)
        #expect(TVMasterClient(baseURL: URL(string: "http://localhost:8787/prefix/")!).streamURL(channel: "a").path == "/prefix/stream/a/index.m3u8")
    }

    @Test func listsChannelsAndSendsTheBearerToken() async throws {
        StubProtocol.handler = { _ in (200, json("[\(channelJSON)]")) }
        let c = TVMasterClient(baseURL: base, token: "abc12345", session: stubSession())
        let list = try await c.channels()
        #expect(list.count == 1 && list[0].name == "Retro TV")
        #expect(StubProtocol.lastRequest?.value(forHTTPHeaderField: "Authorization") == "Bearer abc12345")
        #expect(StubProtocol.lastRequest?.url?.path == "/api/v1/channels")
    }

    @Test func mapsErrors() async {
        let c = TVMasterClient(baseURL: base, session: stubSession())
        StubProtocol.handler = { _ in (401, json(#"{"error":"unauthorized"}"#)) }
        await #expect(throws: TVMasterError.unauthorized) { _ = try await c.channels() }
        StubProtocol.handler = { _ in (400, json(#"{"error":"invalid lineup: bad time"}"#)) }
        await #expect(throws: TVMasterError.http(400, "invalid lineup: bad time")) { _ = try await c.channels() }
        StubProtocol.handler = { _ in (200, json("not json")) }
        await #expect(throws: TVMasterError.invalidResponse) { _ = try await c.channels() }
    }

    @Test func partialChannelUpdatesSendOnlyTheGivenFields() async throws {
        StubProtocol.handler = { _ in (200, json(channelJSON)) }
        let c = TVMasterClient(baseURL: base, session: stubSession())
        _ = try await c.saveChannel(["id": .string("retro"), "number": .number(4), "name": .string("Renamed")])
        let req = StubProtocol.lastRequest
        #expect(req?.httpMethod == "POST" && req?.url?.path == "/api/v1/channels")
        // URLProtocol receives the body as a stream
        var body = Data()
        if let s = req?.httpBodyStream { s.open(); var buf = [UInt8](repeating: 0, count: 1024); while s.hasBytesAvailable { let n = s.read(&buf, maxLength: 1024); if n <= 0 { break }; body.append(buf, count: n) }; s.close() }
        let sent = try JSONDecoder().decode([String: JSONValue].self, from: body.isEmpty ? (req?.httpBody ?? Data()) : body)
        #expect(Set(sent.keys) == ["id", "number", "name"])
    }

    @Test func tvControlsHitTheRightEndpoints() async throws {
        let tv = #"{"id":"den","name":"Den","channel":5,"power":true,"volume":50,"muted":false,"online":false}"#
        StubProtocol.handler = { _ in (200, json(tv)) }
        let c = TVMasterClient(baseURL: base, session: stubSession())
        _ = try await c.tune(tv: "den", number: 5)
        #expect(StubProtocol.lastRequest?.url?.path == "/api/v1/tvs/den/channel")
        _ = try await c.step(tv: "den", up: true)
        _ = try await c.power(tv: "den", on: false)
        #expect(StubProtocol.lastRequest?.url?.path == "/api/v1/tvs/den/power")
        _ = try await c.volume(tv: "den", level: 30)
        #expect(StubProtocol.lastRequest?.url?.path == "/api/v1/tvs/den/volume")
    }

    @Test func noContentResponsesAreFine() async throws {
        StubProtocol.handler = { _ in (204, Data()) }
        let c = TVMasterClient(baseURL: base, session: stubSession())
        try await c.deleteChannel("retro")
        #expect(StubProtocol.lastRequest?.httpMethod == "DELETE")
    }

    @Test func anUnreachableServerIsReportedAsSuch() async {
        let c = TVMasterClient(baseURL: URL(string: "http://127.0.0.1:1")!)
        await #expect(throws: TVMasterError.unreachable) { _ = try await c.system() }
        #expect(await TVMasterClient.isReachable(URL(string: "http://127.0.0.1:1")!) == false)
    }
}

// MARK: - events

@Suite("Events")
struct EventTests {
    @Test func parsesEveryKindOfMessage() throws {
        let tv = #"{"id":"den","name":"Den","channel":5,"power":true,"volume":50,"muted":false,"online":true}"#
        #expect(EventStream.parse(json(#"{"type":"tv.state","tv":\#(tv)}"#)) == .tvState(try JSONDecoder.snake.decode(Tv.self, from: json(tv))))
        if case .snapshot(let tvs)? = EventStream.parse(json(#"{"type":"snapshot","tvs":[\#(tv)],"channels":[]}"#)) { #expect(tvs.count == 1) } else { Issue.record("no snapshot") }
        #expect(EventStream.parse(json(#"{"type":"tv.removed","id":"den"}"#)) == .tvRemoved("den"))
        #expect(EventStream.parse(json(#"{"type":"schedule.changed","channel":"retro"}"#)) == .scheduleChanged(channel: "retro"))
        if case .channelNow(let ch, let now, let next)? = EventStream.parse(json(#"{"type":"channel.now","channel":"retro","now":{"media_id":1,"show":"A","title":"t","season":null,"episode":null,"start_ms":1,"end_ms":2,"runtime_end_ms":2,"label":null},"next":null}"#)) {
            #expect(ch == "retro" && now?.headline == "A" && next == nil)
        } else { Issue.record("no channel.now") }
    }

    @Test func ignoresUnknownAndMalformedMessages() {
        #expect(EventStream.parse(json(#"{"type":"pong"}"#)) == nil)
        #expect(EventStream.parse(json(#"{"type":"error","message":"x"}"#)) == nil)
        #expect(EventStream.parse(json("garbage")) == nil)
        #expect(EventStream.parse(json(#"{"type":"tv.state"}"#)) == nil)
    }

    @Test func socketURLFollowsTheServerAddress() {
        let plain = EventStream(client: TVMasterClient(baseURL: URL(string: "http://tv.local:8787")!, token: "tok12345"))
        #expect(plain.socketRequest()?.url?.absoluteString == "ws://tv.local:8787/api/v1/ws")
        #expect(plain.socketRequest()?.value(forHTTPHeaderField: "Authorization") == "Bearer tok12345")
        let secure = EventStream(client: TVMasterClient(baseURL: URL(string: "https://tv.example.com")!))
        #expect(secure.socketRequest()?.url?.absoluteString == "wss://tv.example.com/api/v1/ws")
    }
}

extension JSONDecoder {
    static var snake: JSONDecoder { let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase; return d }
}

// MARK: - daemon and keychain

@Suite("Daemon and token store")
struct DaemonTests {
    @Test func prefersTheBundledBinaryAndFindsInstalledOnes() {
        let list = DaemonLocator.candidates(
            bundleResources: URL(fileURLWithPath: "/Applications/TVMaster.app/Contents/Resources"),
            environment: ["TVMASTER_BIN": "/custom/tvmaster", "PATH": "/usr/bin:/opt/tools"],
            home: URL(fileURLWithPath: "/Users/me")
        ).map(\.path)
        #expect(list.first == "/Applications/TVMaster.app/Contents/Resources/tvmaster")
        #expect(list.contains("/custom/tvmaster") && list.contains("/opt/homebrew/bin/tvmaster"))
        #expect(list.contains("/Users/me/.cargo/bin/tvmaster") && list.contains("/opt/tools/tvmaster"))
        #expect(DaemonLocator.find(candidates: list.map { URL(fileURLWithPath: $0) }, isExecutable: { $0 == "/custom/tvmaster" })?.path == "/custom/tvmaster")
        #expect(DaemonLocator.find(candidates: [], isExecutable: { _ in true }) == nil)
    }

    @Test func putsHomebrewOnThePathForFFmpeg() {
        let env = DaemonLocator.environment(base: ["PATH": "/usr/bin:/bin:/opt/homebrew/bin"])
        let parts = env["PATH"]!.split(separator: ":").map(String.init)
        #expect(parts.first == "/opt/homebrew/bin" && parts.contains("/usr/bin"))
        #expect(parts.filter { $0 == "/opt/homebrew/bin" }.count == 1)
    }

    @Test func tokensRoundTripThroughTheKeychain() {
        let store = TokenStore(service: "app.tvmaster.tests.\(UUID().uuidString)")
        let server = "http://tv.local:8787"
        #expect(store.token(for: server) == nil)
        #expect(store.set("first-token-123", for: server))
        #expect(store.token(for: server) == "first-token-123")
        #expect(store.set("second-token-456", for: server))
        #expect(store.token(for: server) == "second-token-456")
        #expect(store.token(for: "http://other:1") == nil)
        #expect(store.set(nil, for: server))
        #expect(store.token(for: server) == nil)
    }

    @Test func aMissingBinaryFailsCleanly() async {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("tvm-\(UUID().uuidString).log")
        let sup = DaemonSupervisor(logURL: log)
        await sup.start(binary: URL(fileURLWithPath: "/nonexistent/tvmaster"), bind: "127.0.0.1:1", dataDirectory: FileManager.default.temporaryDirectory.appendingPathComponent("tvm-data"), token: nil)
        if case .failed = await sup.state {} else { Issue.record("expected a failure") }
        await sup.stop()
    }

    @Test func supervisesARealProcessAndStopsIt() async throws {
        // A stand-in "server": a script that ignores its arguments and sleeps.
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("tvm-fake-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let script = dir.appendingPathComponent("tvmaster")
        try "#!/bin/sh\necho started \"$@\"\nsleep 30\n".write(to: script, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)

        let sup = DaemonSupervisor(logURL: dir.appendingPathComponent("app.log"))
        await sup.start(binary: script, bind: "127.0.0.1:9", dataDirectory: dir.appendingPathComponent("data"), token: "tok-12345678")
        guard case .running = await sup.state else { Issue.record("not running"); return }
        try await Task.sleep(nanoseconds: 400_000_000)
        let log = await sup.logTail()
        #expect(log.contains("started serve --bind 127.0.0.1:9") && log.contains("--token tok-12345678"))
        await sup.stop()
        try await Task.sleep(nanoseconds: 600_000_000)
        #expect(await sup.state == .stopped)
        try? FileManager.default.removeItem(at: dir)
    }
}
