import Foundation
import Testing
@testable import TVMasterKit

/// A real `tvmaster` daemon with HTTP and a self-signed HTTPS listener, started
/// from the repository's debug build (set TVMASTER_BIN to use another). The
/// tests are skipped when there is no binary.
final class TestServer: @unchecked Sendable {
    let process = Process()
    let dir: URL
    let httpPort: Int
    let httpsPort: Int
    let token = "test-token-1234abcd"

    static func binary() -> URL? {
        if let p = ProcessInfo.processInfo.environment["TVMASTER_BIN"], FileManager.default.isExecutableFile(atPath: p) { return URL(fileURLWithPath: p) }
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let debug = repo.appendingPathComponent("target/debug/tvmaster")
        return FileManager.default.isExecutableFile(atPath: debug.path) ? debug : nil
    }

    init?() async {
        guard let bin = Self.binary() else { return nil }
        httpPort = Int.random(in: 30000..<40000)
        httpsPort = httpPort + 1
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("tvm-tls-\(UUID().uuidString)")
        process.executableURL = bin
        process.arguments = ["serve", "--bind", "127.0.0.1:\(httpPort)", "--tls", "--tls-bind", "127.0.0.1:\(httpsPort)", "--data-dir", dir.path, "--token", token]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return nil }
        let http = URL(string: "http://127.0.0.1:\(httpPort)")!
        for _ in 0..<80 {
            if await TVMasterClient.isReachable(http) { return }
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
        return nil
    }

    var httpURL: URL { URL(string: "http://127.0.0.1:\(httpPort)")! }
    var httpsURL: URL { URL(string: "https://localhost:\(httpsPort)")! }

    func fingerprint() async throws -> String {
        let info = try await TVMasterClient(baseURL: httpURL, token: token).system()
        return try #require(info.network?.tls.fingerprint)
    }

    func stop() {
        process.interrupt()
        process.waitUntilExit()
        try? FileManager.default.removeItem(at: dir)
    }
}

func freshPins() -> PinStore {
    PinStore(defaults: UserDefaults(suiteName: "tvm-test-\(UUID().uuidString)")!)
}

@Suite("HTTPS and pinned certificates", .enabled(if: TestServer.binary() != nil))
struct TLSTests {
    @Test func anUnknownCertificateIsReportedWithItsFingerprintNotSilentlyAccepted() async throws {
        let server = try #require(await TestServer())
        defer { server.stop() }
        let client = TVMasterClient.make(baseURL: server.httpsURL, token: server.token, pins: freshPins())
        let r = await client.reachability()
        guard case .untrusted(_, let fp) = r else { Issue.record("expected untrusted, got \(r)"); return }
        #expect(fp == (try await server.fingerprint()), "the fingerprint shown to the user is the server's real one")
        await #expect(throws: TVMasterError.self) { _ = try await client.channels() }
    }

    @Test func aPinnedCertificateIsAcceptedForRESTAndOnlyThatOne() async throws {
        let server = try #require(await TestServer())
        defer { server.stop() }
        let pins = freshPins()
        let real = try await server.fingerprint()

        // A wrong pin does not help.
        pins.set("00:11:22", host: "localhost", port: server.httpsPort)
        let wrong = TVMasterClient.make(baseURL: server.httpsURL, token: server.token, pins: pins)
        guard case .untrusted(_, let seen) = await wrong.reachability() else { Issue.record("a wrong pin was accepted"); return }
        #expect(seen == real)

        // The right one does.
        pins.set(real, host: "localhost", port: server.httpsPort)
        let client = TVMasterClient.make(baseURL: server.httpsURL, token: server.token, pins: pins)
        #expect(await client.reachability() == .up)
        let info = try await client.system()
        #expect(info.network?.tls.enabled == true)
        #expect(client.trust?.usesPin(host: "localhost", port: server.httpsPort) == true)
        #expect(try await client.auth().authenticated)
        // The token is still enforced over HTTPS.
        let anon = TVMasterClient.make(baseURL: server.httpsURL, token: nil, pins: pins)
        await #expect(throws: TVMasterError.unauthorized) { _ = try await anon.channels() }
    }

    @Test func liveEventsWorkOverWSSWithAPinnedCertificate() async throws {
        let server = try #require(await TestServer())
        defer { server.stop() }
        let pins = freshPins()
        pins.set(try await server.fingerprint(), host: "localhost", port: server.httpsPort)
        let client = TVMasterClient.make(baseURL: server.httpsURL, token: server.token, pins: pins)
        let stream = EventStream(client: client)
        let task = Task { () -> Bool in
            for await event in stream.events() {
                if case .snapshot = event { return true }
            }
            return false
        }
        try await Task.sleep(nanoseconds: 3_000_000_000)
        task.cancel()
        #expect(await task.value, "a snapshot arrived over wss://")
    }

