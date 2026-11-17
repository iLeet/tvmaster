import Foundation

/// Where a time slot's programming comes from: one show, several shows, anything,
/// or something this editor leaves alone (a rotation), which is kept exactly as it was.
public struct SourceChoice: Equatable, Sendable {
    public enum Kind: String, CaseIterable, Identifiable, Sendable {
        case show, pool, chaos, rotation, other
        public var id: String { rawValue }
        public var label: String {
            switch self { case .show: "One show"; case .pool: "Several shows"; case .chaos: "Anything"; case .rotation: "A rotation"; case .other: "Other" }
        }
    }
    public var kind: Kind = .chaos
    public var show = ""
    public var shows: [String] = []
    public var order = "sequential"
    /// For a rotation: what takes turns, and when the next one takes over.
    public var items: [SourceChoice] = []
    /// `weeks` (every `weeks` weeks) or `finale` (when the current show finishes a full pass).
    public var advance = "weeks"
    public var weeks = 1
    var raw: JSONValue?

    public init() {}

    public init(json: JSONValue?) {
        guard let json, let type = json.get(["type"])?.stringValue else { return }
        switch type {
        case "show": kind = .show; show = json.get(["show"])?.stringValue ?? ""
        case "pool": kind = .pool; shows = json.get(["shows"])?.stringArray ?? []; order = json.get(["order"])?.stringValue ?? "sequential"
        case "chaos": kind = .chaos
        case "rotation":
            kind = .rotation
            items = (json.get(["items"])?.arrayValue ?? []).map { SourceChoice(json: $0) }
            if json.get(["advance"])?.stringValue == "finale" { advance = "finale" }
            else if let w = json.get(["advance", "weeks"])?.intValue { advance = "weeks"; weeks = max(1, w) }
        default: kind = .other; raw = json
        }
    }

    public var json: JSONValue {
        switch kind {
        case .show: .object(["type": .string("show"), "show": .string(show)])
        case .pool: .object(["type": .string("pool"), "shows": .array(shows.map { .string($0) }), "order": .string(order)])
        case .chaos: .object(["type": .string("chaos")])
        case .rotation:
            .object(["type": .string("rotation"), "items": .array(items.map(\.json)),
                     "advance": advance == "finale" ? .string("finale") : .object(["weeks": .number(Double(max(1, weeks)))])])
        case .other: raw ?? .object(["type": .string("chaos")])
        }
    }

    public var summary: String {
        switch kind {
        case .show: show.isEmpty ? "No show chosen" : show
        case .pool: shows.isEmpty ? "Every show" : shows.count == 1 ? shows[0] : "\(shows.count) shows"
        case .chaos: "Anything"
        case .rotation: "\(items.count) taking turns"
        case .other: "Something this editor does not know (kept as it is)"
        }
    }
}

/// One weekly time slot ("Weeknights 8-10 PM: Cheers").
public struct TimeSlot: Equatable, Identifiable, Sendable {
    public var id = UUID()
    public var name = ""
    /// 0 = Monday ... 6 = Sunday; empty = every day.
    public var days: Set<Int> = []
    public var start = "20:00"
    public var end = "21:00"
    public var source = SourceChoice()
    /// Air one episode at the start time, then leave the rest of the slot to the gap policy.
    public var once = false
    public init() {}
}

/// Part of the year with its own time slots ("Summer schedule", 06-01 to 08-31).
public struct SeasonSlot: Equatable, Identifiable, Sendable {
    public var id = UUID()
    public var name = ""
    /// "MM-DD", inclusive.
    public var from = "06-01"
    public var to = "08-31"
    public var slots: [TimeSlot] = []
    public init() {}
}

/// A one-off or yearly special ("Halloween marathon").
public struct EventSlot: Equatable, Identifiable, Sendable {
    public var id = UUID()
    public var name = ""
    /// Local "YYYY-MM-DDTHH:MM".
    public var start = ""
    public var end = ""
    public var yearly = false
    public var source = SourceChoice()
    public init() {}
}

/// Commercial breaks and bumpers.
public struct AdSettings: Equatable, Sendable {
    public var commercials = true
    public var libraryIds: [Int] = []
    public var bumperLibraryIds: [Int] = []
    public var minutesPerHour = 8
    public var podSeconds = 120
    public var maxBreaks = 4
    public var noRepeat = 12
    public var midBreaks = true
    public var brb = true, backTo = true, upNext = true, stationId = true
    public var laterTodayEvery = 3
    /// Only commercials from these years (nil = any).
    public var years: ClosedRange<Int>?
    public init() {}

