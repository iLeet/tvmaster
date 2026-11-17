import SwiftUI
import TVMasterKit

struct ScheduleView: View {
    @Environment(AppModel.self) private var model
    @State private var channelID: String?
    @State private var hours = 6
    @State private var showBreaks = false
    @State private var programs: [Program] = []
    @State private var items: [ScheduleItem] = []
    @State private var loading = false
    @State private var air: AirRequest?

    private struct AirRequest: Identifiable {
        let id = UUID()
        var range: (from: Date, to: Date)?
    }

    private var channel: Channel? { model.channels.first { $0.id == channelID } }
    private var programmable: [Channel] { model.channels.filter { $0.isScheduled } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Schedule", subtitle: channel.map { "\($0.number) · \($0.name)" }) {
                    HStack {
                        Picker("Channel", selection: $channelID) {
                            ForEach(programmable) { Text("\($0.number) · \($0.name)").tag(Optional($0.id)) }
                        }.labelsHidden().frame(width: 200)
                        Picker("Hours", selection: $hours) { ForEach([3, 6, 12, 24, 48], id: \.self) { Text("\($0) h").tag($0) } }
                            .labelsHidden().frame(width: 80)
                        Picker("View", selection: $showBreaks) { Text("Guide").tag(false); Text("Breaks").tag(true) }
                            .pickerStyle(.segmented).labelsHidden().frame(width: 140)
                    }
                }

