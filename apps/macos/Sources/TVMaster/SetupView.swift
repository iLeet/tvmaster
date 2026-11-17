import SwiftUI
import TVMasterKit

/// From nothing to a channel on air: add media, choose a look, make the first channels.
struct SetupView: View {
    @Environment(AppModel.self) private var model
    var onDone: () -> Void

    @State private var state: SetupState?
    @State private var step = 1
    @State private var theme = ThemeChoice()
    @State private var themeLoaded = false
    @State private var adding = false
    @State private var uploading: Library?
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Set up your station", subtitle: "Three steps from nothing to a channel on air") {
                    Button("Not now") { Task { _ = await model.run { try await model.client?.skipSetup() }; onDone() } }.glassButton()
                }
                Picker("", selection: $step) { Text("1. Your media").tag(1); Text("2. The look").tag(2); Text("3. Channels").tag(3) }
                    .pickerStyle(.segmented).labelsHidden().frame(maxWidth: 480)
                Group {
                    switch step {
                    case 1: media
                    case 2: look
                    default: channels
                    }
                }
                .padding(18).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
            }
            .padding(28)
        }
        .task { await reload() }
        .sheet(isPresented: $adding) { AddLibrarySheet { _ in Task { await model.refreshLibraries(); await reload() } } }
        .sheet(item: $uploading) { lib in UploadSheet(library: lib) }
        .onChange(of: uploading) { _, new in if new == nil { Task { await model.refreshLibraries(); await reload() } } }
    }

    private var media: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Where are your shows?").font(.title3.bold())
            Text("Use files already on the server, upload them from this Mac, or both. You can add commercials, bumper clips and music later under Libraries.").foregroundStyle(.secondary)
            HStack {
                Button { adding = true } label: { Label("Use a folder…", systemImage: "folder") }.glassButton()
                if state?.canUpload == true {
                    Button { Task { await upload() } } label: { Label("Upload files from here…", systemImage: "square.and.arrow.up") }.glassButton().disabled(busy)
                }
            }
            ForEach(model.libraries.filter { $0.kind == .shows }) { l in
                HStack { Text(l.path).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle); Spacer(); Text("\(l.items) files").foregroundStyle(.secondary) }
            }
            Text((state?.items ?? 0) > 0 ? "\(state?.shows ?? 0) show\(state?.shows == 1 ? "" : "s"), \(state?.items ?? 0) files found so far." : "Nothing found yet.").font(.callout).foregroundStyle(.secondary)
            HStack { Spacer(); Button("Next") { step = 2 }.glassButton(prominent: true).disabled((state?.items ?? 0) == 0) }
        }
    }

    private var look: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("How should it look?").font(.title3.bold())
            Text("One look for the guide, the cards between shows, alerts and the on-screen graphics. You can change it any time under Appearance.").foregroundStyle(.secondary)
            ThemePickerView(theme: $theme.theme, primary: $theme.primary, accent: $theme.accent, background: $theme.background,
                            autoName: "Per channel", autoAbout: "Colours made from each channel.")
            StylePickerView(style: $theme.style)
            ThemePreviewView(choice: theme, what: "card")
            HStack { Button("Back") { step = 1 }.glassButton(); Spacer(); Button("Next") { step = 3 }.glassButton(prominent: true) }
        }
    }

    private var channels: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Make your first channels").font(.title3.bold())
            Text("A Guide channel that lists what is on, and a Mix channel that shuffles all your shows. Add more channels, weekly lineups, breaks and alerts afterwards.").foregroundStyle(.secondary)
            HStack {
                Button("Back") { step = 2 }.glassButton()
                Spacer()
                Button { Task { await finish() } } label: { busy ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Make my channels")) }
                    .glassButton(prominent: true).disabled(busy || (state?.items ?? 0) == 0)
            }
        }
    }

    private func reload() async {
        state = try? await model.client?.setupState()
        if !themeLoaded, let j = try? await model.client?.stationThemeJSON(), let t = ThemeChoice(json: j) { theme = t; themeLoaded = true }
    }

    private func upload() async {
        busy = true
        defer { busy = false }
        _ = await model.run { try await model.client?.createStationFolder() }
        await model.refreshLibraries()
        uploading = model.libraries.first { $0.kind == .shows && $0.path.replacingOccurrences(of: "\\", with: "/").contains("/media/") }
        await reload()
    }

    private func finish() async {
        busy = true
        defer { busy = false }
        _ = await model.run {
            try await model.client?.setStationTheme(theme)
            _ = try await model.client?.makeStarterChannels()
        }
        await model.refreshChannels()
        onDone()
        model.selection = .home
    }
}
