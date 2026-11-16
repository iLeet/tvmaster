import SwiftUI
import TVMasterKit

/// A compact remote in the menu bar: change a TV's channel without opening the app.
struct MenuBarRemote: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @AppStorage("menuTV") private var tvID = ""

    private var tv: Tv? { model.tvs.first { $0.id == tvID } ?? model.tvs.first }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if model.phase != .ready {
                Text("Not connected").foregroundStyle(.secondary)
            } else if model.tvs.isEmpty {
                Text("No TVs registered yet.").foregroundStyle(.secondary)
            } else if let tv {
                Picker("TV", selection: Binding(get: { tv.id }, set: { tvID = $0 })) {
                    ForEach(model.tvs) { Text($0.name).tag($0.id) }
                }.labelsHidden()
                Text(status(tv)).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                GlassGroup(spacing: 8) {
                    HStack(spacing: 8) {
                        Button { act(tv) { try await $0.step(tv: tv.id, up: false) } } label: { Image(systemName: "chevron.down") }
                        Button { act(tv) { try await $0.step(tv: tv.id, up: true) } } label: { Image(systemName: "chevron.up") }
                        Button { act(tv) { try await $0.power(tv: tv.id, on: !tv.power) } } label: { Image(systemName: "power") }
                        Button { act(tv) { try await $0.volume(tv: tv.id, muted: !tv.muted) } } label: { Image(systemName: tv.muted ? "speaker.slash.fill" : "speaker.wave.2.fill") }
                    }
                    .buttonStyle(.borderless).controlSize(.large)
                }
                Divider()
                ForEach(model.channels.prefix(8)) { ch in
                    Button("\(ch.number)  \(ch.name)") { act(tv) { try await $0.tune(tv: tv.id, number: ch.number) } }.buttonStyle(.plain)
                }
            }
            Divider()
            Button("Copy playlist link") { copyPlaylistLink() }.buttonStyle(.plain)
            Button("Open TVMaster") { NSApp.activate(ignoringOtherApps: true); openWindow(id: "main") }.buttonStyle(.plain)
            Button("Quit") { NSApp.terminate(nil) }.buttonStyle(.plain)
        }
        .padding(14).frame(width: 260)
    }

    /// The M3U link other apps need, built the same way as on the Share screen.
    private func copyPlaylistLink() {
        guard let client = model.client else { return }
        let links = ShareLinks(network: model.system?.network, token: client.token, baseURL: client.baseURL)
        let sections = links.sections(links.defaultOptions, channels: [], tvs: [])
        guard let url = sections.flatMap(\.links).first(where: { $0.id == "playlist" })?.url else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(url, forType: .string)
    }

    private func status(_ tv: Tv) -> String {
        guard tv.power else { return "Off" }
        if let n = tv.channel, let ch = model.channels.first(where: { $0.number == n }) { return "\(n) · \(ch.name)" }
        return "On"
    }

    private func act(_ tv: Tv, _ f: @escaping @Sendable (TVMasterClient) async throws -> some Any) {
        guard let c = model.client else { return }
        Task { _ = await model.run { try await f(c) } }
    }
}