                if let channel {
                    HStack(spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(channel.now.map { "Now: \($0.headline)" } ?? "Nothing on right now").fontWeight(.semibold)
                            if let n = channel.next { Text("Next: \(n.headline) at \(n.start.formatted(date: .omitted, time: .shortened))").foregroundStyle(.secondary) }
                        }
                        Spacer()
                        Button { air = AirRequest(range: nil) } label: { Label("Air now…", systemImage: "play.fill") }.glassButton()
                            .help("Cut to something right now (a stand-by screen, say)")
                        Button { Task { await refreshNow() } } label: { Label("Refresh now", systemImage: "arrow.clockwise") }.glassButton()
                            .help("Put the channel on the plan for this moment, even part way through a show")
                        Button { Task { await reroll() } } label: { Label("Re-roll", systemImage: "arrow.triangle.2.circlepath") }.glassButton()
                            .help("Regenerate everything that hasn't aired yet")
                        Button { let a = Date().timeIntervalSince1970; let from = Date(timeIntervalSince1970: (a / 1800).rounded(.up) * 1800)
                            air = AirRequest(range: (from, from.addingTimeInterval(1800))) } label: { Text("Change a time…") }.glassButton()
                            .help("Change what airs in a stretch of time")
                    }
                    .padding(16).glassCard()
                }

                if loading && programs.isEmpty { ProgressView() }

                if showBreaks {
                    VStack(spacing: 0) {
                        ForEach(items) { it in
                            HStack(spacing: 10) {
                                Text(it.start.formatted(date: .omitted, time: .standard)).font(.callout.monospacedDigit()).foregroundStyle(.secondary).frame(width: 84, alignment: .leading)
                                Label(kind(it), systemImage: symbol(it)).labelStyle(.titleAndIcon).foregroundStyle(color(it)).frame(width: 130, alignment: .leading)
                                Text(it.title ?? "").lineLimit(1)
                                if it.kind == "show" && it.inMs > 0 { Text("act from \(Format.duration(it.inMs))").font(.caption).foregroundStyle(.secondary) }
                                Spacer()
                                Text(Format.duration(it.durationMs)).foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 7).padding(.horizontal, 14)
                            Divider().opacity(0.3)
                        }
                    }
                    .glassCard()
                    if items.isEmpty && !loading { Text("Nothing scheduled in this window.").foregroundStyle(.secondary) }
                } else {
                    VStack(spacing: 0) {
                        ForEach(rows) { row in
                          if let p = row.program {
                            HStack(alignment: .firstTextBaseline, spacing: 12) {
                                Text(p.start.formatted(date: .abbreviated, time: .shortened)).font(.callout.monospacedDigit()).foregroundStyle(.secondary).frame(width: 150, alignment: .leading)
                                VStack(alignment: .leading, spacing: 1) {
                                    HStack(spacing: 6) {
                                        Text(p.headline).fontWeight(.semibold)
                                        if let c = p.episodeCode { Text(c).font(.caption).foregroundStyle(.secondary) }
                                    }
                                    if let s = p.subtitle { Text(s).font(.callout).foregroundStyle(.secondary) }
                                }
                                Spacer()
                                if let l = p.label { Badge(text: l) }
                                Text(Format.duration(p.endMs - p.startMs)).foregroundStyle(.secondary)
                                if p.end > Date() { Button("Change…") { air = AirRequest(range: (max(p.start, Date()), p.end)) }.glassButton() }
                            }
                            .padding(.vertical, 9).padding(.horizontal, 16)
                            Divider().opacity(0.3)
                          } else if let g = row.gap {
                            HStack(alignment: .firstTextBaseline, spacing: 12) {
                                Text(g.from.formatted(date: .abbreviated, time: .shortened)).font(.callout.monospacedDigit()).foregroundStyle(.secondary).frame(width: 150, alignment: .leading)
                                Text("Off air").italic().foregroundStyle(.secondary)
                                Spacer()
                                Text(Format.duration(Int64(g.to.timeIntervalSince(g.from) * 1000))).foregroundStyle(.secondary)
                                Button("Fill…") { air = AirRequest(range: (g.from, g.to)) }.glassButton()
                            }
                            .padding(.vertical, 9).padding(.horizontal, 16)
                            Divider().opacity(0.3)
                          }
                        }
                    }
                    .glassCard()
                    if programs.isEmpty && !loading { Text("Nothing scheduled. Add shows to the channel's libraries.").foregroundStyle(.secondary) }
                }
            }
            .padding(28)
        }
        .sheet(item: $air) { a in
            if let channel { AirSheet(channel: channel, range: a.range) { Task { await load() } } }
        }
        .task { if channelID == nil { channelID = programmable.first?.id } }
        .task(id: "\(channelID ?? "")-\(hours)-\(showBreaks)") { await load() }
        .onChange(of: model.channels.count) { _, _ in if channelID == nil { channelID = programmable.first?.id } }
    }

    private func load() async {
        guard let client = model.client, let id = channelID else { return }
        loading = true
        defer { loading = false }
        if showBreaks {
            if let v = await model.run({ try await client.scheduleItems(channel: id, hours: min(hours, 3)) }) { items = v }
        } else {
            if let v = await model.run({ try await client.schedule(channel: id, hours: hours) }) { programs = v }
        }
    }

    /// The programmes with the stretches between them (off air) as rows of their own.
    private struct Row: Identifiable {
        let id: String
        var program: Program?
        var gap: (from: Date, to: Date)?
    }

    private var rows: [Row] {
        var out: [Row] = []
        var at = max(programs.first?.start ?? Date(), Date())
        for p in programs {
            if p.start.timeIntervalSince(at) >= 60 { out.append(Row(id: "gap-\(at.timeIntervalSince1970)", program: nil, gap: (at, p.start))) }
            out.append(Row(id: p.id, program: p, gap: nil))
            at = max(at, p.end)
        }
        return out
    }

    private func refreshNow() async {
        guard let client = model.client, let id = channelID else { return }
        _ = await model.run { try await client.refresh(channel: id) }
        await model.refreshChannels()
        await load()
    }

    private func reroll() async {
        guard let client = model.client, let id = channelID else { return }
        _ = await model.run { try await client.reroll(channel: id) }
        await load()
    }

    private func kind(_ it: ScheduleItem) -> String {
        switch it.kind {
        case "bumper": (it.bumper ?? "clip").replacingOccurrences(of: "_", with: " ").capitalized
        default: it.kind.capitalized
        }
    }
    private func symbol(_ it: ScheduleItem) -> String {
        switch it.kind { case "show": "play.rectangle"; case "commercial": "megaphone"; case "bumper": "sparkles.tv"; default: "rectangle.dashed" }
    }
    private func color(_ it: ScheduleItem) -> Color {
        switch it.kind { case "show": .accentColor; case "commercial": .orange; case "bumper": .purple; default: .secondary }
    }
}
