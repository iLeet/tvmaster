import AppKit
import Foundation
import Observation
import TVMasterKit

enum SidebarItem: String, CaseIterable, Identifiable, Hashable {
    case home, watch, channels, schedule, libraries, tvs, share
    var id: String { rawValue }
    var title: String {
        switch self {
        case .home: "Home"
        case .watch: "Watch"
        case .channels: "Channels"
        case .schedule: "Schedule"
        case .libraries: "Libraries"
        case .tvs: "TVs"
        case .share: "Share"
        }
    }
    var symbol: String {
        switch self {
        case .home: "house"
        case .watch: "play.tv"
        case .channels: "list.bullet.rectangle"
        case .schedule: "calendar"
        case .libraries: "folder"
        case .tvs: "tv"
        case .share: "square.and.arrow.up"
        }
    }
}

@MainActor
@Observable
final class AppModel {
    enum Phase: Equatable {
        case connecting
        case needsServer(String?)   // optional message
        case needsToken(String?)
        /// An HTTPS server whose certificate this Mac has no reason to trust yet.
        case needsTrust(host: String, fingerprint: String)
        case ready
    }

    // Settings (persisted)
    var serverText: String { didSet { defaults.set(serverText, forKey: "server") } }
    var token: String = ""
    var startLocalServer: Bool { didSet { defaults.set(startLocalServer, forKey: "startLocal") } }
    /// Let other devices reach the server this app starts (it listens on this Mac only otherwise).
    var shareOnNetwork: Bool { didSet { defaults.set(shareOnNetwork, forKey: "shareOnNetwork") } }
    /// Also serve HTTPS from the server this app starts.
    var serveHTTPS: Bool { didSet { defaults.set(serveHTTPS, forKey: "serveHTTPS") } }
    var macIsTV: Bool { didSet { defaults.set(macIsTV, forKey: "macIsTV"); Task { await self.applyTVRegistration() } } }

    // State
    var phase: Phase = .connecting
    var selection: SidebarItem = .home
    var channels: [Channel] = []
    var tvs: [Tv] = []
    var libraries: [Library] = []
    var system: SystemInfo?
    var live = false
    var lastError: String?
    var daemonState: DaemonSupervisor.State = .stopped
    var watchingID: String?
    var showNewChannel = false
    var installingFFmpeg = false

    private(set) var client: TVMasterClient?
    /// Certificates the user has chosen to trust (self-signed HTTPS servers).
    let pins = PinStore()
    /// Relays AVPlayer's plain-HTTP requests to a pinned HTTPS server.
    private var proxy: PinnedProxy?
    private var events: EventStream?
    private var eventTask: Task<Void, Never>?
    private let defaults = UserDefaults.standard
    private let tokens = TokenStore()
    let daemon = DaemonSupervisor()

    init() {
        serverText = defaults.string(forKey: "server") ?? "http://127.0.0.1:8787"
        startLocalServer = defaults.object(forKey: "startLocal") as? Bool ?? true
        macIsTV = defaults.bool(forKey: "macIsTV")
        shareOnNetwork = defaults.bool(forKey: "shareOnNetwork")
        serveHTTPS = defaults.bool(forKey: "serveHTTPS")
        // Developer aid: start on a given screen (used for screenshots).
        if let start = ProcessInfo.processInfo.environment["TVM_SECTION"], let item = SidebarItem(rawValue: start) { selection = item }
        Task { await daemon.observe { [weak self] s in Task { @MainActor in self?.daemonState = s } } }
    }

    // MARK: connecting

    var serverURL: URL? {
        var text = serverText.trimmingCharacters(in: .whitespacesAndNewlines)
        if text.isEmpty { return nil }
        if !text.contains("://") { text = "http://" + text }
        while text.hasSuffix("/") { text.removeLast() }
        guard let url = URL(string: text), url.host != nil else { return nil }
        return url
    }

    var watching: Channel? { channels.first { $0.id == watchingID } }
    var thisMacTVID: String { "mac-" + Format.slug(Host.current().localizedName ?? "this-mac") }

