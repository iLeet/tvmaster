import SwiftUI
import UniformTypeIdentifiers
import TVMasterKit

/// Choose a picture by uploading it to the server (or picking an earlier upload),
/// so nobody has to type a path that exists on another machine.
struct PictureChooser: View {
    @Environment(AppModel.self) private var model
    let title: String
    @Binding var path: String
    var hint: String?
    @State private var importing = false
    @State private var busy = false
    @State private var error: String?
    @State private var preview: NSImage?
    @State private var earlier: [Asset] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Text(title)
                Spacer()
                if let preview {
                    Image(nsImage: preview).resizable().scaledToFit().frame(height: 36).clipShape(RoundedRectangle(cornerRadius: 4))
                } else if !path.isEmpty {
                    Text(path).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle).frame(maxWidth: 220)
                }
                if busy { ProgressView().controlSize(.small) }
                Button(path.isEmpty ? "Upload…" : "Replace…") { importing = true }.glassButton().disabled(busy)
                Menu("Earlier") {
                    if earlier.isEmpty { Text("Nothing uploaded yet") }
                    ForEach(earlier) { a in Button(label(for: a)) { path = a.path } }
                }
                .menuStyle(.borderlessButton).fixedSize()
                .task { earlier = (try? await model.client?.assets()) ?? [] }
                if !path.isEmpty { Button("Remove") { path = "" }.glassButton() }
            }
            if let hint { Text(hint).font(.caption).foregroundStyle(.secondary) }
            if let error { Text(error).font(.caption).foregroundStyle(.red) }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.png, .jpeg, .gif, .webP]) { result in
            guard case .success(let url) = result else { return }
            Task { await upload(url) }
        }
        .task(id: path) { await loadPreview() }
    }

    private func label(for a: Asset) -> String {
        let name = a.id.drop(while: { $0.isHexDigit }).drop(while: { $0 == "-" })
        let stem = name.split(separator: ".").dropLast().joined(separator: ".")
        return stem.isEmpty ? "Picture \(a.id.prefix(6))" : stem
    }

    private func upload(_ url: URL) async {
        guard let client = model.client else { return }
        busy = true; error = nil
        defer { busy = false }
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let data = try Data(contentsOf: url)
            let asset = try await client.uploadAsset(data, name: url.lastPathComponent)
            path = asset.path
            earlier = (try? await client.assets()) ?? earlier
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func loadPreview() async {
        preview = nil
        guard let client = model.client, let id = TVMasterClient.assetID(forPath: path),
              let data = try? await client.assetData(id: id) else { return }
        preview = NSImage(data: data)
    }
}
