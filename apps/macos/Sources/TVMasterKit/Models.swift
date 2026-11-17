import Foundation

// Shapes of the daemon's JSON API (see docs/API.md). The client decodes with
// `.convertFromSnakeCase`, so `now_ms` arrives as `nowMs`.

/// Arbitrary JSON, for the parts of the API this app passes through untouched
/// (a channel's lineup is edited in the web editor; the app only reads and keeps it).
public enum JSONValue: Codable, Sendable, Hashable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Double.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([JSONValue].self) { self = .array(v) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .string(let v): try c.encode(v)
        case .array(let v): try c.encode(v)
        case .object(let v): try c.encode(v)
        }
    }

    /// Looks a key up as written (`grid_minutes`), camel-cased (`gridMinutes`) or
    /// snake-cased, because the client's snake-case decoding strategy also
    /// rewrites dictionary keys.
    public subscript(key: String) -> JSONValue? {
        guard case .object(let o) = self else { return nil }
        if let v = o[key] { return v }
        let parts = key.split(separator: "_").map(String.init)
        let camel = (parts.first ?? "") + parts.dropFirst().map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined()
        if let v = o[camel] { return v }
        var snake = ""
        for ch in key {
            if ch.isUppercase { snake.append("_"); snake.append(contentsOf: ch.lowercased()) } else { snake.append(ch) }
        }
        return o[snake]
    }

    public var stringValue: String? { if case .string(let s) = self { return s }; return nil }
    public var intValue: Int? { if case .number(let n) = self { return Int(n) }; return nil }
    public var arrayValue: [JSONValue]? { if case .array(let a) = self { return a }; return nil }
}

public struct Program: Codable, Sendable, Hashable, Identifiable {
    public var mediaId: Int
    public var show: String?
    public var title: String
    public var season: Int?
    public var episode: Int?
    public var startMs: Int64
    public var endMs: Int64
    public var runtimeEndMs: Int64
    public var label: String?

    public var id: String { "\(mediaId)-\(startMs)" }
    public var start: Date { Date(timeIntervalSince1970: Double(startMs) / 1000) }
    public var end: Date { Date(timeIntervalSince1970: Double(endMs) / 1000) }
    /// "Cheers" for a show with episodes; the title alone for a film or short.
    public var headline: String { show ?? title }
    public var subtitle: String? { show == nil ? nil : title }
    public var episodeCode: String? {
        guard let season, let episode else { return nil }
        return String(format: "S%02dE%02d", season, episode)
    }
}

public enum ChannelKind: String, Codable, Sendable {
    case programming, guide
    /// A test-pattern channel: only airs its off-air screen.
    case test

    /// A kind this app does not know yet (a newer server) is treated as ordinary programming.
    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ChannelKind(rawValue: raw) ?? .programming
    }
}

public enum Strategy: String, Codable, Sendable, CaseIterable, Identifiable {
    case sequential, shuffle, chaos
    case blockGrid = "block_grid"
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .sequential: "In order"
        case .shuffle: "Shuffle"
        case .chaos: "Chaos"
        case .blockGrid: "Weekly lineup"
        }
    }
}

public enum RunOut: String, Codable, Sendable, CaseIterable, Identifiable {
    case keepGoing = "keep_going"
    case nextInRotation = "next_in_rotation"
    case chaosFill = "chaos_fill"
    case hybrid
    case offAir = "off_air"
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .keepGoing: "Keep going"
        case .nextInRotation: "Next in rotation"
        case .chaosFill: "Chaos fill"
        case .hybrid: "Hybrid"
        case .offAir: "Test screen"
        }
    }
}

public struct GuideConfig: Codable, Sendable, Hashable {
    public var hours: Int
    public var title: String?
    public var ticker: [String]
    public var musicLibraryIds: [Int]
}

