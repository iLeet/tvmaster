import AppKit
import Foundation
import Observation
import TVMasterKit

enum SidebarItem: String, CaseIterable, Identifiable, Hashable {
    case home, watch, channels, schedule, libraries, tvs
    var id: String { rawValue }
    var title: String {
        switch self {
        case .home: "Home"
        case .watch: "Watch"
        case .channels: "Channels"
        case .schedule: "Schedule"
        case .libraries: "Libraries"
        case .tvs: "TVs"
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
        case ready
    }

    // Settings (persisted)
    var serverText: String { didSet { defaults.set(serverText, forKey: "server") } }
    var token: String = ""
    var startLocalServer: Bool { didSet { defaults.set(startLocalServer, forKey: "startLocal") } }
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

    private(set) var client: TVMasterClient?
    private var events: EventStream?
    private var eventTask: Task<Void, Never>?
    private let defaults = UserDefaults.standard
    private let tokens = TokenStore()
    let daemon = DaemonSupervisor()

    init() {
        serverText = defaults.string(forKey: "server") ?? "http://127.0.0.1:8787"
        startLocalServer = defaults.object(forKey: "startLocal") as? Bool ?? true
        macIsTV = defaults.bool(forKey: "macIsTV")
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
        let client = TVMasterClient(baseURL: url, token: token)

        if !(await TVMasterClient.isReachable(url)) {
            guard client.isLocal, startLocalServer else {
                phase = .needsServer("Cannot reach \(url.host ?? "the server"). Is TVMaster running?")
                return
            }
            guard let binary = DaemonLocator.find() else {
                phase = .needsServer("Couldn't find the TVMaster server to start. Install it, or point this app at one that is already running.")
                return
            }
            let port = url.port ?? 8787
            await daemon.start(binary: binary, bind: "127.0.0.1:\(port)", dataDirectory: DaemonLocator.defaultDataDirectory, token: token.isEmpty ? nil : token)
            var up = false
            for _ in 0..<40 {
                if await TVMasterClient.isReachable(url) { up = true; break }
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
