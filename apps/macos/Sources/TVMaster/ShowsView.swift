import SwiftUI
import TVMasterKit

/// Every show, the languages in its files, and settings that follow it onto every channel.
struct ShowsView: View {
    @Environment(AppModel.self) private var model
    @State private var shows: [ShowSummary] = []
    @State private var filter = ""
    @State private var loading = true

    private var shown: [ShowSummary] {
        filter.isEmpty ? shows : shows.filter { $0.name.localizedCaseInsensitiveContains(filter) }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                PageHeader(title: "Shows", subtitle: "Audio, subtitles and look for each show, on every channel.")
                Text("Set a show up once here and every channel that airs it follows: anime in Japanese with English subtitles, a show with subtitles already burned in (set Subtitles to Off), a 1950s serial in black and white. A channel can still override a show for itself in its Picture & sound tab.")
                    .font(.callout).foregroundStyle(.secondary)
                if shows.count > 8 { TextField("Filter shows…", text: $filter).textFieldStyle(.roundedBorder).frame(maxWidth: 300) }
                if loading && shows.isEmpty { ProgressView() }
                if !loading && shows.isEmpty { Text("No shows yet. Add a TV shows library first.").foregroundStyle(.secondary) }
                ForEach(shown) { show in
                    ShowRow(show: show) { updated in
                        if let i = shows.firstIndex(where: { $0.name == show.name }) { shows[i].defaults = updated.isEmpty ? nil : updated }
                        Task { if let c = model.client { _ = await model.run { try await c.setShowSettings(show: show.name, updated) } } }
                    }
                }
            }
            .padding(28)
        }
        .task { await load() }
    }

    private func load() async {
        guard let client = model.client else { return }
        if let v = await model.run({ try await client.shows() }) { shows = v.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending } }
        loading = false
    }
}

private struct ShowRow: View {
    var show: ShowSummary
    var onChange: (ShowDefaults) -> Void
    @State private var open = false
    @State private var audio = ""
    @State private var subLangs = ""

    private var d: ShowDefaults { show.defaults ?? ShowDefaults() }

    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            VStack(alignment: .leading, spacing: 8) {
                TextField("Audio languages, best first (jpn, eng)", text: $audio, onCommit: { commit() }).textFieldStyle(.roundedBorder)
                if let found = show.audioLanguages, !found.isEmpty {
                    Text("Found in its files: \(found.joined(separator: ", "))").font(.caption).foregroundStyle(.secondary)
                }
                Picker("Subtitles", selection: Binding(get: { d.subtitles ?? "" }, set: { var n = d; n.subtitles = $0.isEmpty ? nil : $0; onChange(n) })) {
                    Text("Whatever the channel says").tag("")
                    ForEach(PictureSoundSettings.subtitleModes, id: \.value) { Text($0.label).tag($0.value) }
                }
                TextField("Subtitle languages (eng)", text: $subLangs, onCommit: { commit() }).textFieldStyle(.roundedBorder)
                Picker("Look", selection: Binding(get: { d.look ?? "" }, set: { var n = d; n.look = $0.isEmpty ? nil : $0; onChange(n) })) {
                    Text("Whatever the channel says").tag("")
                    ForEach(PictureSoundSettings.looks, id: \.value) { Text($0.label).tag($0.value) }
                }
                Picker("Screen shape", selection: Binding(get: { d.aspect ?? "" }, set: { var n = d; n.aspect = $0.isEmpty ? nil : $0; onChange(n) })) {
                    Text("Whatever the channel says").tag("")
                    ForEach(PictureSoundSettings.aspects, id: \.value) { Text($0.label).tag($0.value) }
                }
                HStack { Spacer(); Button("Clear this show's settings") { audio = ""; subLangs = ""; onChange(ShowDefaults()) }.glassButton().disabled(d.isEmpty) }
            }
            .padding(.top, 6)
        } label: {
            HStack {
                Text(show.name).fontWeight(.semibold)
                if !d.isEmpty { Badge(text: "Set") }
                Spacer()
                Text("\(show.episodes) episodes").font(.callout).foregroundStyle(.secondary)
            }
        }
        .padding(12).glassCard(cornerRadius: 16)
        .onAppear { audio = (d.audioLanguages ?? []).joined(separator: ", "); subLangs = (d.subtitleLanguages ?? []).joined(separator: ", ") }
    }

    private func codes(_ text: String) -> [String] {
        text.split(whereSeparator: { ", ;".contains($0) }).map { $0.lowercased().trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }

    private func commit() {
        var n = d
        n.audioLanguages = codes(audio)
        n.subtitleLanguages = codes(subLangs)
        onChange(n)
    }
}
