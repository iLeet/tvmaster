import SwiftUI
import TVMasterKit

/// Choose what to air: a show's next episode, a particular episode, anything, or (for a stretch of the
/// schedule) nothing. With no range it airs right now; with one it replaces that stretch.
struct AirSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var channel: Channel
    var range: (from: Date, to: Date)?
    var onDone: () -> Void = {}

    private enum What: String, CaseIterable, Identifiable {
        case show = "A show's next episode", episode = "A particular episode", random = "Anything", offAir = "Nothing (off air)"
        var id: String { rawValue }
    }
    @State private var what: What = .show
    @State private var shows: [ShowSummary] = []
    @State private var show = ""
    @State private var episodes: [Episode] = []
    @State private var episode: Int?
    @State private var from = Date()
    @State private var to = Date()
    @State private var busy = false

    private var whats: [What] { range == nil ? [.show, .episode, .random] : What.allCases }
    private var ready: Bool {
        switch what { case .show: !show.isEmpty; case .episode: episode != nil; default: true }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(range == nil ? "Air something now on \(channel.name)" : "Change what airs").font(.title2.bold())
            if range == nil {
                Text("Cuts whatever is on (a stand-by screen, say) and airs this right away. The schedule carries on after it.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            Picker("What", selection: $what) { ForEach(whats) { Text($0.rawValue).tag($0) } }.pickerStyle(.segmented).labelsHidden()
            if what == .show || what == .episode {
                Picker("Show", selection: $show) {
                    ForEach(shows) { Text("\($0.name) (\($0.episodes) episodes)").tag($0.name) }
                }
            }
            if what == .episode {
                Picker("Episode", selection: $episode) {
                    ForEach(episodes) { Text("\($0.label) · \(Format.duration($0.durationMs))").tag(Optional($0.id)) }
                }
            }
            if range != nil {
                DatePicker("From", selection: $from)
                DatePicker("To", selection: $to)
                Text("An episode longer than this stretch is cut where it ends; a shorter one is padded. It lasts until the schedule is re-rolled.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(range == nil ? "Air it now" : "Change the schedule") { Task { await go() } }
                    .glassButton(prominent: true).keyboardShortcut(.defaultAction).disabled(!ready || busy || (range != nil && to <= from))
            }
        }
        .padding(22).frame(width: 520)
        .task {
            if let r = range { from = r.from; to = r.to }
            guard let client = model.client else { return }
            let ids = channel.libraryIds.map(String.init).joined(separator: ",")
            _ = ids
            shows = ((try? await client.shows()) ?? []).sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            if show.isEmpty { show = shows.first?.name ?? "" }
        }
        .task(id: "\(what.rawValue)|\(show)") {
            guard what == .episode, !show.isEmpty, let client = model.client else { return }
            episodes = (try? await client.episodes(channel: channel.id, show: show)) ?? []
            episode = episodes.first?.id
        }
    }

    private func go() async {
        guard let client = model.client else { return }
        busy = true; defer { busy = false }
        let target: AirTarget
        switch what {
        case .show: target = .show(show)
        case .episode: target = .episode(episode ?? 0)
        case .random: target = .random
        case .offAir: target = .offAir
        }
        let ok: Void? = await model.run {
            if let r = range { try await client.editSchedule(channel: channel.id, from: from, to: to, target); _ = r }
            else { try await client.play(channel: channel.id, target) }
        }
        if ok != nil { await model.refreshChannels(); onDone(); dismiss() }
    }
}