    /// Connect to the server, starting a local one first when that is what the address points at.
    func connect() async {
        phase = .connecting
        lastError = nil
        guard let url = serverURL else { phase = .needsServer("Enter your server's address."); return }
        let saved = tokens.token(for: url.absoluteString) ?? ""
        if token.isEmpty { token = saved }
        var client = TVMasterClient.make(baseURL: url, token: token, pins: pins)

        var reach = await client.reachability()
        if case .untrusted(let host, let fingerprint) = reach {
            phase = .needsTrust(host: host, fingerprint: fingerprint)
            return
        }
        if reach != .up {
            guard client.isLocal, startLocalServer else {
                phase = .needsServer("Cannot reach \(url.host ?? "the server"). Is TVMaster running?")
                return
            }
            guard let binary = DaemonLocator.find() else {
                phase = .needsServer("Couldn't find the TVMaster server to start. Install it, or point this app at one that is already running.")
                return
            }
            let port = url.port ?? 8787
            // Reachable from other devices means reachable by anyone on the network: always behind a token.
            if shareOnNetwork && token.isEmpty {
                token = Format.randomToken()
                // The server is about to require it; the client must carry it too.
                client = TVMasterClient.make(baseURL: url, token: token, pins: pins)
            }
            let host = shareOnNetwork ? "0.0.0.0" : "127.0.0.1"
            let tlsArguments = serveHTTPS ? ["--tls", "--tls-bind", "\(host):\(port + 1)"] : []
            await daemon.start(binary: binary, bind: "\(host):\(port)", dataDirectory: DaemonLocator.defaultDataDirectory, token: token.isEmpty ? nil : token, extraArguments: tlsArguments)
            var up = false
            for _ in 0..<40 {
                reach = await client.reachability()
                if reach == .up { up = true; break }
                try? await Task.sleep(nanoseconds: 250_000_000)
            }
            guard up else { phase = .needsServer("The local server didn't start. Check its log in Settings."); return }
        }

        do {
            let auth = try await client.auth()
            if auth.required && !auth.authenticated {
                self.client = client
                phase = .needsToken(token.isEmpty ? nil : "That token isn't right.")
                return
            }
            self.client = client
            if !token.isEmpty { tokens.set(token, for: url.absoluteString) }
            await refreshAll()
            startEvents()
            await applyTVRegistration()
            phase = .ready
        } catch {
            phase = .needsServer(error.localizedDescription)
        }
    }

    /// The user compared the fingerprint and accepts this certificate for this server.
    func trustServer(host: String, fingerprint: String) async {
        guard let url = serverURL else { return }
        pins.set(fingerprint, host: host, port: url.port ?? 443)
        await connect()
    }

    func forgetCertificate(for key: String) {
        let parts = key.split(separator: ":")
        guard parts.count == 2, let port = Int(parts[1]) else { return }
        pins.set(nil, host: String(parts[0]), port: port)
        pinsChanged += 1
    }

    /// Bumped when a pin is forgotten, so views listing them refresh.
    var pinsChanged = 0

    /// Where AVPlayer should load a channel from. A server whose certificate is
    /// only trusted because the user pinned it is reached through the loopback
    /// relay, since AVPlayer cannot be told about pins.
    func playableURL(for channel: Channel) async -> URL? {
        guard let client else { return nil }
        let direct = client.streamURL(channel: channel.id)
        guard client.isHTTPS, let host = client.baseURL.host, client.trust?.usesPin(host: host, port: client.baseURL.port ?? 443) == true else {
            return direct
        }
        if proxy == nil { proxy = PinnedProxy(upstream: client.baseURL, session: client.urlSession) }
        guard (try? await proxy?.start()) != nil else { return nil }
        return proxy?.localURL(for: direct)
    }

    func submitToken(_ t: String) async {
        token = t.trimmingCharacters(in: .whitespacesAndNewlines)
        await connect()
    }

    func disconnect() async {
        eventTask?.cancel()
        client = nil
        live = false
        phase = .needsServer(nil)
    }

    func forgetToken() {
        if let url = serverURL { tokens.set(nil, for: url.absoluteString) }
        token = ""
    }

    func shutdown() async { await daemon.stop() }

    /// Apply a changed "share on network" or HTTPS setting to the server this app runs.
    func restartLocalServer() async {
        guard client?.isLocal == true || phase != .ready else { return }
        eventTask?.cancel()
        await daemon.stop()
        try? await Task.sleep(nanoseconds: 800_000_000)
        await connect()
    }

