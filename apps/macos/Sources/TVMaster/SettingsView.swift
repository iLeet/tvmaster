import SwiftUI
import TVMasterKit

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var log = ""
    @State private var tokenText = ""
    @State private var customToken = ""
    @State private var tzApplyAll = true
    @State private var webhookText = ""
    @State private var confirmRotate = false
    @State private var confirmServerRestart = false

    var body: some View {
        @Bindable var model = model
        TabView {
            Form {
                Section("Time zone") {
                    if let tz = model.timezone {
                        LabeledContent("Station time zone", value: tz.zone)
                        Text(tz.source == "setting" ? "Chosen by you." : tz.source == "environment" ? "From the server's TZ setting." : "Automatic: the server machine's own zone. A server in a container is often on UTC.")
                            .font(.caption).foregroundStyle(.secondary)
                        Toggle("Move every channel's schedule into the zone I choose", isOn: $tzApplyAll)
                        HStack {
                            if tz.zone != TimeZone.current.identifier {
                                Button("Use this Mac's: \(TimeZone.current.identifier)") { Task { await model.setTimezone(TimeZone.current.identifier, applyToChannels: tzApplyAll) } }.glassButton(prominent: true)
                            }
                            if tz.source == "setting" { Button("Back to automatic") { Task { await model.setTimezone(nil, applyToChannels: tzApplyAll) } }.glassButton() }
                        }
                        if !tz.channelsElsewhere.isEmpty {
                            Label(tz.channelsElsewhere.map { "\($0.name) (\($0.zone))" }.joined(separator: ", ") + " in another zone", systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange)
                        }
                    } else { Text("Connect to a server to set its time zone.").font(.caption).foregroundStyle(.secondary) }
                }
                Section("New episodes") {
                    if let m = model.monitor {
                        Picker("Look for new episodes", selection: Binding(get: { m.settings.watchMinutes }, set: { v in var s = m.settings; s.watchMinutes = v; Task { await model.saveMonitor(s) } })) {
                            Text("Only when I ask").tag(0)
                            ForEach([5, 10, 15, 30, 60, 120, 360], id: \.self) { Text($0 >= 60 ? "Every \($0 / 60) hour\($0 > 60 ? "s" : "")" : "Every \($0) minutes").tag($0) }
                        }
                        Text("Re-reads every library's folder so files you add or remove show up by themselves. Only what changed is read.")
                            .font(.caption).foregroundStyle(.secondary)
                        Button("Re-read every library now") { Task { _ = await model.run { try await model.client!.rescanAllLibraries() }; await model.refreshAll() } }.glassButton()
                    } else {
                        Text("Connect to a server to manage monitoring.").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .formStyle(.grouped).tabItem { Label("Station", systemImage: "antenna.radiowaves.left.and.right") }

            Form {
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
            }
            .formStyle(.grouped).tabItem { Label("Connect devices", systemImage: "tv.and.mediabox") }

            Form {
                Section("Alerts") {
                    Text("Told about problems before the family is: a drive that goes offline, a channel that keeps failing, FFmpeg missing.").font(.caption).foregroundStyle(.secondary)
                    if let m = model.monitor {
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
                        Text("Connect to a server to manage alerts.").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .formStyle(.grouped).tabItem { Label("Alerts", systemImage: "bell") }

            Form {
                BackupsSection()
            }
            .formStyle(.grouped).tabItem { Label("Backups", systemImage: "externaldrive") }

            Form {
                MachineSections()
                Section("Server") {
                    TextField("Address", text: $model.serverText, prompt: Text("http://127.0.0.1:8787"))
                    DisclosureGroup("Servers on my network") {
                        if model.scanning { ProgressView().controlSize(.small) }
                        if model.discovered.isEmpty && !model.scanning { Text("None found.").font(.caption).foregroundStyle(.secondary) }
                        ForEach(model.discovered) { s in
                            HStack {
                                VStack(alignment: .leading, spacing: 0) {
                                    Text(s.title).fontWeight(.semibold)
                                    Text("\(s.host)\(s.version.map { " · TVMaster \($0)" } ?? "")").font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("Use") { Task { await model.use(s) } }.glassButton()
                            }
                        }
                        Button(model.scanning ? "Looking…" : "Look for servers") { model.findServers() }.glassButton().disabled(model.scanning)
                    }
                    SecureField("Access token", text: $tokenText, prompt: Text(model.token.isEmpty ? "None" : "Saved on this Mac"))
                    HStack {
                        Button("Connect") { Task { if !tokenText.isEmpty { model.token = tokenText; tokenText = "" }; await model.connect() } }.glassButton(prominent: true)
                        Button("Forget token") { model.forgetToken() }.glassButton()
                        Spacer()
                        HStack(spacing: 6) { StatusDot(on: model.live); Text(model.live ? "Connected" : "Not connected").foregroundStyle(.secondary) }
                    }
                }
                Section("Control the server") {
                    Text("Works on whichever server you are connected to, here or across the network. Streams start and stop from Live control.")
                        .font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("Restart the server…") { confirmServerRestart = true }.glassButton().disabled(!model.live)
                        Button("Open the web admin") { model.openInBrowser("/admin/") }.glassButton()
                        Button("Live control") { model.selection = .live }.glassButton()
                    }
                    Text("A restart stops every channel for about ten seconds. The server comes back by itself in Docker, as a service, or as the Linux or Windows package; one started by hand just stops.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                .confirmationDialog("Restart the server?", isPresented: $confirmServerRestart) {
                    Button("Restart", role: .destructive) {
                        Task { if let c = model.client { _ = await model.run { try await c.restartServer() }; try? await Task.sleep(nanoseconds: 6_000_000_000); await model.connect() } }
                    }
                }
                Section("Access token") {
                    if let sec = model.security {
                        if sec.locked {
                            Text("The token was set with --token when the server started, so it is managed there.")
                                .font(.caption).foregroundStyle(.secondary)
                        } else {
                            Toggle("Let anyone tune in without the token (changing anything still needs it)", isOn: Binding(get: { sec.openViewing ?? false }, set: { v in Task { _ = await model.run { try await model.client!.setOpenViewing(v) }; await model.refreshSecurity() } }))
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
            .formStyle(.grouped).tabItem { Label("Server", systemImage: "server.rack") }

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
                Section("About") {
                    LabeledContent("TVMaster for Mac", value: "0.0.1")
                    if let s = model.system {
                        LabeledContent("Server", value: "\(s.version) · \(s.os) \(s.arch)")
                        LabeledContent("FFmpeg", value: s.ffmpeg.map { "\($0.version) · \($0.encoder ?? "software")" } ?? "not found")
                    }
                    Button("Open the web admin") { model.openInBrowser("/admin/") }.glassButton()
                }
            }
            .formStyle(.grouped).tabItem { Label("This Mac", systemImage: "desktopcomputer") }
            .task { log = await model.daemon.logTail() }
        }
        .frame(width: 600, height: 560)
    }

    private func describe(_ s: DaemonSupervisor.State) -> String {
        switch s {
        case .stopped: "Not started by this app"
        case .running(let pid): "Running (process \(pid))"
        case .failed(let m): "Failed: \(m)"
        }
    }
}

/// The machine the server runs on and how hard it is working, refreshed every few seconds.
struct MachineSections: View {
    @Environment(AppModel.self) private var model
    @State private var st: SystemStats?

    var body: some View {
        Group {
            if let st {
                let busy = st.busyWord
                Section("How it is doing") {
                    HStack { Spacer(); Text(busy.word).font(.callout.weight(.semibold)).foregroundStyle(busy.level == 0 ? Color.green : busy.level == 1 ? .orange : .red) }
                    meter("Processor", st.cpuPct / 100, "\(Int(st.cpuPct))% of \(st.threads) threads")
                    meter("Memory", st.memTotal > 0 ? st.memUsed / st.memTotal : 0, "\(SystemStats.bytes(st.memUsed)) of \(SystemStats.bytes(st.memTotal))\(st.memLimitedByContainer ? " (container limit)" : "")")
                    Text("Load average \(st.load.map { String(format: "%.2f", $0) }.joined(separator: " · ")) (1, 5, 15 minutes; about the number of threads is full)").font(.caption).foregroundStyle(.secondary)
                    Text(st.capacitySentence)
                }
                Section("What TVMaster is using") {
                    LabeledContent("The server itself", value: st.serverCpuPct.map { "\(Int($0))% of a thread · \(SystemStats.bytes(st.serverMemory ?? 0)) · up \(SystemStats.duration(st.serverUptime ?? 0))" } ?? "—")
                    LabeledContent("Encoders (FFmpeg)", value: "\(st.ffmpegProcesses) running · \(String(format: "%.2f", st.ffmpegCores)) cores · \(SystemStats.bytes(st.ffmpegMemory))")
                    LabeledContent("Encoding with", value: (st.encoder ?? "not available") + (st.encoder == "software" ? " (the processor does it all)" : ""))
                    LabeledContent("On air now", value: st.onAir.isEmpty ? "nothing: channels start when someone tunes in" : st.onAir.joined(separator: ", "))
                }
                Section("The machine") {
                    LabeledContent("Name", value: st.hostName + (st.container ? " (in a container)" : ""))
                    LabeledContent("System", value: "\(st.os) · \(st.arch)\(st.kernel.isEmpty ? "" : " · kernel \(st.kernel)")")
                    LabeledContent("Processor", value: "\(st.cpuModel) · \(st.cores.map { "\($0) cores, " } ?? "")\(st.threads) threads")
                    LabeledContent("Memory", value: "\(SystemStats.bytes(st.memTotal)) · \(SystemStats.bytes(st.memAvailable)) free")
                    LabeledContent("Up for", value: SystemStats.duration(st.uptime))
                    LabeledContent("TVMaster", value: st.version)
                }
                Section("Disks") {
                    ForEach(st.disks) { d in
                        meter("\(d.mount)\(d.name.isEmpty ? "" : " (\(d.name))")", d.total > 0 ? (d.total - d.free) / d.total : 0, "\(SystemStats.bytes(d.free)) free of \(SystemStats.bytes(d.total)) · \(d.usedFor.joined(separator: ", "))")
                    }
                }
            } else {
                Section("This machine") { ProgressView().controlSize(.small) }
            }
        }
        .task {
            while !Task.isCancelled {
                if let client = model.client, let s = try? await client.systemStats() { st = s }
                try? await Task.sleep(nanoseconds: 4_000_000_000)
            }
        }
    }

    private func meter(_ label: String, _ frac: Double, _ text: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack { Text(label); Spacer(); Text(text).font(.caption).foregroundStyle(.secondary) }
            ProgressView(value: min(1, max(0, frac))).tint(frac < 0.6 ? .green : frac < 0.85 ? .orange : .red)
        }
    }
}