/// A channel as the list endpoint returns it (with what is on air).
public struct Channel: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var number: Int
    public var name: String
    public var strategy: Strategy
    public var runOut: RunOut
    public var autopilot: Bool
    public var chaos: Int
    public var alwaysOn: Bool
    public var libraryIds: [Int]
    public var lineup: JSONValue
    public var kind: ChannelKind
    public var group: String?
    public var guide: GuideConfig
    /// Audio, subtitles, picture and on-screen graphics (the server's `options`; absent on old servers).
    public var options: JSONValue?
    public var onAir: Bool?
    public var now: Program?
    public var next: Program?

    public var isGuide: Bool { kind == .guide }
    public var isTest: Bool { kind == .test }
    /// Has a schedule worth showing (neither the live guide nor a test pattern).
    public var isScheduled: Bool { kind == .programming }
    public var timeZone: TimeZone? {
        lineup["timezone"]?.stringValue.flatMap { $0.isEmpty ? nil : TimeZone(identifier: $0) }
    }
    /// "2 blocks · 1 season · ads on", for list rows.
    public var lineupSummary: String {
        var parts: [String] = []
        func count(_ key: String, _ word: String) {
            let n = lineup[key]?.arrayValue?.count ?? 0
            if n > 0 { parts.append("\(n) \(word)\(n == 1 ? "" : "s")") }
        }
        count("blocks", "block"); count("seasons", "season"); count("events", "event")
        if let g = lineup["grid_minutes"]?.intValue { parts.append("\(g) min grid") }
        if let ads = lineup["ads"], ads != .null { parts.append("ads on") }
        return parts.joined(separator: " · ")
    }
}

public struct Tv: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var channel: Int?
    public var power: Bool
    public var volume: Int
    public var muted: Bool
    public var online: Bool
}

public enum LibraryKind: String, Codable, Sendable, CaseIterable, Identifiable {
    case shows, commercials, bumpers, music
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .shows: "TV shows"
        case .commercials: "Commercials"
        case .bumpers: "Bumper clips"
        case .music: "Music"
        }
    }
}

public struct Library: Codable, Sendable, Hashable, Identifiable {
    public struct Analysis: Codable, Sendable, Hashable {
        public var total: Int
        public var done: Int
        public var running: Bool
    }
    public var id: Int
    public var path: String
    public var kind: LibraryKind
    public var enabled: Bool
    public var items: Int
    public var analysis: Analysis?
    public var name: String { (path as NSString).lastPathComponent }
}

public struct ScanResult: Codable, Sendable, Hashable {
    public var found: Int
    public var indexed: Int
    public var skipped: Int
    public var analysing: Int?
}

public struct ShowSummary: Codable, Sendable, Hashable, Identifiable {
    public var name: String
    public var episodes: Int
    public var totalMs: Int64
    /// Audio languages found in the show's files (three-letter codes); absent on older servers.
    public var audioLanguages: [String]?
    public var subtitleLanguages: [String]?
    /// Settings that follow the show onto every channel.
    public var defaults: ShowDefaults?
    /// The folders (detected names) the show is made of; absent on older servers.
    public var folders: [ShowFolder]?
    /// Description, rating, tags and poster.
    public var info: ShowInfo?
    public var id: String { name }
}

/// One folder that is part of a show.
public struct ShowFolder: Codable, Sendable, Hashable, Identifiable {
    public var raw: String
    public var episodes: Int
    /// The season this folder stands for, if known.
    public var season: Int?
    /// A rule of the user's own covers it (rather than the automatic clean-up).
    public var custom: Bool
    public var id: String { raw }
}

/// What a show is about, for guides and apps.
public struct ShowInfo: Codable, Sendable, Hashable {
    public var description: String
    public var rating: String
    public var tags: [String]
    public var poster: String?
    public init(description: String = "", rating: String = "", tags: [String] = [], poster: String? = nil) {
        self.description = description; self.rating = rating; self.tags = tags; self.poster = poster
    }
    var json: JSONValue {
        .object(["description": .string(description), "rating": .string(rating),
                 "tags": .array(tags.map { .string($0) }), "poster": (poster ?? "").isEmpty ? .null : .string(poster!)])
    }
}