    /// As the server's `ads` object, for explaining the settings.
    public var json: JSONValue {
        var o: JSONValue = .object([:])
        o.set(["commercials"], .bool(commercials))
        o.set(["minutes_per_hour"], .number(Double(minutesPerHour)))
        o.set(["pod_seconds"], .number(Double(podSeconds)))
        o.set(["max_breaks"], .number(Double(maxBreaks)))
        o.set(["no_repeat"], .number(Double(noRepeat)))
        o.set(["mid_breaks"], .bool(midBreaks))
        o.set(["bumpers", "brb"], .bool(brb))
        o.set(["bumpers", "back_to"], .bool(backTo))
        o.set(["bumpers", "up_next"], .bool(upNext))
        o.set(["bumpers", "station_id"], .bool(stationId))
        o.set(["bumpers", "later_today_every"], .number(Double(laterTodayEvery)))
        return o
    }
}

/// A ready-made amount of advertising, from the server.
public struct AdPreset: Decodable, Identifiable, Sendable, Hashable {
    public let id: String
    public let name: String
    public let about: String
    public let commercials: Bool
    public let podSeconds: Int
    public let maxBreaks: Int
    public let minutesPerHour: Int
    public let midBreaks: Bool
}

public struct AdCard: Decodable, Identifiable, Sendable, Hashable {
    public let id: String
    public let name: String
    public let about: String
}

public struct AdOptions: Decodable, Sendable {
    public let presets: [AdPreset]
    public let cards: [AdCard]
}

extension AdSettings {
    /// The preset these numbers are, or "custom".
    public func presetID(in presets: [AdPreset]) -> String {
        if !commercials { return "none" }
        return presets.first { $0.commercials && $0.podSeconds == podSeconds && $0.maxBreaks == maxBreaks && $0.minutesPerHour == minutesPerHour && $0.midBreaks == midBreaks }?.id ?? "custom"
    }

    public mutating func apply(_ p: AdPreset) {
        commercials = p.commercials; podSeconds = p.podSeconds; maxBreaks = p.maxBreaks; minutesPerHour = p.minutesPerHour; midBreaks = p.midBreaks
    }
}

/// Everything about a programming channel's schedule that the Programming tab edits.
/// Anything it does not know about (seasons, rotations, newer fields) is kept as it was.
public struct LineupSettings: Equatable, Sendable {
    public var gridMinutes: Int?
    public var catchUpAdvances = false
    public var slots: [TimeSlot] = []
    public var events: [EventSlot] = []
    public var seasons: [SeasonSlot] = []
    /// What airs between slots; nil = automatic (from the channel's strategy).
    public var fill: SourceChoice?
    public var ads: AdSettings?

    public static let grids = [5, 10, 15, 20, 30, 60]

    static func slots(_ blocks: JSONValue?) -> [TimeSlot] {
        (blocks?.arrayValue ?? []).map { b in
            var s = TimeSlot()
            s.name = b.get(["name"])?.stringValue ?? ""
            s.days = Set((b.get(["days"])?.arrayValue ?? []).compactMap(\.intValue))
            s.start = b.get(["start"])?.stringValue ?? "20:00"
            s.end = b.get(["end"])?.stringValue ?? "21:00"
            s.source = SourceChoice(json: b.get(["source"]))
            s.once = b.get(["once"])?.boolValue ?? false
            return s
        }
    }

    static func blocksJSON(_ slots: [TimeSlot]) -> JSONValue {
        .array(slots.map { s in
            var o: [String: JSONValue] = [
                "name": .string(s.name), "days": .array(s.days.sorted().map { .number(Double($0)) }),
                "start": .string(s.start), "end": .string(s.end), "source": s.source.json,
            ]
            if s.once { o["once"] = .bool(true) }
            return .object(o)
        })
    }

    public init() {}

    public init(lineup: JSONValue?) {
        guard let l = lineup else { return }
        gridMinutes = l.get(["grid_minutes"])?.intValue
        catchUpAdvances = l.get(["catch_up"])?.stringValue == "advance"
        slots = Self.slots(l.get(["blocks"]))
        seasons = (l.get(["seasons"])?.arrayValue ?? []).map { j in
            var v = SeasonSlot()
            v.name = j.get(["name"])?.stringValue ?? ""
            v.from = j.get(["from"])?.stringValue ?? "06-01"
            v.to = j.get(["to"])?.stringValue ?? "08-31"
            v.slots = Self.slots(j.get(["blocks"]))
            return v
        }
        events = (l.get(["events"])?.arrayValue ?? []).map { e in
            var v = EventSlot()
            v.name = e.get(["name"])?.stringValue ?? ""
            v.start = e.get(["start"])?.stringValue ?? ""
            v.end = e.get(["end"])?.stringValue ?? ""
            v.yearly = e.get(["yearly"])?.boolValue ?? false
            v.source = SourceChoice(json: e.get(["source"]))
            return v
        }
        if let f = l.get(["fill"]), f != .null { fill = SourceChoice(json: f) }
        if let a = l.get(["ads"]), a != .null {
            var v = AdSettings()
            v.commercials = a.get(["commercials"])?.boolValue ?? true
            v.libraryIds = (a.get(["library_ids"])?.arrayValue ?? []).compactMap(\.intValue)
            v.bumperLibraryIds = (a.get(["bumper_library_ids"])?.arrayValue ?? []).compactMap(\.intValue)
            v.minutesPerHour = a.get(["minutes_per_hour"])?.intValue ?? v.minutesPerHour
            v.podSeconds = a.get(["pod_seconds"])?.intValue ?? v.podSeconds
            v.maxBreaks = a.get(["max_breaks"])?.intValue ?? v.maxBreaks
            v.noRepeat = a.get(["no_repeat"])?.intValue ?? v.noRepeat
            v.midBreaks = a.get(["mid_breaks"])?.boolValue ?? true
            v.brb = a.get(["bumpers", "brb"])?.boolValue ?? true
            v.backTo = a.get(["bumpers", "back_to"])?.boolValue ?? true
            v.upNext = a.get(["bumpers", "up_next"])?.boolValue ?? true
            v.stationId = a.get(["bumpers", "station_id"])?.boolValue ?? true
            v.laterTodayEvery = a.get(["bumpers", "later_today_every"])?.intValue ?? 3
            if let y = a.get(["years"])?.arrayValue, y.count == 2, let lo = y[0].intValue, let hi = y[1].intValue, lo <= hi { v.years = lo...hi }
            ads = v
        }
    }

