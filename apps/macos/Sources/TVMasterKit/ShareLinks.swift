import Foundation

/// Every address another app or device can use to reach this server, built from
/// what the server says about itself. Pure, so it is tested without a UI.
public struct ShareLinks: Sendable {
    public enum Scheme: String, CaseIterable, Sendable, Identifiable {
        case http, https
        public var id: String { rawValue }
        public var label: String { rawValue.uppercased() }
    }

    public struct Options: Sendable, Equatable {
        public var scheme: Scheme
        public var host: String
        public var includeToken: Bool
        public init(scheme: Scheme, host: String, includeToken: Bool) {
            self.scheme = scheme; self.host = host; self.includeToken = includeToken
        }
    }

    public struct Link: Identifiable, Sendable, Equatable {
        public var id: String
        public var title: String
        public var detail: String
        public var url: String
        /// A QR code is useful for this one (opened on a phone).
        public var qr: Bool
        /// The link contains the access token: treat it like a password.
        public var hasToken: Bool
    }

    public struct Section: Identifiable, Sendable, Equatable {
        public var id: String
        public var title: String
        public var note: String?
        public var links: [Link]
    }

    public let network: SystemInfo.Network?
    public let token: String?
    public let baseURL: URL

    public init(network: SystemInfo.Network?, token: String?, baseURL: URL) {
        self.network = network
        self.token = token?.isEmpty == true ? nil : token
        self.baseURL = baseURL
    }

    static func isLoopback(_ host: String) -> Bool {
        ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host.lowercased())
    }

    /// Hosts someone else could use, best first: the server's own name, its
    /// addresses, and the address this app used if that is not just "this machine".
    public var hosts: [String] {
        var out: [String] = []
        if let h = baseURL.host, !Self.isLoopback(h) { out.append(h) }
        for h in network?.hosts ?? [] where !out.contains(h) { out.append(h) }
        if out.isEmpty, let h = baseURL.host { out.append(h) }
        return out
    }

    public var schemes: [Scheme] {
        var s: [Scheme] = []
        if network?.httpPort != nil || network == nil { s.append(.http) }
        if network?.httpsPort != nil { s.append(.https) }
        return s.isEmpty ? [.http] : s
    }

    public func port(for scheme: Scheme) -> Int? {
        switch scheme {
        case .http: network?.httpPort ?? (baseURL.scheme == "http" ? baseURL.port : nil)
        case .https: network?.httpsPort ?? (baseURL.scheme == "https" ? baseURL.port : nil)
        }
    }

    public var defaultOptions: Options {
        // Prefer what the app itself is connected through, unless it is loopback-only.
        let scheme = Scheme(rawValue: baseURL.scheme?.lowercased() ?? "http").flatMap { schemes.contains($0) ? $0 : nil } ?? schemes[0]
        return Options(scheme: scheme, host: hosts.first ?? "localhost", includeToken: token != nil)
    }

    /// `http://host:port`, bracketing IPv6 literals.
    public func origin(_ o: Options) -> String {
        let h = o.host.contains(":") && !o.host.hasPrefix("[") ? "[\(o.host)]" : o.host
        let p = port(for: o.scheme).map { ":\($0)" } ?? ""
        return "\(o.scheme.rawValue)://\(h)\(p)"
    }

    private func tokenQuery(_ o: Options) -> String {
        guard o.includeToken, let token else { return "" }
        return "?token=\(token)"
    }

    /// `host:port`, the form Plex, Jellyfin and Emby ask for.
    public func tunerAddress(_ o: Options) -> String {
        let h = o.host.contains(":") && !o.host.hasPrefix("[") ? "[\(o.host)]" : o.host
        return "\(h):\(port(for: .http).map(String.init) ?? "8787")"
    }

    public func sections(_ o: Options, channels: [Channel], tvs: [Tv]) -> [Section] {
        let origin = origin(o)
        let t = tokenQuery(o)
        let hasToken = !t.isEmpty
        func link(_ id: String, _ title: String, _ detail: String, _ path: String, query: String? = nil, qr: Bool = false, token: Bool = true) -> Link {
            let q = query ?? (token ? t : "")
            return Link(id: id, title: title, detail: detail, url: origin + path + q, qr: qr, hasToken: token && hasToken && query == nil)
        }

        var out: [Section] = []
        out.append(Section(id: "iptv", title: "IPTV apps", note: "VLC, Kodi, TiviMate, IPTV Smarters, Jellyfin Live TV and most others take these two addresses.", links: [
            link("playlist", "Channel playlist (M3U)", "Every channel, in channel-number order", "/iptv/playlist.m3u"),
            link("guide", "Programme guide (XMLTV)", "What is on, 24 hours ahead", "/iptv/guide.xml"),
        ]))
        out.append(Section(id: "tuner", title: "Plex, Jellyfin and Emby tuner", note: "Add a network tuner (HDHomeRun) and enter this address, then use the XMLTV guide above. These servers use plain HTTP.", links: [
            Link(id: "tuner", title: "Tuner address", detail: "Enter this as the device address", url: tunerAddress(o), qr: false, hasToken: false),
            link("discover", "Tuner discovery", "For apps that ask for a full URL", "/discover.json"),
        ]))
        out.append(Section(id: "web", title: "Browser and phone", note: nil, links: [
            link("remote", "Phone remote", "Scan with a phone to change channels; Add to Home Screen keeps it like an app", "/admin/#/remote", token: false).withQR(),
            link("player", "Web player", "Watch in any browser", "/", token: false),
            link("admin", "Web admin", "Manage everything from a browser", "/admin/", token: false),
        ]))
        if !tvs.isEmpty || true {
            var tvLinks = [Link(id: "tv-new", title: "Make a screen a TV", detail: "Open on a smart TV or tablet; it registers itself", url: origin + "/?tv=living-room", qr: true, hasToken: false)]
            tvLinks += tvs.map { tv in
                Link(id: "tv-\(tv.id)", title: "\(tv.name) remote", detail: "A remote for just this TV", url: origin + "/?remote=\(tv.id)", qr: true, hasToken: false)
            }
            out.append(Section(id: "tvs", title: "TVs", note: "Change the name after tv= to register a different screen.", links: tvLinks))
        }
        let sorted = channels.sorted { $0.number < $1.number }
        out.append(Section(id: "channels", title: "Individual channels", note: "Open one channel directly in any player. HLS is best for phones and TVs; MPEG-TS is for tuner-style players.", links: sorted.flatMap { ch in
            [
                link("hls-\(ch.id)", "\(ch.number) · \(ch.name)", "HLS stream", "/stream/\(ch.id)/index.m3u8"),
                link("ts-\(ch.id)", "\(ch.number) · \(ch.name)", "MPEG-TS stream", "/stream/\(ch.id).ts"),
            ]
        }))
        return out
    }
}

private extension ShareLinks.Link {
    func withQR() -> ShareLinks.Link { var l = self; l.qr = true; return l }
}
