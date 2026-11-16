import Foundation
import Testing
@testable import TVMasterKit

private func decode<T: Decodable>(_ json: String) throws -> T {
    let d = JSONDecoder()
    d.keyDecodingStrategy = .convertFromSnakeCase
    return try d.decode(T.self, from: Data(json.utf8))
}

private func channel(_ id: String, _ number: Int, _ name: String) throws -> Channel {
    try decode(#"{"id":"\#(id)","number":\#(number),"name":"\#(name)","strategy":"sequential","run_out":"keep_going","autopilot":true,"chaos":0,"always_on":false,"library_ids":[],"lineup":{},"kind":"programming","group":null,"guide":{"hours":3,"title":null,"ticker":[],"music_library_ids":[]}}"#)
}

private func network(http: Int? = 8787, https: Int? = nil, hosts: [String] = ["tv.local", "10.0.0.5"]) -> SystemInfo.Network {
    SystemInfo.Network(hostname: "tv.local", hosts: hosts, httpPort: http, httpsPort: https,
                       tls: .init(enabled: https != nil, selfSigned: true, fingerprint: https != nil ? "AA:BB" : nil))
}

@Suite("Share links")
struct ShareLinksTests {
    let base = URL(string: "http://127.0.0.1:8787")!

    @Test func neverOffersLoopbackWhenARealAddressIsKnown() {
        let l = ShareLinks(network: network(), token: nil, baseURL: base)
        #expect(l.hosts == ["tv.local", "10.0.0.5"])
        // the address the app itself used comes first when it is shareable
        let remote = ShareLinks(network: network(), token: nil, baseURL: URL(string: "http://10.0.0.152:8787")!)
        #expect(remote.hosts.first == "10.0.0.152")
        // and loopback is only a last resort
        #expect(ShareLinks(network: nil, token: nil, baseURL: base).hosts == ["127.0.0.1"])
    }

    @Test func buildsOriginsForBothSchemesAndBracketsIPv6() {
        let l = ShareLinks(network: network(https: 8788), token: nil, baseURL: base)
        #expect(l.schemes == [.http, .https])
        #expect(l.origin(.init(scheme: .http, host: "tv.local", includeToken: false)) == "http://tv.local:8787")
        #expect(l.origin(.init(scheme: .https, host: "tv.local", includeToken: false)) == "https://tv.local:8788")
        #expect(l.origin(.init(scheme: .http, host: "fe80::1", includeToken: false)) == "http://[fe80::1]:8787")
    }

    @Test func httpsIsOnlyOfferedWhenTheServerListens() {
        #expect(ShareLinks(network: network(https: nil), token: nil, baseURL: base).schemes == [.http])
        #expect(ShareLinks(network: network(http: nil, https: 8788), token: nil, baseURL: base).schemes == [.https])
    }

    @Test func theTokenGoesOnlyWhereAnAppNeedsIt() throws {
        let l = ShareLinks(network: network(), token: "secret-token-1", baseURL: base)
        let o = ShareLinks.Options(scheme: .http, host: "tv.local", includeToken: true)
        let all = l.sections(o, channels: [try channel("retro", 4, "Retro")], tvs: []).flatMap(\.links)
        func url(_ id: String) -> String { all.first { $0.id == id }!.url }
        #expect(url("playlist") == "http://tv.local:8787/iptv/playlist.m3u?token=secret-token-1")
        #expect(url("guide") == "http://tv.local:8787/iptv/guide.xml?token=secret-token-1")
        #expect(url("hls-retro") == "http://tv.local:8787/stream/retro/index.m3u8?token=secret-token-1")
        #expect(url("ts-retro") == "http://tv.local:8787/stream/retro.ts?token=secret-token-1")
        // pages for people sign in instead of carrying the token
        #expect(!url("remote").contains("token"))
        #expect(!url("player").contains("token"))
        #expect(!url("tuner").contains("token"))
        #expect(all.filter(\.hasToken).map(\.id).sorted() == ["discover", "guide", "hls-retro", "playlist", "ts-retro"])

        let without = ShareLinks.Options(scheme: .http, host: "tv.local", includeToken: false)
        let plain = l.sections(without, channels: [try channel("retro", 4, "Retro")], tvs: []).flatMap(\.links)
        #expect(plain.allSatisfy { !$0.url.contains("token") })
        #expect(plain.allSatisfy { !$0.hasToken })
    }

    @Test func theTunerAddressIsHostAndPortWithoutAScheme() {
        let l = ShareLinks(network: network(https: 8788), token: "t", baseURL: base)
        #expect(l.tunerAddress(.init(scheme: .https, host: "10.0.0.5", includeToken: true)) == "10.0.0.5:8787", "Plex and friends use plain http")
    }

    @Test func listsEveryChannelInNumberOrderWithBothStreamKinds() throws {
        let l = ShareLinks(network: network(), token: nil, baseURL: base)
        let sections = l.sections(l.defaultOptions, channels: [try channel("b", 9, "Nine"), try channel("a", 2, "Two")], tvs: [])
        let channels = try #require(sections.first { $0.id == "channels" })
        #expect(channels.links.map(\.id) == ["hls-a", "ts-a", "hls-b", "ts-b"])
        #expect(channels.links[0].title == "2 · Two")
    }

    @Test func eachTVGetsItsOwnRemoteWithAQRCode() throws {
        let tv: Tv = try decode(#"{"id":"den","name":"Den","channel":4,"power":true,"volume":30,"muted":false,"online":true}"#)
        let l = ShareLinks(network: network(), token: nil, baseURL: base)
        let tvs = try #require(l.sections(l.defaultOptions, channels: [], tvs: [tv]).first { $0.id == "tvs" })
        let den = try #require(tvs.links.first { $0.id == "tv-den" })
        #expect(den.url.hasSuffix("/?remote=den") && den.qr)
        #expect(tvs.links.contains { $0.url.contains("?tv=") }, "and a link to make a new screen a TV")
    }

    @Test func defaultsToWhatTheAppIsConnectedThroughAndOnlyOffersTokensWhenThereIsOne() {
        let https = ShareLinks(network: network(https: 8788), token: "t", baseURL: URL(string: "https://tv.local:8788")!)
        #expect(https.defaultOptions.scheme == .https)
        #expect(https.defaultOptions.includeToken)
        #expect(!ShareLinks(network: network(), token: nil, baseURL: base).defaultOptions.includeToken)
        // connected over HTTPS but the server no longer lists it: fall back to what exists
        #expect(ShareLinks(network: network(https: nil), token: nil, baseURL: URL(string: "https://tv.local:8788")!).defaultOptions.scheme == .http)
    }

    @Test func randomTokensAreLongDistinctAndAcceptableToTheServer() {
        let a = Format.randomToken(), b = Format.randomToken()
        #expect(a != b && a.count == 32)
        #expect(a.allSatisfy { $0.isHexDigit })
    }
}

@Suite("QR codes")
struct QRCodeTests {
    @Test func aCodeScansBackToTheSameLink() throws {
        for url in ["http://Corys-MacBook-Air.local:8787/admin/#/remote",
                    "https://10.0.0.152:8788/?remote=living-room",
                    "http://tv.local:8787/iptv/playlist.m3u?token=ace373ed76e17614d7f38ee821f48dcf"] {
            let image = try #require(QRCode.cgImage(for: url, size: 280))
            #expect(image.width >= 200 && image.width == image.height, "square and big enough to scan")
            #expect(QRCode.decode(image) == [url])
        }
    }
}

@Suite("Picture and sound settings")
struct PictureSoundTests {
    static let server = #"""
    {"id":"anime","options":{"audio":{"languages":["jpn","eng"],"avoid_commentary":false,"loudness":"broadcast","night_mode":true,"dialogue_boost":false,"mono":false,"avoid_description":true},
     "subtitles":{"mode":"foreign","languages":["eng"],"size":"large","style":"box"},
     "video":{"aspect":"fill","quality":"1080","look":"crt","fps":24,"kbps":null,"deinterlace":"auto"},
     "overlay":{"bug":{"enabled":true,"text":"ANIME","position":"bottom_left","opacity":80},"clock":{"enabled":true,"position":"bottom_right","hour12":true},"show_title":{"enabled":true,"seconds":12}},
     "shows":{"Cowboy_Bebop":{"audio_languages":["jpn"],"look":"vhs"}}},
     "lineup":{"timezone":"UTC","blocks":[],"signoff":{"from":"01:00","to":"06:00","style":"static"},"ads":null}}
    """#

    private func raw() throws -> JSONValue {
        JSONValue(any: try JSONSerialization.jsonObject(with: Data(Self.server.utf8)))
    }

    @Test func readsTheServersSettings() throws {
        let r = try raw()
        let s = PictureSoundSettings(options: r.get(["options"]), lineup: r.get(["lineup"]))
        #expect(s.audioLanguages == ["jpn", "eng"])
        #expect(!s.avoidCommentary && s.nightMode)
        #expect(s.loudness == "broadcast" && s.subtitleMode == "foreign" && s.subtitleStyle == "box")
        #expect(s.aspect == "fill" && s.quality == "1080" && s.look == "crt")
        #expect(s.bugEnabled && s.bugText == "ANIME" && s.bugCorner == "bottom_left")
        #expect(s.titleEnabled && s.titleSeconds == 12 && s.clockEnabled)
        #expect(s.signOffEnabled && s.signOffFrom == "01:00" && s.signOffTo == "06:00" && s.signOffStyle == "static")
    }

    @Test func anOldServerWithoutOptionsGivesTheDefaults() {
        let s = PictureSoundSettings(options: nil, lineup: nil)
        #expect(s == PictureSoundSettings())
        #expect(s.avoidCommentary && s.quality == "720" && !s.signOffEnabled)
    }

    @Test func editsKeepEverythingTheEditorDoesNotShowAndUseTheServersKeyNames() throws {
        let r = try raw()
        var s = PictureSoundSettings(options: r.get(["options"]), lineup: r.get(["lineup"]))
        s.audioLanguages = ["eng"]
        s.look = "vhs"
        s.avoidCommentary = true
        let o = s.apply(to: r.get(["options"]))
        #expect(o.get(["audio", "languages"])?.stringArray == ["eng"])
        #expect(o.get(["audio", "avoid_commentary"])?.boolValue == true)
        #expect(o.get(["video", "look"])?.stringValue == "vhs")
        // not shown in the editor, so untouched, including a show name with an underscore
        #expect(o.get(["shows", "Cowboy_Bebop", "audio_languages"])?.stringArray == ["jpn"])
        #expect(o.get(["video", "fps"])?.intValue == 24)
        #expect(o.get(["overlay", "bug", "opacity"])?.intValue == 80)
        #expect(o.get(["audio", "avoid_description"])?.boolValue == true)
        // and the keys sent are the server's snake_case ones
        let json = String(decoding: try JSONEncoder().encode(o), as: UTF8.self)
        #expect(json.contains("avoid_commentary") && !json.contains("avoidCommentary"))
        #expect(json.contains("night_mode"))
    }

    @Test func signOffIsWrittenOrRemovedFromTheLineup() throws {
        let r = try raw()
        var s = PictureSoundSettings(options: r.get(["options"]), lineup: r.get(["lineup"]))
        s.signOffFrom = "03:30"
        let on = s.apply(toLineup: r.get(["lineup"]))
        #expect(on.get(["signoff", "from"])?.stringValue == "03:30")
        #expect(on.get(["timezone"])?.stringValue == "UTC", "the rest of the lineup is kept")
        s.signOffEnabled = false
        let off = s.apply(toLineup: r.get(["lineup"]))
        #expect(off.get(["signoff"]) == .null)
    }

    @Test func languageNamesAreFriendlyAndUnknownCodesPassThrough() {
        #expect(Language.name("jpn") == "Japanese")
        #expect(Language.name("tlh") == "tlh")
        #expect(Language.name("") == "Unknown")
        #expect(Language.all.count >= 30)
    }
}
