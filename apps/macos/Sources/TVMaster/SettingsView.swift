import SwiftUI
import TVMasterKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var log = ""
    @State private var tokenText = ""

    var body: some View {
        @Bindable var model = model
        TabView {
            Form {
                Section("Server") {
                    TextField("Address", text: $model.serverText, prompt: Text("http://127.0.0.1:8787"))
                    SecureField("Access token", text: $tokenText, prompt: Text(model.token.isEmpty ? "None" : "Saved in your Keychain"))
                    HStack {
                        Button("Connect") { Task { if !tokenText.isEmpty { model.token = tokenText; tokenText = "" }; await model.connect() } }.glassButton(prominent: true)
                        Button("Forget token") { model.forgetToken() }.glassButton()
                        Spacer()
                        HStack(spacing: 6) { StatusDot(on: model.live); Text(model.live ? "Connected" : "Not connected").foregroundStyle(.secondary) }
                    }
                }
                Section("This Mac as a TV") {
                    Toggle("Let a remote control this Mac's Watch screen", isOn: $model.macIsTV)
                    Text("Shows up under TVs as “\(Host.current().localizedName ?? "This Mac")” and follows channel changes from your phone or the web admin.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Section("Links for other apps and devices") {
                    Text("Playlists, guide, tuner address and a phone-remote QR code live under Share in the sidebar.")
                        .font(.caption).foregroundStyle(.secondary)
                    Button("Open Share") { model.selection = .share }.glassButton()
                }
                Section("Trusted certificates") {
                    let _ = model.pinsChanged
                    let pinned = model.pins.all.sorted { $0.key < $1.key }
                    if pinned.isEmpty {
                        Text("None. Servers with a certificate your Mac already trusts need nothing here; for a self-signed HTTPS server the app asks the first time.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(pinned, id: \.key) { item in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(item.key)
                                Text(item.value).font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                            }
                            Spacer()
                            Button("Forget") { model.forgetCertificate(for: item.key) }.glassButton()
                        }
                    }
                }
            }
            .formStyle(.grouped).tabItem { Label("General", systemImage: "gearshape") }

            Form {
                Section("Local server") {
                    Toggle("Start a server on this Mac when none is running", isOn: $model.startLocalServer)
                    Toggle("Let other devices on my network use it", isOn: Binding(get: { model.shareOnNetwork }, set: { model.shareOnNetwork = $0; Task { await model.restartLocalServer() } }))
                    Toggle("Also serve HTTPS (self-signed certificate)", isOn: Binding(get: { model.serveHTTPS }, set: { model.serveHTTPS = $0; Task { await model.restartLocalServer() } }))
                    Text("Sharing on the network always uses an access token, made for you the first time. macOS may ask whether TVMaster may accept incoming connections: allow it.")
                        .font(.caption).foregroundStyle(.secondary)
                    LabeledContent("Server program") {
                        Text(DaemonLocator.find()?.path ?? "Not found").font(.caption.monospaced()).foregroundStyle(DaemonLocator.find() == nil ? .red : .secondary)
                    }
                    LabeledContent("Status") { Text(describe(model.daemonState)) }
                    HStack {
                        Button("Stop") { Task { await model.daemon.stop() } }.glassButton()
                        Button("Restart and reconnect") { Task { await model.daemon.stop(); try? await Task.sleep(nanoseconds: 800_000_000); await model.connect() } }.glassButton()
                        Spacer()
                        Button("Show data folder") { NSWorkspace.shared.open(DaemonLocator.defaultDataDirectory) }.glassButton()
                    }
                }
                Section("Log") {
                    ScrollView { Text(log.isEmpty ? "No log yet." : log).font(.caption.monospaced()).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled) }
                        .frame(height: 220)
                    Button("Refresh log") { Task { log = await model.daemon.logTail() } }.glassButton()
                }
            }
            .formStyle(.grouped).tabItem { Label("Server", systemImage: "server.rack") }
            .task { log = await model.daemon.logTail() }

            Form {
                Section("About") {
                    LabeledContent("TVMaster for Mac", value: "0.0.1")
                    if let s = model.system {
                        LabeledContent("Server", value: "\(s.version) · \(s.os) \(s.arch)")
                        LabeledContent("FFmpeg", value: s.ffmpeg.map { "\($0.version) · \($0.encoder ?? "software")" } ?? "not found")
                    }
                    Button("Open the web admin") { model.openInBrowser("/admin/") }.glassButton()
                }
            }
            .formStyle(.grouped).tabItem { Label("About", systemImage: "info.circle") }
        }
        .frame(width: 560, height: 520)
    }

    private func describe(_ s: DaemonSupervisor.State) -> String {
        switch s {
        case .stopped: "Not started by this app"
        case .running(let pid): "Running (process \(pid))"
        case .failed(let m): "Failed: \(m)"
        }
    }
}
