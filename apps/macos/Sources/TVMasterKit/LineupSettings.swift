import Foundation

/// Where a time slot's programming comes from: one show, several shows, anything,
/// or something this editor leaves alone (a rotation), which is kept exactly as it was.
public struct SourceChoice: Equatable, Sendable {
    public enum Kind: String, CaseIterable, Identifiable, Sendable {
        case show, pool, chaos, other
        public var id: String { rawValue }
        public var label: String {
            switch self { case .show: "One show"; case .pool: "Several shows"; case .chaos: "Anything"; case .other: "Rotation" }
        }
    }
    public var kind: Kind = .chaos
    public var show = ""
    public var shows: [String] = []
    public var order = "sequential"
    var raw: JSONValue?

    public init() {}

    public init(json: JSONValue?) {
        guard let json, let type = json.get(["type"])?.stringValue else { return }
        switch type {
        case "show": kind = .show; show = json.get(["show"])?.stringValue ?? ""
        case "pool": kind = .pool; shows = json.get(["shows"])?.stringArray ?? []; order = json.get(["order"])?.stringValue ?? "sequential"
        case "chaos": kind = .chaos
        default: kind = .other; raw = json
        }
    }

    public var json: JSONValue {
        switch kind {
        case .show: .object(["type": .string("show"), "show": .string(show)])
        case .pool: .object(["type": .string("pool"), "shows": .array(shows.map { .string($0) }), "order": .string(order)])
        case .chaos: .object(["type": .string("chaos")])
        case .other: raw ?? .object(["type": .string("chaos")])
        }
    }

    public var summary: String {
        switch kind {
        case .show: show.isEmpty ? "No show chosen" : show
        case .pool: shows.isEmpty ? "Every show" : shows.count == 1 ? shows[0] : "\(shows.count) shows"
        case .chaos: "Anything"
        case .other: "A rotation (edit it in the web admin)"
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
    public init() {}
}

/// Everything about a programming channel's schedule that the Programming tab edits.
/// Anything it does not know about (seasons, rotations, newer fields) is kept as it was.
public struct LineupSettings: Equatable, Sendable {
    public var gridMinutes: Int?
    public var catchUpAdvances = false
    public var slots: [TimeSlot] = []
    public var events: [EventSlot] = []
    /// What airs between slots; nil = automatic (from the channel's strategy).
    public var fill: SourceChoice?
    public var ads: AdSettings?
    public var seasonCount = 0

    public static let grids = [5, 10, 15, 20, 30, 60]

    public init() {}

    public init(lineup: JSONValue?) {
        guard let l = lineup else { return }
        gridMinutes = l.get(["grid_minutes"])?.intValue
        catchUpAdvances = l.get(["catch_up"])?.stringValue == "advance"
        seasonCount = l.get(["seasons"])?.arrayValue?.count ?? 0
        slots = (l.get(["blocks"])?.arrayValue ?? []).map { b in
            var s = TimeSlot()
            s.name = b.get(["name"])?.stringValue ?? ""
            s.days = Set((b.get(["days"])?.arrayValue ?? []).compactMap(\.intValue))
            s.start = b.get(["start"])?.stringValue ?? "20:00"
            s.end = b.get(["end"])?.stringValue ?? "21:00"
            s.source = SourceChoice(json: b.get(["source"]))
            s.once = b.get(["once"])?.boolValue ?? false
            return s
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
            ads = v
        }
    }

    /// The first thing wrong with these settings, in words, or nil.
    public var problem: String? {
        for s in slots {
            if s.start == s.end { return "A time slot starts and ends at the same time." }
            if s.source.kind == .show && s.source.show.isEmpty { return "Choose a show for every time slot." }
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
        l.set(["blocks"], .array(slots.map { s in
            var o: [String: JSONValue] = [
                "name": .string(s.name), "days": .array(s.days.sorted().map { .number(Double($0)) }),
                "start": .string(s.start), "end": .string(s.end), "source": s.source.json,
            ]
            if s.once { o["once"] = .bool(true) }
            return .object(o)
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
