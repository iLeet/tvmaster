import SwiftUI
import TVMasterKit

/// The week at a glance: drag on an empty spot to add a slot, drag a slot to move it (to another day too),
/// drag its bottom edge to resize it, click it to edit. The same editor as the web admin.
struct WeekGridView: View {
    @Binding var slots: [TimeSlot]
    var shows: [String]
    var step: Int

    @State private var drag: Drag?
    @State private var editing: UUID?

    private let hourH: CGFloat = 30
    private let gutter: CGFloat = 46
    private let head: CGFloat = 24
    private let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]

    private enum Mode { case create(day: Int), move(seg: WeekGrid.Seg, grabMin: Double), resize(seg: WeekGrid.Seg) }
    private struct Drag { var mode: Mode; var start: CGPoint; var now: CGPoint }

    private var step5: Int { max(5, step) }

    /// Where the grid opens: an hour before the earliest slot (or the early evening when there are none).
    private var firstHour: Int {
        let starts = WeekGrid.segments(slots).map(\.start)
        guard let m = starts.min() else { return 17 }
        return max(0, min(16, m / 60 - 1))
    }

    var body: some View {
        GeometryReader { geo in
            let colW = max(40, (geo.size.width - gutter) / 7)
            ScrollViewReader { proxy in
            ScrollView(.vertical) {
                ZStack(alignment: .topLeading) {
                    gridLines(colW: colW)
                    ForEach(0..<24, id: \.self) { h in Color.clear.frame(width: 1, height: 1).offset(y: head + CGFloat(h) * hourH).id(h) }
                    ForEach(Array(WeekGrid.segments(working).enumerated()), id: \.offset) { _, seg in
                        block(seg, colW: colW)
                    }
                    if case .create(let day)? = drag?.mode, let d = drag {
                        let a = minutes(d.start.y), b = minutes(d.now.y)
                        let s = WeekGrid.newSlot(day: day, from: a, to: b, step: step5)
                        if let sp = WeekGrid.span(s) {
                            RoundedRectangle(cornerRadius: 6).fill(Color.accentColor.opacity(0.35))
                                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.accentColor))
                                .frame(width: colW - 4, height: CGFloat(sp.end - sp.start) / 60 * hourH)
                                .offset(x: gutter + CGFloat(day) * colW + 2, y: head + CGFloat(sp.start) / 60 * hourH)
                        }
                    }
                }
                .frame(width: geo.size.width, height: head + hourH * 24, alignment: .topLeading)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onChanged { v in
                        if drag == nil { drag = begin(v.startLocation, colW: colW) }
                        drag?.now = v.location
                    }
                    .onEnded { v in
                        defer { drag = nil }
                        guard let d = drag else { return }
                        finish(d, at: v.location, colW: colW)
                    })
            }
            .onAppear { lastColW = colW; proxy.scrollTo(firstHour, anchor: .top) }
            .onChange(of: geo.size.width) { _, _ in lastColW = colW }
            }
        }
        .frame(height: 420)
        .sheet(isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
            if let id = editing, let i = slots.firstIndex(where: { $0.id == id }) {
                VStack(spacing: 12) {
                    Form { SlotEditor(slot: $slots[i], shows: shows) { slots.remove(at: i); editing = nil } }.formStyle(.grouped)
                    HStack {
                        if WeekGrid.daysOf(slots[i]).count > 1, let day = WeekGrid.segments([slots[i]]).first?.origin {
                            Button("Make just \(days[day]) its own slot") {
                                let r = WeekGrid.detachDay(slots, index: i, day: day)
                                slots = r.slots; editing = slots[r.index].id
                            }.glassButton()
                        }
                        Button("Duplicate") { var c = slots[i]; c.id = UUID(); if !c.name.isEmpty { c.name += " (copy)" }; slots.append(c); editing = c.id }.glassButton()
                        Spacer()
                        Button("Done") { editing = nil }.glassButton(prominent: true).keyboardShortcut(.defaultAction)
                    }
                }
                .padding(18).frame(width: 520, height: 540)
            }
        }
    }

    // MARK: geometry

    private func minutes(_ y: CGFloat) -> Double { Double(max(0, min(hourH * 24, y - head)) / hourH * 60) }

    private func dayIndex(_ x: CGFloat, colW: CGFloat) -> Int { max(0, min(6, Int((x - gutter) / colW))) }

    private func rect(_ seg: WeekGrid.Seg, colW: CGFloat) -> CGRect {
        CGRect(x: gutter + CGFloat(seg.day) * colW + 2, y: head + CGFloat(seg.start) / 60 * hourH,
               width: colW - 4, height: max(10, CGFloat(seg.end - seg.start) / 60 * hourH))
    }

    /// The slots as they will be if the drag in progress is let go.
    private var working: [TimeSlot] {
        guard let d = drag, let (i, slot) = applied(d, colW: lastColW) else { return slots }
        var s = slots
        s[i] = slot
        return s
    }

    @State private var lastColW: CGFloat = 100

    private func applied(_ d: Drag, colW: CGFloat) -> (Int, TimeSlot)? {
        switch d.mode {
        case .create: return nil
        case .move(let seg, let grab):
            guard slots.indices.contains(seg.slot) else { return nil }
            let delta = minutes(d.now.y) - grab
            var s = WeekGrid.shift(slots[seg.slot], by: delta, step: step5)
            let target = dayIndex(d.now.x, colW: colW)
            if target != seg.day { s = WeekGrid.moveOccurrence(s, from: seg.origin, to: seg.origin + (target - seg.day)) }
            return (seg.slot, s)
        case .resize(let seg):
            guard slots.indices.contains(seg.slot) else { return nil }
            let abs = minutes(d.now.y) + (seg.contFrom ? Double(WeekGrid.dayMinutes) : 0)
            return (seg.slot, WeekGrid.resizeEnd(slots[seg.slot], to: abs, step: step5))
        }
    }

    private func begin(_ p: CGPoint, colW: CGFloat) -> Drag {
        let segs = WeekGrid.segments(slots)
        if let seg = segs.last(where: { rect($0, colW: colW).contains(p) }) {
            let r = rect(seg, colW: colW)
            if !seg.contTo && p.y > r.maxY - 8 { return Drag(mode: .resize(seg: seg), start: p, now: p) }
            return Drag(mode: .move(seg: seg, grabMin: minutes(p.y)), start: p, now: p)
        }
        return Drag(mode: .create(day: dayIndex(p.x, colW: colW)), start: p, now: p)
    }

    private func finish(_ d: Drag, at p: CGPoint, colW: CGFloat) {
        var d = d
        d.now = p
        let moved = hypot(p.x - d.start.x, p.y - d.start.y)
        switch d.mode {
        case .create(let day):
            guard moved >= 6 else { return }
            slots.append(WeekGrid.newSlot(day: day, from: minutes(d.start.y), to: minutes(p.y), step: step5))
            editing = slots.last?.id
        case .move(let seg, _), .resize(let seg):
            if moved < 4 { if slots.indices.contains(seg.slot) { editing = slots[seg.slot].id }; return }
            if let (i, s) = applied(d, colW: colW) { slots[i] = s }
        }
    }

    // MARK: drawing

    private func gridLines(colW: CGFloat) -> some View {
        ZStack(alignment: .topLeading) {
            ForEach(0..<7, id: \.self) { d in
                Text(days[d]).font(.caption.bold()).foregroundStyle(.secondary)
                    .frame(width: colW, height: head).offset(x: gutter + CGFloat(d) * colW, y: 0)
            }
            ForEach(0..<25, id: \.self) { h in
                Rectangle().fill(.secondary.opacity(h % 6 == 0 ? 0.35 : 0.14)).frame(height: 1)
                    .offset(x: gutter, y: head + CGFloat(h) * hourH)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if h < 24 && h % 2 == 0 {
                    Text(WeekGrid.clockLabel(h * 60)).font(.caption2).foregroundStyle(.secondary)
                        .frame(width: gutter - 6, alignment: .trailing).offset(y: head + CGFloat(h) * hourH - 7)
                }
            }
            ForEach(0..<8, id: \.self) { d in
                Rectangle().fill(.secondary.opacity(0.18)).frame(width: 1, height: hourH * 24)
                    .offset(x: gutter + CGFloat(d) * colW, y: head)
            }
        }
    }

    private func block(_ seg: WeekGrid.Seg, colW: CGFloat) -> some View {
        let slot = working[seg.slot]
        let r = rect(seg, colW: colW)
        let title = slot.name.isEmpty ? describe(slot.source) : slot.name
        let hue = Double(WeekGrid.hue(for: describe(slot.source))) / 360
        return RoundedRectangle(cornerRadius: 6)
            .fill(Color(hue: hue, saturation: 0.45, brightness: 0.78).opacity(0.92))
            .overlay(alignment: .topLeading) {
                VStack(alignment: .leading, spacing: 0) {
                    Text(title).font(.caption.bold()).lineLimit(1)
                    if r.height > 34, let sp = WeekGrid.span(slot) {
                        Text("\(WeekGrid.clockLabel(sp.start))–\(WeekGrid.clockLabel(sp.end))").font(.caption2).lineLimit(1)
                    }
                }
                .foregroundStyle(.black.opacity(0.85)).padding(.horizontal, 4).padding(.top, 2)
            }
            .overlay(alignment: .bottom) { if !seg.contTo { Capsule().fill(.black.opacity(0.35)).frame(width: 18, height: 3).padding(.bottom, 2) } }
            .frame(width: r.width, height: r.height)
            .offset(x: r.minX, y: r.minY)
            .allowsHitTesting(false)
    }

    private func describe(_ s: SourceChoice) -> String {
        switch s.kind {
        case .show: s.show.isEmpty ? "A show" : s.show
        case .pool: s.shows.isEmpty ? "Every show" : s.shows.count == 1 ? s.shows[0] : "\(s.shows.count) shows"
        case .chaos: "Anything"
        case .rotation: "A rotation"
        case .other: "Special"
        }
    }
}
