import SwiftUI
import TVMasterKit

/// What the station calls a show, which folders it is made of, and its description, rating, tags and poster.
struct ShowDetailsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let show: ShowSummary
    var onChanged: () -> Void

    @State private var title = ""
    @State private var info = ShowInfo()
    @State private var tags = ""
    @State private var poster = ""
    @State private var seasons: [String: String] = [:]
    @State private var busy = false

    private var folders: [ShowFolder] { show.folders ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(show.name).font(.title2.bold())
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Title on air").font(.headline)
                        TextField("Title", text: $title).textFieldStyle(.roundedBorder)
                        Text("What the guide, the schedule and the apps call this show. The folder names stay as they are.").font(.caption).foregroundStyle(.secondary)
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Folders (\(folders.count))").font(.headline)
                        Text(folders.count > 1 ? "One show made of several folders. Give each its season so the episodes air in order." : "The folder this show was found in.")
                            .font(.caption).foregroundStyle(.secondary)
                        ForEach(folders) { f in
                            HStack {
                                Text(f.raw).lineLimit(1).truncationMode(.middle)
                                Text("· \(f.episodes) episode\(f.episodes == 1 ? "" : "s")").foregroundStyle(.secondary)
                                Spacer()
                                Text("Season").foregroundStyle(.secondary)
                                TextField("", text: Binding(get: { seasons[f.raw] ?? f.season.map(String.init) ?? "" }, set: { seasons[f.raw] = $0 }), onCommit: { Task { await setSeason(f) } })
                                    .textFieldStyle(.roundedBorder).frame(width: 56)
                                if f.custom { Button("Reset") { Task { await reset(f) } }.glassButton() }
                            }
                        }
                        if folders.count > 1, folders.contains(where: { $0.season == nil }) {
                            Text("Some folders have no season, so their episodes will mix with the others by episode number. Give each one a season.")
                                .font(.caption).foregroundStyle(.orange)
                        }
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Description").font(.headline)
                        TextEditor(text: $info.description).frame(height: 80).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                        Text("Shown in IPTV guides and the apps.").font(.caption).foregroundStyle(.secondary)
                    }
                    HStack(alignment: .top, spacing: 14) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Rating").font(.headline)
                            HStack(spacing: 6) {
                                TextField("TV-PG, 14+, …", text: $info.rating).textFieldStyle(.roundedBorder).frame(width: 120)
                                Menu("Pick…") { ForEach(["TV-Y", "TV-Y7", "TV-G", "TV-PG", "TV-14", "TV-MA", "G", "PG", "PG-13", "R", "C", "C8", "14+", "18+"], id: \.self) { r in Button(r) { info.rating = r } } }.fixedSize()
                            }
                            Text("Day parts use it to keep strong shows out of the morning.").font(.caption).foregroundStyle(.secondary)
                        }
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Tags").font(.headline)
                            TextField("sitcom, 80s", text: $tags).textFieldStyle(.roundedBorder)
                        }
                    }
                    PictureChooser(title: "Poster", path: $poster)
                }
                .padding(.trailing, 6)
            }
            HStack {
                Spacer()
                Button("Close") { dismiss() }.glassButton()
                Button { Task { await save() } } label: { busy ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Save")) }
                    .glassButton(prominent: true).disabled(busy).keyboardShortcut(.defaultAction)
            }
        }
        .padding(22).frame(width: 560, height: 640)
        .onAppear {
            title = show.name
            info = show.info ?? ShowInfo()
            tags = info.tags.joined(separator: ", ")
            poster = info.poster ?? ""
        }
    }

    private func setSeason(_ f: ShowFolder) async {
        guard let c = model.client else { return }
        let text = (seasons[f.raw] ?? "").trimmingCharacters(in: .whitespaces)
        if text == (f.season.map(String.init) ?? "") { return }
        _ = await model.run { try await c.setFolderSeason(raw: f.raw, title: show.name, season: Int(text)) }
        onChanged()
    }

    private func reset(_ f: ShowFolder) async {
        guard let c = model.client else { return }
        _ = await model.run { try await c.resetFolder(raw: f.raw) }
        onChanged()
    }

    private func save() async {
        guard let c = model.client else { return }
        busy = true
        defer { busy = false }
        let t = title.trimmingCharacters(in: .whitespaces)
        var i = info
        i.tags = tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        i.poster = poster.isEmpty ? nil : poster
        let ok: Void? = await model.run {
            if !t.isEmpty, t != show.name { try await c.mergeShows([show.name], title: t) }
            try await c.setShowInfo(show: t.isEmpty ? show.name : t, i)
        }
        if ok != nil { onChanged(); dismiss() }
    }
}

/// Rename a show, or put several together as one.
struct MergeSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let names: [String]
    var onDone: () -> Void
    @State private var title = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(names.count > 1 ? "Put these together as one show" : "Rename show").font(.title2.bold())
            if names.count > 1 {
                Text(names.joined(separator: ", ") + ". Their folders become one show; give each folder its season under Name & details so episodes air in order.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            TextField("Title on air", text: $title).textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.glassButton()
                Button("Save") {
                    Task {
                        if let c = model.client, await model.run({ try await c.mergeShows(names, title: title) }) != nil { onDone(); dismiss() }
                    }
                }.glassButton(prominent: true).disabled(title.trimmingCharacters(in: .whitespaces).isEmpty).keyboardShortcut(.defaultAction)
            }
        }
        .padding(22).frame(width: 460)
        .onAppear { title = names.first ?? "" }
    }
}
