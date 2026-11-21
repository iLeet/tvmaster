import Foundation

/// The new-channel guide: plain answers in, the channel the server expects out. The same steps as the web admin.
public struct ChannelGuideAnswers: Equatable, Sendable {
    public enum Style: String, CaseIterable, Sendable, Identifiable {
        case shuffle, inOrder = "in_order", weekly
        public var id: String { rawValue }
        public var title: String {
            switch self { case .shuffle: "Mix it up"; case .inOrder: "In order, like a series"; case .weekly: "A real TV schedule" }
        }
        public var about: String {
            switch self {
            case .shuffle: "Episodes in a random order, shows mixed together, like channel surfing. Every episode comes round before any repeats."
            case .inOrder: "Each show goes episode by episode, remembering where it left off. Shows take turns."
            case .weekly: "Shows at set times on set days: cartoons at 8, a movie at 9. You lay out the week next."
            }
        }
    }

    public var name = ""
    public var id = ""
    public var number = 2
    public var group = ""
    /// Shows to play; empty = everything in the libraries.
    public var shows: [String] = []
    public var style: Style = .shuffle
    /// `clean` = no breaks, else the id of a break preset.
    public var breaks = "clean"
    /// This channel's own look; nil = follow the station.
    public var look: ThemeChoice?
    public var alwaysOn = false
    /// Surprises, 0-100.
    public var chaos = 0

    public init() {}

    /// What the guide needs to know about the station.
    public struct World: Sendable {
        public var timezone: String
        public var showLibraryIds: [Int]
        public var commercialLibraryIds: [Int]
        public var presets: [AdPreset]
        public init(timezone: String, showLibraryIds: [Int], commercialLibraryIds: [Int], presets: [AdPreset]) {
            self.timezone = timezone; self.showLibraryIds = showLibraryIds; self.commercialLibraryIds = commercialLibraryIds; self.presets = presets
        }
    }

    /// The breaks the answers ask for (nil = none).
    public func ads(_ world: World) -> AdSettings? {
        guard breaks != "clean", let p = world.presets.first(where: { $0.id == breaks }) else { return nil }
        var a = AdSettings()
        a.apply(p)
        a.libraryIds = world.commercialLibraryIds
        return a
    }

    /// The fields for `POST /channels`.
    public func body(_ world: World) -> [String: JSONValue] {
        var ls = LineupSettings()
        ls.ads = ads(world)
        if style == .weekly { ls.gridMinutes = 30 }
        if style == .weekly {
            // Time nobody scheduled shows the title card, not more programmes.
            var f = SourceChoice(); f.kind = .offAir; ls.fill = f
        } else if !shows.isEmpty {
            var f = SourceChoice()
            f.kind = .pool
            f.shows = shows
            f.order = style == .shuffle ? "shuffle" : "sequential"
            ls.fill = f
        }
        var f: [String: JSONValue] = [
            "id": .string(id), "number": .number(Double(number)), "name": .string(name.trimmingCharacters(in: .whitespaces)),
            "kind": .string("programming"),
            "strategy": .string(style == .shuffle ? "shuffle" : style == .inOrder ? "sequential" : "block_grid"),
            "run_out": .string("keep_going"),
            "chaos": .number(style == .weekly ? 0 : Double(chaos)),
            "autopilot": .bool(true), "always_on": .bool(alwaysOn),
            "library_ids": .array(world.showLibraryIds.map { .number(Double($0)) }),
            "lineup": ls.apply(to: .object(["timezone": .string(world.timezone)])),
        ]
        let g = group.trimmingCharacters(in: .whitespaces)
        if !g.isEmpty { f["group"] = .string(g) }
        if let look {
            var ps = PictureSoundSettings(options: .object([:]), lineup: nil)
            ps.theme = look
            f["options"] = ps.apply(to: .object([:]))
        }
        return f
    }

    /// What the answers add up to, in a few plain lines.
    public func summary(_ world: World, totalShows: Int) -> [String] {
        var out = ["Channel \(number), \(name.trimmingCharacters(in: .whitespaces).isEmpty ? "unnamed" : name.trimmingCharacters(in: .whitespaces))."]
        out.append(shows.isEmpty ? "Plays everything in your libraries (\(totalShows) show\(totalShows == 1 ? "" : "s"))."
                   : "Plays \(shows.count) show\(shows.count == 1 ? "" : "s"): \(shows.prefix(4).joined(separator: ", "))\(shows.count > 4 ? " and \(shows.count - 4) more" : "").")
        out.append(style.title + (style == .weekly ? ": you lay out the week next, and time you leave empty shows the off-air screen." : "."))
        if let p = world.presets.first(where: { $0.id == breaks }), breaks != "clean" {
            out.append("Breaks: \(p.id == "none" ? "station cards only" : p.name).")
        } else { out.append("No commercial breaks.") }
        out.append(look.map { "Its own look (\($0.theme)\($0.gentle || $0.theme == "kids" ? ", gentle alerts" : ""))." } ?? "Follows the station look.")
        if alwaysOn { out.append("Always on: it keeps running even when nobody is watching.") }
        return out
    }

    /// The first thing wrong on the first step.
    public func problem(step: Int, takenNumbers: [Int]) -> String? {
        guard step == 0 else { return nil }
        if name.trimmingCharacters(in: .whitespaces).isEmpty { return "Give the channel a name." }
        if number < 1 { return "Pick a channel number." }
        if takenNumbers.contains(number) { return "Channel \(number) is already used." }
        if !Format.isValidID(id) { return "The id needs lowercase letters, digits, - and _." }
        return nil
    }
}
