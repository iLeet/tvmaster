import AppKit
import SwiftUI
import TVMasterKit

struct LibrariesView: View {
    @Environment(AppModel.self) private var model
    @State private var adding = false
    @State private var busy: Int?
    @State private var removing: Library?
    @State private var note: String?

    private var analysing: Bool { model.libraries.contains { $0.analysis?.running == true } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Libraries", subtitle: "Folders of shows, commercials, bumpers and music") {
                    Button { adding = true } label: { Label("Add library", systemImage: "plus") }.glassButton(prominent: true)
                }
                if let note { Label(note, systemImage: "checkmark.circle").foregroundStyle(.green) }
                if model.libraries.isEmpty {
                    Text("No libraries yet. Add the folder where your TV collection lives.").foregroundStyle(.secondary)
                }
                GlassGroup(spacing: 10) {
                    VStack(spacing: 10) {
                        ForEach(model.libraries) { lib in
                            VStack(alignment: .leading, spacing: 8) {
                                HStack(spacing: 12) {
                                    Image(systemName: "folder.fill").foregroundStyle(.tint).font(.title3)
                                    VStack(alignment: .leading, spacing: 1) {
                                        Text(lib.name).fontWeight(.semibold)
                                        Text(lib.path).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                                    }
                                    Spacer()
                                    Badge(text: lib.kind.label, color: lib.kind == .shows ? .accentColor : .secondary)
                                    Text("\(lib.items.formatted()) \(lib.items == 1 ? "file" : "files")").foregroundStyle(.secondary).frame(width: 90, alignment: .trailing)
                                }
                                HStack {
                                    Toggle("Enabled", isOn: Binding(get: { lib.enabled }, set: { v in Task { await setEnabled(lib, v) } })).toggleStyle(.switch)
                                    Spacer()
                                    Button { Task { await scan(lib) } } label: { busy == lib.id ? AnyView(ProgressView().controlSize(.small)) : AnyView(Label("Scan", systemImage: "arrow.clockwise")) }.glassButton().disabled(busy == lib.id)
                                    if lib.kind == .shows {
                                        Button("Find ad breaks") { Task { await analyze(lib) } }.glassButton()
                                            .help("Detect natural break points (black frames) in files that have none")
                                    }
                                    Button(role: .destructive) { removing = lib } label: { Image(systemName: "trash") }.glassButton()
                                }
                                if let a = lib.analysis {
                                    HStack {
                                        ProgressView(value: Double(a.done), total: Double(max(a.total, 1)))
                                        Text(a.running ? "Finding ad breaks \(a.done)/\(a.total)" : "Ad breaks found for \(a.total) files").font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                            }
                            .padding(16).glassCard(cornerRadius: 20).opacity(lib.enabled ? 1 : 0.6)
                        }
                    }
                }
            }
            .padding(28)
        }
        .sheet(isPresented: $adding) { AddLibrarySheet { note = $0 } }
        .confirmationDialog("Remove “\(removing?.name ?? "")”?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } })) {
            Button("Remove library", role: .destructive) { if let l = removing { Task { await remove(l) } } }
        } message: { Text("Your files on disk are not touched. Channels using this library will be rescheduled.") }
        .task(id: analysing) {
            guard analysing else { return }
            while !Task.isCancelled && model.libraries.contains(where: { $0.analysis?.running == true }) {
                try? await Task.sleep(nanoseconds: 2_000_000_000)
                await model.refreshLibraries()
            }
        }
    }

    private func scan(_ lib: Library) async {
        guard let c = model.client else { return }
        busy = lib.id
        if let r = await model.run({ try await c.scan(library: lib.id) }) {
            note = "Found \(r.found) files, indexed \(r.indexed)" + (r.skipped > 0 ? ", skipped \(r.skipped)" : "") + ((r.analysing ?? 0) > 0 ? ", analysing \(r.analysing!) for ad breaks" : "")
        }
        busy = nil
        await model.refreshLibraries()
    }

    private func analyze(_ lib: Library) async {
        guard let c = model.client else { return }
        _ = await model.run { try await c.analyze(library: lib.id) }
        await model.refreshLibraries()
    }

    private func setEnabled(_ lib: Library, _ v: Bool) async {
        guard let c = model.client else { return }
        _ = await model.run { try await c.setEnabled(library: lib.id, v) }
        await model.refreshLibraries(); await model.refreshChannels()
    }

    private func remove(_ lib: Library) async {
        guard let c = model.client else { return }
        _ = await model.run { try await c.deleteLibrary(lib.id) }
        await model.refreshLibraries(); await model.refreshChannels()
    }
}

struct AddLibrarySheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var onDone: (String) -> Void
    @State private var kind: LibraryKind = .shows
    @State private var path = ""
    @State private var busy = false

    private var isLocal: Bool { model.client?.isLocal ?? false }
    private var help: String {
        switch kind {
        case .shows: "Folders of episodes (Show/Season 1/Show - S01E02 - Title.mkv). These feed your channels."
        case .commercials: "Retro ads. Put the year in a folder or file name (1987/…) to filter ads by era."
        case .bumpers: "Your own station IDs and stings, used to open commercial breaks."
        case .music: "Background music for the guide channel."
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Add a library").font(.title2.bold())
            Picker("Kind", selection: $kind) { ForEach(LibraryKind.allCases) { Text($0.label).tag($0) } }.pickerStyle(.segmented)
            Text(help).font(.callout).foregroundStyle(.secondary)
            HStack {
                TextField("Folder on the server", text: $path, prompt: Text("/path/on/the/server")).font(.body.monospaced())
                if isLocal { Button("Choose…") { choose() }.glassButton() }
            }
            if !isLocal { Text("This server is on another machine: enter the folder's path as that machine sees it.").font(.caption).foregroundStyle(.secondary) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button { Task { await add() } } label: { busy ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Add and scan")) }
                    .glassButton(prominent: true).keyboardShortcut(.defaultAction).disabled(path.isEmpty || busy)
            }
        }
        .padding(24).frame(width: 520)
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"
        panel.message = "Choose a folder of \(kind.label.lowercased())"
        if panel.runModal() == .OK, let url = panel.url { path = url.path }
    }

    private func add() async {
        guard let c = model.client else { return }
        busy = true
        defer { busy = false }
        guard let lib = await model.run({ try await c.addLibrary(path: path, kind: kind) }) else { return }
        dismiss()
        if let r = await model.run({ try await c.scan(library: lib.id) }) {
            onDone("Added “\(lib.name)”: found \(r.found) files, indexed \(r.indexed)")
        }
        await model.refreshLibraries(); await model.refreshChannels()
    }
}
