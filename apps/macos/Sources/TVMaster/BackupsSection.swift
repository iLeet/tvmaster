import AppKit
import UniformTypeIdentifiers
import SwiftUI
import TVMasterKit

/// Back up and restore the station: channels, lineups, libraries, show names, episode positions, settings and pictures.
struct BackupsSection: View {
    @Environment(AppModel.self) private var model
    @State private var list: BackupList?
    @State private var label = ""
    @State private var busy = false
    @State private var note: String?
    @State private var restoring: BackupInfo?
    @State private var deleting: BackupInfo?

    var body: some View {
        Section("Backup & restore") {
            Text("Saves your channels, lineups, libraries (their folders and what was found in them), show names, episode positions, settings and pictures. Your video files are not in it, and neither is this server's access token. Restoring first saves what is there now, so you can go back.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                TextField("Name (optional)", text: $label).frame(maxWidth: 220)
                Button { Task { await run("Backed up") { try await $0.backUpNow(label: label); label = "" } } } label: { busy ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Back up now")) }
                    .glassButton(prominent: true).disabled(busy)
                Button("Add a backup file…") { addFile() }.glassButton().disabled(busy)
            }
            if let list {
                Toggle("Back up every night (the newest \(list.keep) are kept)", isOn: Binding(get: { list.auto }, set: { v in Task { await run(nil) { try await $0.setBackupSettings(auto: v) } } }))
                if list.auto {
                    Stepper("Keep \(list.keep)", value: Binding(get: { list.keep }, set: { v in Task { await run(nil) { try await $0.setBackupSettings(keep: v) } } }), in: 1...60)
                }
                if list.backups.isEmpty { Text("No backups yet.").foregroundStyle(.secondary) }
                ForEach(list.backups) { b in
                    HStack {
                        VStack(alignment: .leading, spacing: 1) {
                            HStack(spacing: 6) {
                                Text(b.created.formatted(date: .abbreviated, time: .shortened))
                                if b.auto { Badge(text: "nightly") }
                                if !b.label.isEmpty { Badge(text: b.label, color: .accentColor) }
                            }
                            Text("\(b.counts.channels) channels · \(b.counts.libraries) libraries · \(b.counts.files) files · \(ByteCountFormatter.string(fromByteCount: b.size, countStyle: .file)) · v\(b.version)")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Download…") { Task { await save(b) } }.glassButton()
                        Button("Restore…") { restoring = b }.glassButton()
                        Button(role: .destructive) { deleting = b } label: { Image(systemName: "trash") }.glassButton()
                    }
                }
            }
            if let note { Label(note, systemImage: "checkmark.circle").font(.callout).foregroundStyle(.green) }
        }
        .task { await load() }
        .confirmationDialog("Restore this backup?", isPresented: Binding(get: { restoring != nil }, set: { if !$0 { restoring = nil } })) {
            Button("Restore", role: .destructive) {
                if let b = restoring {
                    Task {
                        busy = true; defer { busy = false }
                        if let c = model.client, let r = await model.run({ try await c.restoreBackup(b.name) }) {
                            note = "Restored \(r.channels) channels and \(r.libraries) libraries" + (r.missingFolders.isEmpty ? "" : ". \(r.missingFolders.count) library folder(s) are not on this machine") + ". The station as it was is saved as \(r.safetyCopy)."
                            await model.refreshAll()
                        }
                        await load()
                    }
                }
            }
        } message: { Text("Channels stop for a moment and every schedule is planned again. What is there now is saved first.") }
        .confirmationDialog("Delete this backup?", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
            Button("Delete", role: .destructive) { if let b = deleting { Task { await run("Deleted") { try await $0.deleteBackup(b.name) } } } }
        }
    }

    private func load() async { list = try? await model.client?.backups() }

    private func run<T>(_ done: String?, _ what: @escaping (TVMasterClient) async throws -> T) async {
        guard let c = model.client else { return }
        busy = true; defer { busy = false }
        if await model.run({ try await what(c) }) != nil, let done { note = done }
        await load()
    }

    private func addFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.gzip]
        panel.canChooseDirectories = false
        panel.prompt = "Add"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Task { await run("Backup added. Restore it from the list.") { try await $0.uploadBackup(from: url) } }
    }

    private func save(_ b: BackupInfo) async {
        guard let c = model.client, let data = await model.run({ try await c.downloadBackup(b.name) }) else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = b.name
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do { try data.write(to: url); note = "Saved \(url.lastPathComponent)" } catch { model.lastError = error.localizedDescription }
    }
}
