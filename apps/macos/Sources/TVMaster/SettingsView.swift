import SwiftUI
import TVMasterKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var log = ""
    @State private var tokenText = ""
    @State private var customToken = ""
    @State private var webhookText = ""
    @State private var confirmRotate = false

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
                Section("Access token") {
                    if let sec = model.security {
                        if sec.locked {
                            Text("The token was set with --token when the server started, so it is managed there.")
                                .font(.caption).foregroundStyle(.secondary)
                        } else {
                            Toggle("Require an access token", isOn: Binding(get: { sec.enabled }, set: { v in Task { await model.changeSecurity(enabled: v) } }))
                            Text(sec.enabled
                                 ? "Every app, TV and browser must present the token. Share links can include it."
                                 : "Optional. Without one, anyone on your network can use and change this server.")
                                .font(.caption).foregroundStyle(.secondary)
                            if sec.enabled, let t = sec.token {
                                LabeledContent("Token") {
                                    HStack {
                                        Text(t).font(.caption.monospaced()).textSelection(.enabled).lineLimit(1).truncationMode(.middle)
                                        Button { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(t, forType: .string) } label: { Image(systemName: "doc.on.doc") }
                                            .buttonStyle(.borderless).help("Copy the token")
                                    }
                                }
                                HStack {
                                    Button("Make a new token") { confirmRotate = true }.glassButton()
                                    TextField("Or choose your own", text: $customToken).textFieldStyle(.roundedBorder).font(.body.monospaced())
                                    Button("Use") { let c = customToken.trimmingCharacters(in: .whitespaces); customToken = ""; Task { await model.changeSecurity(enabled: true, token: c) } }
                                        .glassButton().disabled(customToken.trimmingCharacters(in: .whitespaces).count < 8)
                                }
                                .confirmationDialog("Make a new token?", isPresented: $confirmRotate) {
                                    Button("Make a new token", role: .destructive) { Task { await model.changeSecurity(enabled: true, rotate: true) } }
                                } message: { Text("Anything using the old token (IPTV apps, other browsers) is signed out until it gets the new one.") }
                            }
                        }
                    } else {
                        Text("Connect to a server to manage its access token.").font(.caption).foregroundStyle(.secondary)
                    }
                }
                Section("Watching and alerts") {
                    if let m = model.monitor {
                        Picker("Look for new episodes", selection: Binding(get: { m.settings.watchMinutes }, set: { v in var s = m.settings; s.watchMinutes = v; Task { await model.saveMonitor(s) } })) {
                            Text("Only when I ask").tag(0)
                            ForEach([5, 10, 15, 30, 60, 120, 360], id: \.self) { Text($0 >= 60 ? "Every \($0 / 60) hour\($0 > 60 ? "s" : "")" : "Every \($0) minutes").tag($0) }
                        }
                        Text("Re-reads every library's folder so files you add or remove show up by themselves. Only what changed is read.")
                            .font(.caption).foregroundStyle(.secondary)
                        Button("Re-read every library now") { Task { _ = await model.run { try await model.client!.rescanAllLibraries() }; await model.refreshAll() } }.glassButton()
                        Toggle("Tell me when something goes wrong", isOn: Binding(get: { m.settings.alerts }, set: { v in var s = m.settings; s.alerts = v; Task { await model.saveMonitor(s) } }))
                        if m.settings.alerts {
                            TextField("Also send alerts to a web address", text: $webhookText, prompt: Text("https://ntfy.sh/my-tv-station"))
                                .font(.body.monospaced())
                                .onSubmit { var s = m.settings; s.webhook = webhookText.trimmingCharacters(in: .whitespaces); Task { await model.saveMonitor(s) } }
                                .onAppear { webhookText = m.settings.webhook ?? "" }
                            HStack {
                                Button("Save address") { var s = m.settings; s.webhook = webhookText.trimmingCharacters(in: .whitespaces); Task { await model.saveMonitor(s) } }.glassButton()
                                    .disabled(webhookText.trimmingCharacters(in: .whitespaces) == (m.settings.webhook ?? ""))
                                Button("Send a test") { Task { _ = await model.run { try await model.client!.testWebhook() } } }.glassButton().disabled((m.settings.webhook ?? "").isEmpty)
                            }
                            if !m.curl { Text("The server needs curl to send these, and it isn't installed there.").font(.caption).foregroundStyle(.orange) }
                            Toggle("Plain text (for ntfy) instead of JSON", isOn: Binding(get: { m.settings.webhookText }, set: { v in var s = m.settings; s.webhookText = v; Task { await model.saveMonitor(s) } }))
                            Toggle("Say when it is fine again", isOn: Binding(get: { m.settings.notifyRecovery }, set: { v in var s = m.settings; s.notifyRecovery = v; Task { await model.saveMonitor(s) } }))
                        }
                        if m.alerts.isEmpty { Text("Nothing is wrong right now.").font(.caption).foregroundStyle(.secondary) }
                        ForEach(m.alerts) { a in Label("\(a.title). \(a.detail)", systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange) }
                    } else {
                        Text("Connect to a server to manage monitoring.").font(.caption).foregroundStyle(.secondary)
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
                    Text("Letting other devices in turns the access token on, made for you the first time (see General). macOS may ask whether TVMaster may accept incoming connections: allow it.")
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
