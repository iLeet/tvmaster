import AppKit
import SwiftUI
import TVMasterKit

/// Copy files (or whole folders) from this Mac into a library on the server, which may be another machine.
struct UploadSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let library: Library

    struct Item: Identifiable {
        let id = UUID()
        let url: URL
        let path: String
        let bytes: Int64
        var progress = 0.0
        var state = "waiting"
        var message: String?
    }

    @State private var items: [Item] = []
    @State private var subfolder = ""
    @State private var running = false

    private var pending: Bool { items.contains { $0.state == "waiting" || $0.state == "failed" } }
    private var total: Int64 { items.reduce(0) { $0 + $1.bytes } }
    private let videoExts: Set<String> = ["mkv", "mp4", "m4v", "avi", "mov", "ts", "m2ts", "mpg", "mpeg", "wmv", "webm", "flv"]
    private let audioExts: Set<String> = ["mp3", "m4a", "aac", "flac", "wav", "ogg", "opus", "wma", "aiff"]
    private let subExts: Set<String> = ["srt", "ass", "ssa", "vtt"]

    private var accepted: Set<String> {
        switch library.kind {
        case .music: audioExts
        case .shows: videoExts.union(subExts)
        default: videoExts
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Upload to \(library.name)").font(.title2.bold())
            Text("Drop files or folders here, or choose them. The files are copied into this library's folder on the server, then added to the schedule.\(library.kind == .shows ? " Choose a whole folder to keep its Show/Season layout." : "")")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                Button("Choose files or folders…") { choose() }.glassButton().disabled(running)
                TextField("Sub-folder (optional)", text: $subfolder).frame(width: 220)
            }
            if !items.isEmpty {
                List($items) { $it in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(it.path).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle)
                            Spacer()
                            Text(UploadPlan.size(it.bytes)).font(.caption).foregroundStyle(.secondary)
                            Text(it.state == "exists" ? "already there" : it.state).font(.caption.bold())
                                .foregroundStyle(it.state == "done" ? .green : it.state == "failed" ? .orange : .secondary)
                        }
                        if it.state == "sending" { ProgressView(value: it.progress) }
                        if let m = it.message, it.state == "failed" { Text(m).font(.caption).foregroundStyle(.orange) }
                    }
                }
                .frame(minHeight: 180)
            }
            HStack {
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.cancelAction).disabled(running)
                if items.contains(where: { $0.state == "exists" }) {
                    Button("Replace the ones already there") { Task { await run(overwrite: true) } }.glassButton().disabled(running)
                }
                Button { Task { await run(overwrite: false) } } label: {
                    running ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text(items.isEmpty ? "Upload" : "Upload \(items.count) file\(items.count == 1 ? "" : "s") (\(UploadPlan.size(total)))"))
                }
                .glassButton(prominent: true).keyboardShortcut(.defaultAction).disabled(!pending || running)
            }
        }
        .padding(24).frame(width: 640, height: 520)
        .dropDestination(for: URL.self) { urls, _ in
            guard !running else { return false }
            for url in urls { addURL(url) }
            return !urls.isEmpty
        }
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = true
        panel.prompt = "Choose"
        guard panel.runModal() == .OK else { return }
        for url in panel.urls { addURL(url) }
    }

    /// A file, or a whole folder (which keeps its own name and layout: Show/Season 1/Episode.mkv).
    private func addURL(_ url: URL) {
        var isDir: ObjCBool = false
        FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir)
        if isDir.boolValue {
            let base = url.deletingLastPathComponent()
            let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey], options: [.skipsHiddenFiles])
            while let f = walker?.nextObject() as? URL { add(f, base: base) }
        } else {
            add(url, base: url.deletingLastPathComponent())
        }
    }

    private func add(_ file: URL, base: URL) {
        guard accepted.contains(file.pathExtension.lowercased()) else { return }
        let size = (try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(Int64.init) ?? 0
        let path = UploadPlan.relativePath(file: file, base: base, subfolder: subfolder)
        if !items.contains(where: { $0.path == path }) { items.append(Item(url: file, path: path, bytes: size)) }
    }

    private func run(overwrite: Bool) async {
        guard let client = model.client else { return }
        running = true
        defer { running = false }
        for i in items.indices {
            let it = items[i]
            if it.state == "done" || (it.state == "exists" && !overwrite) { continue }
            items[i].state = "sending"; items[i].progress = 0; items[i].message = nil
            do {
                let overwriteThis = overwrite && it.state == "exists"
                _ = try await client.uploadFile(library: library.id, path: it.path, from: it.url, overwrite: overwriteThis) { f in
                    Task { @MainActor in if i < items.count { items[i].progress = f } }
                }
                items[i].state = "done"; items[i].progress = 1
            } catch {
                let m = error.localizedDescription
                items[i].state = m.contains("already there") ? "exists" : "failed"
                items[i].message = m
            }
        }
        await model.refreshLibraries(); await model.refreshChannels()
    }
}
