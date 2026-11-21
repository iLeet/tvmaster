import Foundation

/// Programming in plain words. The server stores a strategy, a run-out policy and a fill; people
/// think in three questions: how it plays, what happens when nothing is scheduled, and what happens
/// when a show runs out. These translate between the two (the web admin does the same).
public enum GapChoice: String, CaseIterable, Identifiable, Sendable {
    case screen, keep, random, these
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .screen: "Show the off-air screen"
        case .keep: "Keep playing shows"
        case .random: "Play something random"
        case .these: "Play these shows"
        }
    }
    public var help: String {
        switch self {
        case .screen: "The title card, colour bars, snow or your own picture, until the next scheduled show. Nothing else plays."
        case .keep: "Carry on with the channel's shows, as if it had no gaps."
        case .random: "Anything from the libraries, at random."
        case .these: "Pick the shows (or a rotation) that fill the gaps."
        }
    }
}

extension LineupSettings {
    /// Whether the channel has anything scheduled by time.
    public func hasSchedule(strategy: Strategy) -> Bool {
        strategy == .blockGrid || !slots.isEmpty || seasons.contains { !$0.slots.isEmpty } || !events.isEmpty
    }

    /// What airs in unscheduled time, as a person would put it.
    public func gapChoice(runOut: RunOut) -> GapChoice {
        guard let f = fill else { return runOut == .offAir && !slots.isEmpty ? .screen : .keep }
        switch f.kind {
        case .offAir: return .screen
        case .chaos: return .random
        default: return .these
        }
    }

    /// Choose what airs in unscheduled time. "Keep playing" is unset, unless the older "test screen"
    /// policy would still blank the gaps, which is then said outright.
    public mutating func setGap(_ c: GapChoice, strategy: Strategy, runOut: RunOut) {
        func pool() -> SourceChoice { var p = SourceChoice(); p.kind = .pool; p.order = strategy == .shuffle ? "shuffle" : "sequential"; return p }
        switch c {
        case .screen: var f = SourceChoice(); f.kind = .offAir; fill = f
        case .random: var f = SourceChoice(); f.kind = .chaos; fill = f
        case .these:
            if let f = fill, f.kind != .offAir, f.kind != .chaos { return }
            fill = pool()
        case .keep: fill = runOut == .offAir ? pool() : nil
        }
    }

    /// One line for the General tab.
    public func summary(strategy: Strategy, runOut: RunOut, breaks: String?) -> String {
        var parts = [strategy.label]
        if hasSchedule(strategy: strategy) {
            let n = slots.count + seasons.reduce(0) { $0 + $1.slots.count }
            parts.append("\(n) slot\(n == 1 ? "" : "s")")
            parts.append("nothing scheduled: \(gapChoice(runOut: runOut).label.lowercased())")
        }
        parts.append("when a show runs out: \(runOut.label.lowercased())")
        if let breaks { parts.append(breaks) }
        return parts.joined(separator: " · ")
    }
}

extension AdSettings {
    /// The amount of advertising in a few words.
    public func breaksLabel(in presets: [AdPreset]) -> String {
        if !commercials { return "station cards between shows, no commercials" }
        let id = presetID(in: presets)
        if let p = presets.first(where: { $0.id == id }) { return "commercial breaks: \(p.name.lowercased())" }
        return "commercial breaks: custom"
    }
}
