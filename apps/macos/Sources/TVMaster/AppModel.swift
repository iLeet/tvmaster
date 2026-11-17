import AppKit
import Foundation
import Observation
import TVMasterKit

enum SidebarItem: String, CaseIterable, Identifiable, Hashable {
    case home, watch, live, channels, schedule, libraries, shows, promos, tvs, appearance, share
    var id: String { rawValue }
    var title: String {
        switch self {
        case .home: "Home"
        case .watch: "Watch"
        case .live: "Live control"
        case .channels: "Channels"
        case .schedule: "Schedule"
        case .libraries: "Libraries"
        case .shows: "Shows"
        case .promos: "Promos"
        case .tvs: "TVs"
        case .appearance: "Appearance"
        case .share: "Share"
        }
    }
    var symbol: String {
        switch self {
        case .home: "house"
        case .watch: "play.tv"
        case .live: "dot.radiowaves.left.and.right"
        case .channels: "list.bullet.rectangle"
        case .schedule: "calendar"
        case .libraries: "folder"
        case .shows: "film.stack"
        case .promos: "sparkles.tv"
        case .tvs: "tv"
        case .appearance: "paintpalette"
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
    /// What has been put on channels in place of their programming.
    var liveState = LiveState()
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
                phase = .needsServer("Cannot reach \(url.host ?? "the server") (\(url.absoluteString)). Check that TVMaster is running there. If macOS asked to let TVMaster find devices on your local network, it has to be allowed in System Settings → Privacy & Security → Local Network.")
                return
            }
            guard let binary = DaemonLocator.find() else {
                phase = .needsServer("Couldn't find the TVMaster server to start. Install it, or point this app at one that is already running.")
                return
            }
            let port = url.port ?? 8787
            let host = shareOnNetwork ? "0.0.0.0" : "127.0.0.1"
            let tlsArguments = serveHTTPS ? ["--tls", "--tls-bind", "\(host):\(port + 1)"] : []
            await daemon.start(binary: binary, bind: "\(host):\(port)", dataDirectory: DaemonLocator.defaultDataDirectory, token: nil, extraArguments: tlsArguments)
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
            // Reachable from other devices means reachable by anyone on the network: the
            // server this app runs is put behind a token (saved in its database, so it is kept).
            if client.isLocal, shareOnNetwork, !auth.required, let made = try? await client.setSecurity(enabled: true), let t = made.token {
                token = t
                client = TVMasterClient.make(baseURL: url, token: t, pins: pins)
                self.client = client
            }
            if !token.isEmpty { tokens.set(token, for: url.absoluteString) }
            await refreshSecurity()
            await refreshMonitor()
            await refreshTimezone()
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

    /// TVMaster servers found on the local network.
    var discovered: [FoundServer] = []
    var scanning = false
    private var scanTask: Task<Void, Never>?

    /// Look for servers on this Mac's networks (a few seconds). Results appear as they are found.
    func findServers() {
        guard !scanning else { return }
        scanning = true
        discovered = []
        scanTask = Task {
            let all = await ServerDiscovery.scan { found in
                Task { @MainActor in if !self.discovered.contains(found) { self.discovered.append(found) } }
            }
            discovered = all
            scanning = false
        }
    }

    /// Use a server that was found: put its address in and connect.
    func use(_ server: FoundServer) async {
        serverText = server.url.absoluteString
        token = ""
        await connect()
    }

    /// The server's access-token setting, for Settings.
    var security: SecurityStatus?
    /// The station's time zone, for Settings and the "is this your time zone?" nudge.
    var timezone: TimezoneState?
    var timezoneNudgeDismissed = UserDefaults.standard.string(forKey: "tzDismissed") == TimeZone.current.identifier

    func refreshTimezone() async { timezone = try? await client?.timezone() }

    func setTimezone(_ zone: String?, applyToChannels: Bool) async {
        guard let client else { return }
        if let t = await run({ try await client.setTimezone(zone, applyToChannels: applyToChannels) }) {
            timezone = t
            await refreshChannels()
            system = try? await client.system()
        }
    }

    func dismissTimezoneNudge() {
        UserDefaults.standard.set(TimeZone.current.identifier, forKey: "tzDismissed")
        timezoneNudgeDismissed = true
    }

    /// What the server is keeping an eye on: its settings and any problems right now.
    var monitor: MonitorState?

    func refreshMonitor() async { monitor = try? await client?.monitor() }

    func saveMonitor(_ s: MonitorSettings) async {
        guard let client else { return }
        if let m = await run({ try await client.setMonitor(s) }) { monitor = m }
    }

    func refreshSecurity() async {
        security = try? await client?.security()
    }

    /// Turn the token on or off, replace it with a fresh or chosen one, and carry on with the new one.
    func changeSecurity(enabled: Bool, token chosen: String? = nil, rotate: Bool = false) async {
        guard let client, let url = serverURL else { return }
        do {
            let s = try await client.setSecurity(enabled: enabled, token: chosen, rotate: rotate)
            security = s
            token = s.token ?? ""
            tokens.set(s.token, for: url.absoluteString)
            self.client = TVMasterClient.make(baseURL: url, token: token, pins: pins)
            eventTask?.cancel()
            startEvents()
            system = try? await self.client?.system()
        } catch {
            lastError = error.localizedDescription
        }
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
        async let lv = try? client.live()
        if let v = await lv { liveState = v }
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
            // The view that asked has gone away or asked again: nothing went wrong.
            if error is CancellationError || (error as? URLError)?.code == .cancelled { return nil }
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
        case .liveChanged: Task { if let client, let v = try? await client.live() { liveState = v }; await refreshChannels() }
        case .monitorChanged: Task { await refreshMonitor(); await refreshAll() }
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
