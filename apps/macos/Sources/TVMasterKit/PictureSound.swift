import Foundation

/// The server's own key names matter when sending JSON back (`avoid_commentary`,
/// not `avoidCommentary`), and the client's decoder rewrites keys. So these
/// helpers work on the raw JSON exactly as the server wrote it.
extension JSONValue {
    /// Convert what `JSONSerialization` produced, keeping every key as written.
    public init(any: Any) {
        switch any {
        case let n as NSNumber:
            if CFGetTypeID(n) == CFBooleanGetTypeID() { self = .bool(n.boolValue) } else { self = .number(n.doubleValue) }
        case let s as String: self = .string(s)
        case let a as [Any]: self = .array(a.map { JSONValue(any: $0) })
        case let d as [String: Any]: self = .object(d.mapValues { JSONValue(any: $0) })
        default: self = .null
        }
    }

    /// The value at `path`, with keys matched exactly.
    public func get(_ path: [String]) -> JSONValue? {
        var cur: JSONValue = self
        for key in path {
            guard case .object(let o) = cur, let next = o[key] else { return nil }
            cur = next
        }
        return cur
    }

    /// Set the value at `path`, creating objects on the way and keeping everything else.
    public mutating func set(_ path: [String], _ value: JSONValue) {
        guard let first = path.first else { self = value; return }
        var o: [String: JSONValue] = [:]
        if case .object(let existing) = self { o = existing }
        var child = o[first] ?? .null
        child.set(Array(path.dropFirst()), value)
        o[first] = child
        self = .object(o)
    }

    public var stringArray: [String] { (arrayValue ?? []).compactMap(\.stringValue) }
    public var boolValue: Bool? { if case .bool(let b) = self { return b }; return nil }
}

/// A language the pickers offer.
public struct Language: Identifiable, Hashable, Sendable {
    public let code: String
    public let name: String
    public var id: String { code }

    public static let all: [Language] = [
        ("eng", "English"), ("jpn", "Japanese"), ("fra", "French"), ("spa", "Spanish"), ("deu", "German"), ("ita", "Italian"),
        ("por", "Portuguese"), ("kor", "Korean"), ("zho", "Chinese"), ("rus", "Russian"), ("ara", "Arabic"), ("hin", "Hindi"),
        ("nld", "Dutch"), ("swe", "Swedish"), ("nor", "Norwegian"), ("dan", "Danish"), ("fin", "Finnish"), ("pol", "Polish"),
        ("tur", "Turkish"), ("ell", "Greek"), ("heb", "Hebrew"), ("ces", "Czech"), ("hun", "Hungarian"), ("ron", "Romanian"),
        ("ukr", "Ukrainian"), ("tha", "Thai"), ("vie", "Vietnamese"), ("ind", "Indonesian"), ("fil", "Filipino"), ("cat", "Catalan"),
    ].map { Language(code: $0.0, name: $0.1) }

    public static func name(_ code: String) -> String {
        all.first { $0.code == code.lowercased() }?.name ?? (code.isEmpty ? "Unknown" : code)
    }
}

/// The picture and sound settings of one channel, as the editor shows them.
/// `apply` writes them into the channel's raw JSON and leaves everything the
/// editor does not show (per-show settings, ad configuration, blocks) untouched.
public struct PictureSoundSettings: Equatable, Sendable {
    public var audioLanguages: [String] = []
    public var avoidCommentary = true
    public var loudness = "off"
    public var nightMode = false
    public var dialogueBoost = false
    public var mono = false

    public var subtitleMode = "off"
    public var subtitleLanguages: [String] = []
    public var subtitleSize = "medium"
    public var subtitleStyle = "outline"

    public var aspect = "fit"
    public var quality = "720"
    public var look = "none"
    public var deinterlace = "auto"

    public var bugEnabled = false
    public var bugText = ""
    public var bugCorner = "top_right"
    public var titleEnabled = false
    public var titleSeconds = 8
    public var clockEnabled = false

    public var offAirScreen = "card"
    public var offAirMessage = ""
    public var offAirImage = ""
    public var offAirTone = false

    public var signOffEnabled = false
    public var signOffFrom = "02:00"
    public var signOffTo = "05:00"
    public var signOffStyle = "bars"

    public static let subtitleModes: [(value: String, label: String)] = [
        ("off", "Off"), ("foreign", "When the audio is foreign"), ("always", "Always"), ("forced", "Forced only"), ("sdh", "For the hard of hearing"),
    ]
    public static let loudnessModes: [(value: String, label: String)] = [("off", "Off"), ("light", "Light"), ("broadcast", "Broadcast (EBU R128)")]
    public static let looks: [(value: String, label: String)] = [
        ("none", "None"), ("crt", "CRT"), ("vhs", "VHS"), ("film", "Film"), ("black_white", "Black & white"), ("sepia", "Sepia"),
    ]
    public static let aspects: [(value: String, label: String)] = [("fit", "Fit (bars if needed)"), ("fill", "Fill (crop edges)"), ("stretch", "Stretch")]
    public static let qualities: [(value: String, label: String)] = [("480", "480p"), ("720", "720p"), ("1080", "1080p")]
    /// What can fill the screen when nothing is on air (and during sign-off).
    public static let screens: [(value: String, label: String)] = [
        ("card", "Station card"), ("bars", "Colour bars"), ("static", "Snow"), ("black", "Black"), ("image", "My picture"),
    ]
    public static let signOffStyles = screens