/// A show's own audio, subtitle and picture settings (blank = whatever the channel says).
public struct ShowDefaults: Codable, Sendable, Hashable {
    public var audioLanguages: [String]?
    public var subtitles: String?
    public var subtitleLanguages: [String]?
    public var look: String?
    public var aspect: String?
    public init(audioLanguages: [String]? = nil, subtitles: String? = nil, subtitleLanguages: [String]? = nil, look: String? = nil, aspect: String? = nil) {
        self.audioLanguages = audioLanguages; self.subtitles = subtitles; self.subtitleLanguages = subtitleLanguages; self.look = look; self.aspect = aspect
    }
    public var isEmpty: Bool { (audioLanguages ?? []).isEmpty && subtitles == nil && (subtitleLanguages ?? []).isEmpty && look == nil && aspect == nil }

    var json: JSONValue {
        func list(_ v: [String]?) -> JSONValue { (v ?? []).isEmpty ? .null : .array(v!.map { .string($0) }) }
        return .object(["audio_languages": list(audioLanguages), "subtitles": subtitles.map { .string($0) } ?? .null,
                        "subtitle_languages": list(subtitleLanguages), "look": look.map { .string($0) } ?? .null,
                        "aspect": aspect.map { .string($0) } ?? .null])
    }
}

public struct SystemInfo: Codable, Sendable, Hashable {
    public struct FFmpeg: Codable, Sendable, Hashable {
        public var version: String
        public var encoder: String?
        public var hasLibx264: Bool
        public var hasVideotoolbox: Bool
        public var hasNvenc: Bool
        public var hasDrawtext: Bool
        /// libass is built in: text subtitles can be drawn. Absent on older servers.
        public var hasSubtitles: Bool?
    }
    public var version: String
    public var os: String
    public var arch: String
    public var timezone: String
    /// The server's data folder (database, caches, uploads). Absent on older servers.
    public var dataDir: String?
    public var ffmpeg: FFmpeg?
    public var onAir: [String]
    public var authRequired: Bool
    public var network: Network?

    /// Where the server can be reached, so links can be built for other devices.
    public struct Network: Codable, Sendable, Hashable {
        public struct TLS: Codable, Sendable, Hashable {
            public var enabled: Bool
            public var selfSigned: Bool
            public var fingerprint: String?
        }
        public var hostname: String
        /// Names first (they survive an address change), then IPv4 addresses.
        public var hosts: [String]
        public var httpPort: Int?
        public var httpsPort: Int?
        public var tls: TLS
    }
}

/// Something that needs attention (a library offline, a channel that keeps failing).
public struct AlertItem: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var severity: String
    public var title: String
    public var detail: String
    public var sinceMs: Int64
    public var isError: Bool { severity == "error" }
}

public struct MonitorSettings: Codable, Sendable, Hashable {
    public var watchMinutes: Int
    public var alerts: Bool
    public var webhook: String?
    public var webhookText: Bool
    public var notifyRecovery: Bool
}

/// What `/api/v1/monitor` says: the settings and the problems right now.
public struct MonitorState: Codable, Sendable, Hashable {
    public var settings: MonitorSettings
    public var alerts: [AlertItem]
    /// `curl` is available on the server to send the webhook.
    public var curl: Bool
}

/// A picture uploaded to the server (an off-air screen, a channel logo).
public struct Asset: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    /// Where the file is on the server: what a channel's picture option stores.
    public var path: String
    public var bytes: Int
}

/// The server's access-token setting (`/api/v1/security`).
/// The station's time zone and where it came from (`/api/v1/timezone`).
public struct TimezoneState: Codable, Sendable, Hashable {
    public struct Elsewhere: Codable, Sendable, Hashable { public var id: String; public var name: String; public var zone: String }
    public var zone: String
    /// `setting`, `environment` or `machine`.
    public var source: String
    public var setting: String?
    public var channelsElsewhere: [Elsewhere]
}

public struct SecurityStatus: Codable, Sendable, Hashable {
    /// Anyone may watch without the token; managing still needs it. Absent on older servers.
    public var openViewing: Bool?
    public var enabled: Bool
    /// Set with `--token` on the server's command line, so it cannot be changed from here.
    public var locked: Bool
    public var token: String?
}

public struct AuthStatus: Codable, Sendable, Hashable {
    public var required: Bool
    public var authenticated: Bool
}

