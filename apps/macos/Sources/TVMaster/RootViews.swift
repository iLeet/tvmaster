import SwiftUI
import TVMasterKit

struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ZStack {
            AppBackground()
            switch model.phase {
            case .connecting:
                VStack(spacing: 14) { ProgressView(); Text("Connecting…").foregroundStyle(.secondary) }
            case .needsServer(let message):
                ConnectView(mode: .server, message: message)
            case .needsToken(let message):
                ConnectView(mode: .token, message: message)
            case .ready:
                MainView()
            }
        }
        .frame(minWidth: 900, minHeight: 600)
        .task { if model.phase == .connecting { await model.connect() } }
    }
}

struct ConnectView: View {
    enum Mode { case server, token }
    @Environment(AppModel.self) private var model
    var mode: Mode
    var message: String?
    @State private var tokenText = ""

    var body: some View {
        @Bindable var model = model
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                Image(systemName: "tv.inset.filled").font(.system(size: 30)).foregroundStyle(.tint)
                VStack(alignment: .leading) {
                    Text("TVMaster").font(.title.bold())
                    Text(mode == .server ? "Connect to your TV station" : "This server needs an access token").foregroundStyle(.secondary)
                }
            }
            if let message { Label(message, systemImage: "exclamationmark.triangle").foregroundStyle(.orange).font(.callout) }

            if mode == .server {
                TextField("Server address", text: $model.serverText, prompt: Text("http://127.0.0.1:8787"))
                    .textFieldStyle(.roundedBorder).onSubmit { Task { await model.connect() } }
                Toggle("Start a server on this Mac if none is running", isOn: $model.startLocalServer)
                HStack {
                    Spacer()
                    Button("Connect") { Task { await model.connect() } }.glassButton(prominent: true).keyboardShortcut(.defaultAction)
                }
            } else {
                SecureField("Access token", text: $tokenText).textFieldStyle(.roundedBorder)
                    .onSubmit { Task { await model.submitToken(tokenText) } }
                HStack {
                    Button("Change server") { Task { await model.disconnect() } }.glassButton()
                    Spacer()
                    Button("Sign in") { Task { await model.submitToken(tokenText) } }.glassButton(prominent: true)
                        .keyboardShortcut(.defaultAction).disabled(tokenText.trimmingCharacters(in: .whitespaces).count < 8)
                }
            }
        }
        .padding(26).frame(width: 440).glassCard(cornerRadius: 28)
    }
}

struct MainView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            List(SidebarItem.allCases, selection: Binding(get: { model.selection }, set: { model.selection = $0 ?? .home })) { item in
                Label(item.title, systemImage: item.symbol).tag(item)
            }
            .navigationSplitViewColumnWidth(min: 170, ideal: 200, max: 260)
            .safeAreaInset(edge: .bottom) {
                HStack(spacing: 8) {
                    StatusDot(on: model.live)
                    Text(model.live ? "Live" : "Reconnecting…").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(12)
            }
        } detail: {
            ZStack(alignment: .top) {
                Group {
                    switch model.selection {
                    case .home: HomeView()
                    case .watch: WatchView()
                    case .channels: ChannelsView()
                    case .schedule: ScheduleView()
                    case .libraries: LibrariesView()
                    case .tvs: TVsView()
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                if let error = model.lastError {
                    HStack {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                        Text(error).lineLimit(2)
                        Button { model.lastError = nil } label: { Image(systemName: "xmark") }.buttonStyle(.plain)
                    }
                    .padding(.horizontal, 16).padding(.vertical, 10).glassPill().padding(.top, 10)
                    .transition(.move(edge: .top).combined(with: .opacity))
                }
            }
            .animation(.snappy, value: model.lastError)
        }
        .toolbar {
            ToolbarItemGroup {
                Button { Task { await model.refreshAll() } } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    .help("Refresh everything")
                Button { model.openInBrowser("/admin/") } label: { Label("Open web admin", systemImage: "safari") }
                    .help("Open the full web admin (lineups, ads, guide design)")
            }
        }
    }
}

/// A page title with an optional trailing control.
struct PageHeader<Trailing: View>: View {
    var title: String
    var subtitle: String?
    @ViewBuilder var trailing: Trailing
    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.largeTitle.bold())
                if let subtitle { Text(subtitle).foregroundStyle(.secondary) }
            }
            Spacer()
            trailing
        }
    }
}
extension PageHeader where Trailing == EmptyView {
    init(title: String, subtitle: String? = nil) { self.init(title: title, subtitle: subtitle) { EmptyView() } }
}