    @Test func aPlainHTTPClientStillWorks() async throws {
        let server = try #require(await TestServer())
        defer { server.stop() }
        let client = TVMasterClient.make(baseURL: server.httpURL, token: server.token, pins: freshPins())
        #expect(await client.reachability() == .up)
        #expect(!client.isHTTPS)
    }

    @Test func theLoopbackRelayForwardsStreamRequestsToTheRealServer() async throws {
        let server = try #require(await TestServer())
        defer { server.stop() }
        let pins = freshPins()
        pins.set(try await server.fingerprint(), host: "localhost", port: server.httpsPort)
        let client = TVMasterClient.make(baseURL: server.httpsURL, token: server.token, pins: pins)
        // A guide channel needs no media; it is rendered live.
        _ = try await client.saveChannel(["id": .string("guide"), "number": .number(3), "name": .string("Guide"), "kind": .string("guide")])

        let proxy = PinnedProxy(upstream: server.httpsURL, session: client.urlSession)
        let port = try await proxy.start()
        defer { proxy.stop() }
        #expect(port > 0)

        // The relay answers plain HTTP on loopback.
        let local = proxy.localURL(for: client.streamURL(channel: "unknown-channel"))
        #expect(local.scheme == "http" && local.host == "127.0.0.1")
        let (_, missing) = try await URLSession.shared.data(from: local)
        #expect((missing as? HTTPURLResponse)?.statusCode == 404, "the upstream status passes through")

        // Only streams are relayed.
        var api = URLComponents(url: proxy.localURL(for: server.httpsURL), resolvingAgainstBaseURL: false)!
        api.path = "/api/v1/channels"
        let (_, denied) = try await URLSession.shared.data(from: api.url!)
        #expect((denied as? HTTPURLResponse)?.statusCode == 403)

        // A real playlist (needs FFmpeg to render the guide).
        if (try? Process.run(URL(fileURLWithPath: "/usr/bin/env"), arguments: ["ffmpeg", "-version"])) != nil {
            let playlist = proxy.localURL(for: client.streamURL(channel: "guide"))
            var req = URLRequest(url: playlist)
            req.timeoutInterval = 40
            let (data, res) = try await URLSession.shared.data(for: req)
            #expect((res as? HTTPURLResponse)?.statusCode == 200)
            let text = String(decoding: data, as: UTF8.self)
            #expect(text.hasPrefix("#EXTM3U") && text.contains("#EXTINF"), "got: \(text.prefix(80))")
            #expect(text.contains("token="), "segment lines carry the token, so AVPlayer can fetch them through the relay too")
        }
    }
}

@Suite("Relay parsing and fingerprints")
struct RelayParsingTests {
    @Test func parsesARequestHead() throws {
        let r = try #require(PinnedProxy.parse(head: "GET /stream/retro/index.m3u8?token=abc HTTP/1.1\r\nHost: 127.0.0.1:1\r\nRange: bytes=0-99\r\nUser-Agent: AppleCoreMedia"))
        #expect(r.method == "GET")
        #expect(r.target == "/stream/retro/index.m3u8?token=abc")
        #expect(r.headers["range"] == "bytes=0-99")
        #expect(PinnedProxy.parse(head: "garbage") == nil)
    }

    @Test func fingerprintsAreColonSeparatedUppercaseSHA256() {
        // SHA-256 of nothing
        #expect(Fingerprint.of(der: Data()) == "E3:B0:C4:42:98:FC:1C:14:9A:FB:F4:C8:99:6F:B9:24:27:AE:41:E4:64:9B:93:4C:A4:95:99:1B:78:52:B8:55")
    }

    @Test func pinsAreKeptPerServerAndCanBeForgotten() {
        let pins = freshPins()
        pins.set("AA:BB", host: "Tv.Local", port: 8788)
        #expect(pins.fingerprint(host: "tv.local", port: 8788) == "AA:BB", "host names are case-insensitive")
        #expect(pins.fingerprint(host: "tv.local", port: 8787) == nil, "a different port is a different server")
        pins.set(nil, host: "tv.local", port: 8788)
        #expect(pins.fingerprint(host: "tv.local", port: 8788) == nil)
        #expect(PinStore.key(for: URL(string: "https://tv.local")!) == "tv.local:443")
    }
}