public struct ScheduleItem: Codable, Sendable, Hashable, Identifiable {
    public var kind: String
    public var bumper: String?
    public var mediaId: Int?
    public var title: String?
    public var startMs: Int64
    public var endMs: Int64
    public var durationMs: Int64
    public var inMs: Int64
    public var label: String?
    public var id: String { "\(startMs)-\(kind)-\(mediaId ?? 0)" }
    public var start: Date { Date(timeIntervalSince1970: Double(startMs) / 1000) }
}

/// What the web player and remote both listen to.
/// One episode of a show a channel can air.
public struct Episode: Codable, Sendable, Hashable, Identifiable {
    public var id: Int
    public var season: Int?
    public var episode: Int?
    public var title: String?
    public var durationMs: Int64
    public var label: String {
        let code = (season != nil && episode != nil) ? String(format: "S%02dE%02d ", season!, episode!) : ""
        return code + (title ?? "")
    }
}

/// What to put on the air.
public enum AirTarget: Sendable, Equatable {
    case show(String)
    case episode(Int)
    case random
    case offAir

    var body: [String: JSONValue] {
        switch self {
        case .show(let n): ["show": .string(n)]
        case .episode(let id): ["media_id": .number(Double(id))]
        case .random: ["random": .bool(true)]
        case .offAir: ["off_air": .bool(true)]
        }
    }
}

/// A channel the operator has taken over (test screen, blank, emergency alert...).
public struct LiveOverride: Codable, Sendable, Hashable, Identifiable {
    public var channel: String
    public var mode: String
    public var title: String
    public var message: String
    public var startedMs: Int64
    public var untilMs: Int64?
    public var style: String?
    public var theme: String?
    public var severity: String?
    public var sound: String?
    public var id: String { channel }

    public var modeLabel: String {
        switch mode {
        case "alert": "Emergency alert"
        case "test_screen": "Test screen"
        case "off_air": "Off air"
        case "black": "Black"
        case "static": "Snow"
        default: mode
        }
    }
}

/// A ready-made alert to try or start from.
public struct LiveSample: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var name: String
    public var event: String
    public var text: String
    public var severity: String?
    public var sound: String?
}

public struct LiveChoice: Codable, Sendable, Hashable, Identifiable {
    public var value: String
    public var label: String
    public var id: String { value }
    public init(value: String, label: String) { self.value = value; self.label = label }
}

/// What an alert can look and sound like, as the server offers it.
public struct LiveOptions: Codable, Sendable, Hashable {
    public var styles: [LiveChoice]
    public var themes: [LiveChoice]
    public var severities: [LiveChoice]
    public var sounds: [LiveChoice]
}

public struct LiveState: Codable, Sendable, Hashable {
    public var overrides: [LiveOverride]
    public var samples: [LiveSample]
    public var options: LiveOptions?
    public init(overrides: [LiveOverride] = [], samples: [LiveSample] = [], options: LiveOptions? = nil) { self.overrides = overrides; self.samples = samples; self.options = options }
    public func override(for channel: String) -> LiveOverride? { overrides.first { $0.channel == channel } }
}

/// The ways to take a channel over, in the order they are offered.
public enum LiveMode: String, CaseIterable, Identifiable, Sendable {
    case testScreen = "test_screen", offAir = "off_air", black, snow = "static"
    public var id: String { rawValue }
    public var label: String {
        switch self { case .testScreen: "Test screen"; case .offAir: "Off air"; case .black: "Black"; case .snow: "Snow" }
    }
    public var help: String {
        switch self {
        case .testScreen: "Colour bars and a steady tone"
        case .offAir: "The channel's own off-air screen"
        case .black: "A black screen, no sound"
        case .snow: "Static and hiss"
        }
    }
}

public enum ServerEvent: Sendable, Equatable {
    case snapshot(tvs: [Tv])
    case tvState(Tv)
    case tvRemoved(String)
    case channelNow(channel: String, now: Program?, next: Program?)
    case scheduleChanged(channel: String)
    /// A problem was raised or went away, or a watched folder changed.
    case monitorChanged
    /// A channel was taken over or given back.
    case liveChanged
    /// The live connection came up or went down.
    case connection(Bool)
}
