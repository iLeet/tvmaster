import Foundation

/// One part of the broadcast day: what may air in it, whether there are breaks, and the look of the
/// station's graphics (the kids look and gentle alerts in the morning, say).
public struct DaypartSetting: Equatable, Sendable {
    public var name = "New day part"
    /// 0 = Monday ... 6 = Sunday; empty = every day.
    public var days: [Int] = []
    public var start = "06:00"
    public var end = "09:00"
    /// Channel ids; empty = every channel.
    public var channels: [String] = []
    /// `all_ages`, `kids`, `pg`, `teen`, `adult`; empty = no limit.
    public var maxRating = ""
    public var allowUnrated = false
    /// No commercial breaks in these hours.
    public var breaksOff = false
    public var look: ThemeChoice?

    public init() {}

    public init(json j: JSONValue) {
        name = j.get(["name"])?.stringValue ?? "Day part"
        days = (j.get(["days"])?.arrayValue ?? []).compactMap(\.intValue)
        start = j.get(["start"])?.stringValue ?? "06:00"
        end = j.get(["end"])?.stringValue ?? "09:00"
        channels = j.get(["channels"])?.stringArray ?? []
        maxRating = j.get(["max_rating"])?.stringValue ?? ""
        allowUnrated = j.get(["allow_unrated"])?.boolValue ?? false
        breaksOff = j.get(["breaks"])?.stringValue == "off"
        look = ThemeChoice(json: j.get(["look"]))
    }

    public var json: JSONValue {
        .object([
            "name": .string(name),
            "days": .array(days.sorted().map { .number(Double($0)) }),
            "start": .string(start), "end": .string(end),
            "channels": .array(channels.map { .string($0) }),
            "max_rating": maxRating.isEmpty ? .null : .string(maxRating),
            "allow_unrated": .bool(allowUnrated),
            "breaks": .string(breaksOff ? "off" : "normal"),
            "look": look?.json ?? .null,
        ])
    }

    /// "6:00 AM"
    public static func twelve(_ t: String) -> String {
        let p = t.split(separator: ":").compactMap { Int($0) }
        guard p.count == 2 else { return t }
        let h = p[0]
        return "\(h % 12 == 0 ? 12 : h % 12):\(String(format: "%02d", p[1])) \(h < 12 ? "AM" : "PM")"
    }

    public static let dayNames = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

    /// "6:00 AM – 9:00 AM, weekdays"
    public var summary: String {
        let d = Set(days)
        let when: String
        if d.isEmpty || d.count == 7 { when = "every day" }
        else if d == [0, 1, 2, 3, 4] { when = "weekdays" }
        else if d == [5, 6] { when = "weekends" }
        else { when = days.sorted().map { Self.dayNames[min($0, 6)] }.joined(separator: ", ") }
        return "\(Self.twelve(start)) – \(Self.twelve(end)), \(when)"
    }
}

/// What the server says about the day parts.
public struct DaypartsState: Sendable {
    public struct Check: Sendable, Equatable { public var shows = 0, airing = 0, heldBack = 0, unrated = 0 }
    public struct Named: Sendable, Equatable, Identifiable { public var id: String; public var name: String; public var about: String }
    public struct Preset: Sendable, Equatable { public var about: String; public var part: DaypartSetting }
    public var parts: [DaypartSetting] = []
    public var check: [Check] = []
    public var channels: [Named] = []
    public var ratings: [Named] = []
    public var presets: [Preset] = []
    public var timezone = ""

    public init() {}

    public init(json j: JSONValue) {
        parts = (j.get(["parts"])?.arrayValue ?? []).map { DaypartSetting(json: $0) }
        check = (j.get(["check"])?.arrayValue ?? []).map {
            Check(shows: $0.get(["shows"])?.intValue ?? 0, airing: $0.get(["airing"])?.intValue ?? 0,
                  heldBack: $0.get(["held_back"])?.intValue ?? 0, unrated: $0.get(["unrated"])?.intValue ?? 0)
        }
        channels = (j.get(["channels"])?.arrayValue ?? []).map {
            Named(id: $0.get(["id"])?.stringValue ?? "", name: "\($0.get(["number"])?.intValue ?? 0) \($0.get(["name"])?.stringValue ?? "")", about: "")
        }
        ratings = (j.get(["ratings"])?.arrayValue ?? []).map {
            Named(id: $0.get(["id"])?.stringValue ?? "", name: $0.get(["name"])?.stringValue ?? "", about: $0.get(["about"])?.stringValue ?? "")
        }
        presets = (j.get(["presets"])?.arrayValue ?? []).compactMap {
            guard let p = $0.get(["part"]) else { return nil }
            return Preset(about: $0.get(["about"])?.stringValue ?? "", part: DaypartSetting(json: p))
        }
        timezone = j.get(["timezone"])?.stringValue ?? ""
    }
}

/// The kinds of channel the creator offers, with a suggested id, number and name for each.
public struct ChannelTemplate: Sendable, Equatable, Identifiable {
    public var id: String
    public var group: String
    public var kind: String
    public var name: String
    public var about: String
    public var needs: [String]
    public var suggestedId: String
    public var suggestedNumber: Int
    public var suggestedName: String
    /// What the channel starts with (the `info` section of an info channel, say).
    public var body: JSONValue
}

public struct ChannelTemplates: Sendable {
    public var templates: [ChannelTemplate] = []
    public var feeds: [(name: String, url: String)] = []

    public init() {}

    public init(json j: JSONValue) {
        templates = (j.get(["templates"])?.arrayValue ?? []).map { t in
            ChannelTemplate(
                id: t.get(["id"])?.stringValue ?? "", group: t.get(["group"])?.stringValue ?? "", kind: t.get(["kind"])?.stringValue ?? "programming",
                name: t.get(["name"])?.stringValue ?? "", about: t.get(["about"])?.stringValue ?? "", needs: t.get(["needs"])?.stringArray ?? [],
                suggestedId: t.get(["suggested", "id"])?.stringValue ?? "", suggestedNumber: t.get(["suggested", "number"])?.intValue ?? 1,
                suggestedName: t.get(["suggested", "name"])?.stringValue ?? "", body: t.get(["body"]) ?? .object([:])
            )
        }
        feeds = (j.get(["feeds"])?.arrayValue ?? []).map { ($0.get(["name"])?.stringValue ?? "", $0.get(["url"])?.stringValue ?? "") }
    }

    public var groups: [String] {
        var seen: [String] = []
        for t in templates where !seen.contains(t.group) { seen.append(t.group) }
        return seen
    }
}