    /// Install FFmpeg with Homebrew, then restart the local server so it finds it.
    func installFFmpeg() async {
        guard let brew = FFmpegSetup.brew(), !installingFFmpeg else { return }
        installingFFmpeg = true
        defer { installingFFmpeg = false }
        do {
            try await FFmpegSetup.install(brew: brew)
        } catch {
            lastError = error.localizedDescription
            return
        }
        // A server this app started has to restart to see the new binary; one
        // running elsewhere is not ours to restart.
        if client?.isLocal == true {
            eventTask?.cancel()
            await daemon.stop()
            try? await Task.sleep(nanoseconds: 800_000_000)
            await connect()
        }
    }

    // MARK: data

    func refreshAll() async {
        guard let client else { return }
        async let c = try? client.channels()
        async let t = try? client.tvs()
        async let l = try? client.libraries()
        async let s = try? client.system()
        if let v = await c { channels = v.sorted { $0.number < $1.number } }
        if let v = await t { tvs = v }
        if let v = await l { libraries = v }
        if let v = await s { system = v }
        if watchingID == nil || watching == nil { watchingID = channels.first(where: { !$0.isGuide })?.id ?? channels.first?.id }
    }

    func refreshChannels() async {
        guard let client, let v = try? await client.channels() else { return }
        channels = v.sorted { $0.number < $1.number }
    }

    func refreshLibraries() async {
        guard let client, let v = try? await client.libraries() else { return }
        libraries = v
    }

    /// Run an action; show its failure in the banner instead of crashing the flow.
    @discardableResult
    func run<T>(_ action: () async throws -> T) async -> T? {
        do { return try await action() } catch {
            if let e = error as? TVMasterError, e == .unauthorized { phase = .needsToken("Your session expired.") }
            lastError = error.localizedDescription
            return nil
        }
    }

    // MARK: live events

    private func startEvents() {
        eventTask?.cancel()
        guard let client else { return }
        let stream = EventStream(client: client)
        events = stream
        stream.announce(tv: macIsTV ? thisMacTVID : nil)
        eventTask = Task { [weak self] in
            for await event in stream.events() {
                await self?.handle(event)
            }
        }
    }

    private func handle(_ event: ServerEvent) {
        switch event {
        case .connection(let up): live = up
        case .snapshot(let list): tvs = list
        case .tvState(let tv):
            if let i = tvs.firstIndex(where: { $0.id == tv.id }) { tvs[i] = tv } else { tvs.append(tv) }
            if macIsTV && tv.id == thisMacTVID { follow(tv) }
        case .tvRemoved(let id): tvs.removeAll { $0.id == id }
        case .channelNow(let id, let now, let next):
            if let i = channels.firstIndex(where: { $0.id == id }) { channels[i].now = now; channels[i].next = next }
        case .scheduleChanged: Task { await refreshChannels() }
        }
    }

    // MARK: this Mac as a TV

    /// The Mac follows its own TV entry: a remote can switch what the Watch screen plays.
    private func follow(_ tv: Tv) {
        guard tv.power else { watchingID = nil; return }
        if let ch = channels.first(where: { $0.number == tv.channel }) {
            watchingID = ch.id
            selection = .watch
        }
    }

    func applyTVRegistration() async {
        guard let client, phase == .ready || phase == .connecting else { return }
        events?.announce(tv: macIsTV ? thisMacTVID : nil)
        if macIsTV {
            _ = await run { try await client.registerTv(id: thisMacTVID, name: Host.current().localizedName ?? "This Mac") }
            startEvents() // reconnect so the server hears the hello
            tvs = (try? await client.tvs()) ?? tvs
        }
    }

    // MARK: watching

    func watch(_ channel: Channel) {
        selection = .watch
        if macIsTV, let client {
            Task { _ = await run { try await client.tune(tv: self.thisMacTVID, number: channel.number) } }
        }
        watchingID = channel.id
    }

    func zap(_ direction: Int) {
        let list = channels
        guard !list.isEmpty else { return }
        let i = list.firstIndex { $0.id == watchingID } ?? -1
        let next = list[(i + direction + list.count) % list.count]
        watch(next)
    }

    // MARK: helpers

    func openInBrowser(_ path: String) {
        guard let client else { return }
        NSWorkspace.shared.open(client.webURL(path))
    }
}