    public init() {}

    /// Read the settings out of a channel fetched as raw JSON (`options` and `lineup`).
    public init(options: JSONValue?, lineup: JSONValue?) {
        func str(_ p: [String], _ d: String) -> String { options?.get(p)?.stringValue ?? d }
        func bool(_ p: [String], _ d: Bool) -> Bool { options?.get(p)?.boolValue ?? d }
        audioLanguages = options?.get(["audio", "languages"])?.stringArray ?? []
        avoidCommentary = bool(["audio", "avoid_commentary"], true)
        loudness = str(["audio", "loudness"], "off")
        nightMode = bool(["audio", "night_mode"], false)
        dialogueBoost = bool(["audio", "dialogue_boost"], false)
        mono = bool(["audio", "mono"], false)
        offAirScreen = str(["off_air", "screen"], "card")
        offAirMessage = str(["off_air", "message"], "")
        offAirImage = options?.get(["off_air", "image"])?.stringValue ?? ""
        offAirTone = bool(["off_air", "tone"], false)
        subtitleMode = str(["subtitles", "mode"], "off")
        subtitleLanguages = options?.get(["subtitles", "languages"])?.stringArray ?? []
        subtitleSize = str(["subtitles", "size"], "medium")
        subtitleStyle = str(["subtitles", "style"], "outline")
        aspect = str(["video", "aspect"], "fit")
        quality = str(["video", "quality"], "720")
        look = str(["video", "look"], "none")
        deinterlace = str(["video", "deinterlace"], "auto")
        bugEnabled = bool(["overlay", "bug", "enabled"], false)
        bugText = str(["overlay", "bug", "text"], "")
        bugCorner = str(["overlay", "bug", "position"], "top_right")
        titleEnabled = bool(["overlay", "show_title", "enabled"], false)
        titleSeconds = options?.get(["overlay", "show_title", "seconds"])?.intValue ?? 8
        clockEnabled = bool(["overlay", "clock", "enabled"], false)
        if let so = lineup?.get(["signoff"]), so != .null {
            signOffEnabled = true
            signOffFrom = so.get(["from"])?.stringValue ?? "02:00"
            signOffTo = so.get(["to"])?.stringValue ?? "05:00"
            signOffStyle = so.get(["style"])?.stringValue ?? "bars"
        }
    }

    /// `options` with these settings written in, everything else kept.
    public func apply(to options: JSONValue?) -> JSONValue {
        var o = options ?? .object([:])
        let strings: ([String]) -> JSONValue = { .array($0.map { .string($0) }) }
        o.set(["audio", "languages"], strings(audioLanguages))
        o.set(["audio", "avoid_commentary"], .bool(avoidCommentary))
        o.set(["audio", "loudness"], .string(loudness))
        o.set(["audio", "night_mode"], .bool(nightMode))
        o.set(["audio", "dialogue_boost"], .bool(dialogueBoost))
        o.set(["audio", "mono"], .bool(mono))
        o.set(["off_air", "screen"], .string(offAirScreen))
        o.set(["off_air", "message"], .string(offAirMessage))
        o.set(["off_air", "image"], offAirImage.isEmpty ? .null : .string(offAirImage))
        o.set(["off_air", "tone"], .bool(offAirTone))
        o.set(["subtitles", "mode"], .string(subtitleMode))
        o.set(["subtitles", "languages"], strings(subtitleLanguages))
        o.set(["subtitles", "size"], .string(subtitleSize))
        o.set(["subtitles", "style"], .string(subtitleStyle))
        o.set(["video", "aspect"], .string(aspect))
        o.set(["video", "quality"], .string(quality))
        o.set(["video", "look"], .string(look))
        o.set(["video", "deinterlace"], .string(deinterlace))
        o.set(["overlay", "bug", "enabled"], .bool(bugEnabled))
        o.set(["overlay", "bug", "text"], .string(bugText))
        o.set(["overlay", "bug", "position"], .string(bugCorner))
        o.set(["overlay", "show_title", "enabled"], .bool(titleEnabled))
        o.set(["overlay", "show_title", "seconds"], .number(Double(titleSeconds)))
        o.set(["overlay", "clock", "enabled"], .bool(clockEnabled))
        return o
    }

    /// `lineup` with the sign-off hours set or removed.
    public func apply(toLineup lineup: JSONValue?) -> JSONValue {
        var l = lineup ?? .object([:])
        if signOffEnabled {
            l.set(["signoff"], .object(["from": .string(signOffFrom), "to": .string(signOffTo), "style": .string(signOffStyle)]))
        } else {
            l.set(["signoff"], .null)
        }
        return l
    }
}
