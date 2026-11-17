import Foundation

/// Geometry for the weekly grid editor: time slots <-> rectangles on a
/// Monday-to-Sunday by 24-hour grid. Mirrors the web admin's editor so both
/// behave the same. A slot's days always mean the day it STARTS; an overnight
/// slot (end before start) continues into the next day's column.
public enum WeekGrid {
    public static let dayMinutes = 1440

    public static func toMin(_ s: String) -> Int? {
        let p = s.split(separator: ":")
        guard p.count == 2, let h = Int(p[0]), let m = Int(p[1]), (0...24).contains(h), (0..<60).contains(m) else { return nil }
        let v = h * 60 + m
        return v <= dayMinutes ? v : nil
    }

    public static func fmtMin(_ m: Int) -> String {
        let v = max(0, min(dayMinutes, m))
        return String(format: "%02d:%02d", v / 60, v % 60)
    }

    /// "8:00 PM" style, for labels.
    public static func clockLabel(_ m: Int) -> String {
        let v = ((m % dayMinutes) + dayMinutes) % dayMinutes
        let h = v / 60, mi = v % 60
        let h12 = h % 12 == 0 ? 12 : h % 12
        return "\(h12)\(mi > 0 ? String(format: ":%02d", mi) : "") \(h < 12 ? "AM" : "PM")"
    }

    public static func snap(_ minutes: Double, _ step: Int) -> Int { Int((minutes / Double(step)).rounded()) * step }

    public static func daysOf(_ s: TimeSlot) -> [Int] { s.days.isEmpty ? Array(0...6) : s.days.sorted() }

    /// Start and end in minutes from the start day's midnight; the end may pass 1440 (overnight).
    public static func span(_ s: TimeSlot) -> (start: Int, end: Int)? {
        guard let a = toMin(s.start), let b = toMin(s.end), a != b, a < dayMinutes else { return nil }
        return (a, b > a ? b : b + dayMinutes)
    }

    /// One rectangle: part of one slot on one day's column.
    public struct Seg: Equatable {
        public var slot: Int
        public var day: Int
        public var origin: Int
        public var start: Int
        public var end: Int
        public var contFrom: Bool
        public var contTo: Bool
    }

    public static func segments(_ slots: [TimeSlot]) -> [Seg] {
        var out: [Seg] = []
        for (i, s) in slots.enumerated() {
            guard let sp = span(s) else { continue }
            for origin in daysOf(s) {
                if sp.end <= dayMinutes {
                    out.append(Seg(slot: i, day: origin, origin: origin, start: sp.start, end: sp.end, contFrom: false, contTo: false))
                } else {
                    out.append(Seg(slot: i, day: origin, origin: origin, start: sp.start, end: dayMinutes, contFrom: false, contTo: true))
                    out.append(Seg(slot: i, day: (origin + 1) % 7, origin: origin, start: 0, end: sp.end - dayMinutes, contFrom: true, contTo: false))
                }
            }
        }
        return out
    }

    /// A new slot on one day, snapped to `step`, at least one step long.
    public static func newSlot(day: Int, from a: Double, to b: Double, step: Int) -> TimeSlot {
        var start = snap(min(a, b), step), end = snap(max(a, b), step)
        if end - start < step { end = start + step }
        if end > dayMinutes { end = dayMinutes; start = max(0, end - step) }
        start = max(0, start)
        var s = TimeSlot()
        s.days = [day]; s.start = fmtMin(start); s.end = fmtMin(end)
        return s
    }

    /// Shift a slot in time, keeping its length. Ordinary slots stop at the edges of the day; overnight ones wrap.
    public static func shift(_ s: TimeSlot, by delta: Double, step: Int) -> TimeSlot {
        guard let sp = span(s) else { return s }
        let len = sp.end - sp.start
        var start = snap(Double(sp.start) + delta, step)
        if sp.end <= dayMinutes { start = max(0, min(dayMinutes - len, start)) } else { start = ((start % dayMinutes) + dayMinutes) % dayMinutes }
        let end = start + len
        var n = s
        n.start = fmtMin(start); n.end = fmtMin(end > dayMinutes ? end - dayMinutes : end)
        return n
    }

    /// Move one occurrence of a slot from the day it starts on to another day.
    public static func moveOccurrence(_ s: TimeSlot, from: Int, to: Int) -> TimeSlot {
        let to6 = max(0, min(6, to))
        if to6 == from { return s }
        let days = daysOf(s)
        guard days.contains(from), !days.contains(to6) else { return s }
        let next = Set(days.map { $0 == from ? to6 : $0 })
        var n = s
        n.days = next.count == 7 ? [] : next
        return n
    }

    /// Drag the end edge to `abs` minutes from the start day's midnight.
    public static func resizeEnd(_ s: TimeSlot, to abs: Double, step: Int) -> TimeSlot {
        guard let sp = span(s) else { return s }
        let end = max(sp.start + step, min(sp.start + dayMinutes - step, snap(abs, step)))
        var n = s
        n.end = fmtMin(end > dayMinutes ? end - dayMinutes : end)
        return n
    }

    /// Drag the start edge to `abs`, keeping the end where it is.
    public static func resizeStart(_ s: TimeSlot, to abs: Double, step: Int) -> TimeSlot {
        guard let sp = span(s) else { return s }
        let start = max(max(0, sp.end - dayMinutes + step), min(sp.end - step, snap(abs, step)))
        if start >= dayMinutes { return s }
        var n = s
        n.start = fmtMin(start)
        return n
    }

    /// Make one day of a multi-day slot its own slot, so it can differ from the rest.
    public static func detachDay(_ slots: [TimeSlot], index: Int, day: Int) -> (slots: [TimeSlot], index: Int) {
        guard slots.indices.contains(index) else { return (slots, index) }
        let days = daysOf(slots[index])
        guard days.count > 1, days.contains(day) else { return (slots, index) }
        var kept = slots[index]
        kept.days = Set(days.filter { $0 != day })
        var alone = slots[index]
        alone.id = UUID()
        alone.days = [day]
        var next = slots
        next[index] = kept
        next.append(alone)
        return (next, next.count - 1)
    }

    /// Hue (0-359) for a slot's colour, stable for what it airs.
    public static func hue(for key: String) -> Int {
        var h: UInt32 = 0
        for u in key.utf16 { h = h &* 31 &+ UInt32(u) }
        return Int(h % 360)
    }
}