    /// The first thing wrong with these settings, in words, or nil.
    public var problem: String? {
        for s in slots {
            if s.start == s.end { return "A time slot starts and ends at the same time." }
            if s.source.kind == .show && s.source.show.isEmpty { return "Choose a show for every time slot." }
        }
        for season in seasons {
            if season.name.trimmingCharacters(in: .whitespaces).isEmpty { return "Give every season a name." }
            for sl in season.slots where sl.start == sl.end { return "A time slot in “\(season.name)” starts and ends at the same time." }
            for sl in season.slots where sl.source.kind == .show && sl.source.show.isEmpty { return "Choose a show for every time slot in “\(season.name)”." }
        }
        for e in events {
            if e.name.trimmingCharacters(in: .whitespaces).isEmpty { return "Give every event a name." }
            if e.start.isEmpty || e.end.isEmpty { return "Give “\(e.name)” a start and an end." }
        }
        let names = events.map { $0.name.lowercased() }
        if Set(names).count != names.count { return "Event names must be different from each other." }
        return nil
    }

    /// `lineup` with these settings written into it, everything else left alone.
    public func apply(to lineup: JSONValue?) -> JSONValue {
        var l = lineup ?? .object([:])
        l.set(["grid_minutes"], gridMinutes.map { .number(Double($0)) } ?? .null)
        l.set(["catch_up"], .string(catchUpAdvances ? "advance" : "resume"))
        l.set(["blocks"], Self.blocksJSON(slots))
        l.set(["seasons"], .array(seasons.map { v in
            .object(["name": .string(v.name), "from": .string(v.from), "to": .string(v.to), "blocks": Self.blocksJSON(v.slots)])
        }))
        l.set(["events"], .array(events.map { e in
            .object(["name": .string(e.name), "start": .string(e.start), "end": .string(e.end),
                     "yearly": .bool(e.yearly), "source": e.source.json])
        }))
        l.set(["fill"], fill?.json ?? .null)
        if let a = ads {
            l.set(["ads", "commercials"], .bool(a.commercials))
            l.set(["ads", "library_ids"], .array(a.libraryIds.map { .number(Double($0)) }))
            l.set(["ads", "bumper_library_ids"], .array(a.bumperLibraryIds.map { .number(Double($0)) }))
            l.set(["ads", "minutes_per_hour"], .number(Double(a.minutesPerHour)))
            l.set(["ads", "pod_seconds"], .number(Double(a.podSeconds)))
            l.set(["ads", "max_breaks"], .number(Double(a.maxBreaks)))
            l.set(["ads", "no_repeat"], .number(Double(a.noRepeat)))
            l.set(["ads", "mid_breaks"], .bool(a.midBreaks))
            l.set(["ads", "bumpers", "brb"], .bool(a.brb))
            l.set(["ads", "bumpers", "back_to"], .bool(a.backTo))
            l.set(["ads", "bumpers", "up_next"], .bool(a.upNext))
            l.set(["ads", "bumpers", "station_id"], .bool(a.stationId))
            l.set(["ads", "bumpers", "later_today_every"], .number(Double(a.laterTodayEvery)))
            l.set(["ads", "years"], a.years.map { .array([.number(Double($0.lowerBound)), .number(Double($0.upperBound))]) } ?? .null)
        } else {
            l.set(["ads"], .null)
        }
        return l
    }

    /// "Mon–Fri", "Every day", "Sat, Sun".
    public static func daysLabel(_ days: Set<Int>) -> String {
        let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        if days.isEmpty || days.count == 7 { return "Every day" }
        if days == [0, 1, 2, 3, 4] { return "Weekdays" }
        if days == [5, 6] { return "Weekends" }
        return days.sorted().map { names[$0] }.joined(separator: ", ")
    }
}
