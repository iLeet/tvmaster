import SwiftUI
import TVMasterKit

/// Take over any channel, or the whole station, right now: test screen, blank,
/// snow or an emergency alert, and start, stop or restart streams.
struct LiveControlView: View {
    @Environment(AppModel.self) private var model

    @State private var target: Set<String> = []          // empty = every channel
    @State private var event = ""
    @State private var message = ""
    @State private var minutes = 0
    @State private var style = "full"
    @State private var theme = "retro"
    @State private var severity = "emergency"
    @State private var sound = "attention"
    @State private var shot: NSImage?
    @State private var busy = false
    @State private var airOn: Channel?
    @State private var confirm: Confirmation?

    private struct Confirmation: Identifiable {
        let id = UUID()
        let title: String
        let detail: String
        let action: () async -> Void
    }

    private var state: LiveState { model.liveState }
    private var scopeName: String {
        switch target.count {
        case 0: "every channel"
        case 1: model.channels.first { $0.id == target.first }?.name ?? "1 channel"
        default: "\(target.count) channels"
        }
    }
    private var canAlert: Bool {
        !event.trimmingCharacters(in: .whitespaces).isEmpty && !message.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Live control", subtitle: "Take over any channel, or the whole station, right now.")

                if !state.overrides.isEmpty {
                    HStack(spacing: 12) {
                        Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                        Text(state.overrides.count == model.channels.count
                             ? "The whole station is not showing normal programming."
                             : "\(state.overrides.count) channel\(state.overrides.count == 1 ? " is" : "s are") not showing normal programming.")
                            .fontWeight(.semibold)
                        Spacer()
                        Button("Resume all programming") { Task { await giveBack([]) } }.glassButton(prominent: true).disabled(busy)
                    }
                    .padding(14).glassCard(cornerRadius: 18, tint: .orange)
                }

                wholeStation
                alertComposer
                channelList
            }
            .padding(28)
        }
        .sheet(item: $airOn) { ch in AirSheet(channel: ch) { } }
        .alert(item: $confirm) { c in
            Alert(title: Text(c.title), message: Text(c.detail),
                  primaryButton: .destructive(Text("Do it")) { Task { await c.action() } },
                  secondaryButton: .cancel())
        }
    }

    // MARK: whole station

    private var wholeStation: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Whole station").font(.headline)
            Text("One press changes every channel, the guide included. Channels that are on air switch within seconds, and the choice survives a restart until you resume.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                ForEach(LiveMode.allCases) { m in
                    Button("All to \(m.label.lowercased())") {
                        confirm = Confirmation(title: "Put every channel on the \(m.label.lowercased())?",
                                               detail: "Nobody watching will see their programming until you resume it.") { await takeOver([], m.rawValue) }
                    }.glassButton().help(m.help).disabled(busy)
                }
                Spacer()
                Button("Resume all programming") { Task { await giveBack([]) } }.glassButton(prominent: true).disabled(busy || state.overrides.isEmpty)
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
    }

    // MARK: alert

    private var alertComposer: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack { Text("Emergency alert").font(.headline); Badge(text: "Overrides programming", color: .orange) }
            Text("Takes over the picture with an alert screen: a flashing header, the event, your message and a crawl, with the two-tone attention signal at the start. The samples all say they are tests, so use them for a drill.")
                .font(.callout).foregroundStyle(.secondary)
            if !state.samples.isEmpty {
                HStack {
                    Text("Start from").font(.caption).foregroundStyle(.secondary)
                    ForEach(state.samples) { s in
                        Button(s.name) {
                            event = s.event; message = s.text
                            if let v = s.severity { severity = v }
                            if let v = s.sound { sound = v }
                        }.glassButton()
                    }
                }
            }
            let o = state.options
            HStack(alignment: .top) {
                Picker("Shown as", selection: $style) { ForEach(o?.styles ?? [LiveChoice(value: "full", label: "Full screen")]) { Text($0.label).tag($0.value) } }
                Picker("Look", selection: $theme) { ForEach(o?.themes ?? [LiveChoice(value: "retro", label: "Retro")]) { Text($0.label).tag($0.value) } }
            }
            HStack(alignment: .top) {
                Picker("How serious", selection: $severity) { ForEach(o?.severities ?? [LiveChoice(value: "emergency", label: "Emergency (red)")]) { Text($0.label).tag($0.value) } }
                Picker("Sound", selection: $sound) { ForEach(o?.sounds ?? [LiveChoice(value: "attention", label: "Attention tone")]) { Text($0.label).tag($0.value) } }
            }
            if style == "bar" {
                Text("The programme keeps playing underneath. If a channel has nothing playing, the alert takes the whole screen.").font(.caption).foregroundStyle(.secondary)
            }
            if let shot {
                Image(nsImage: shot).resizable().aspectRatio(contentMode: .fit).frame(maxWidth: 520)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
            }
            TextField("Event, like “Tornado warning”", text: $event)
            TextField("What people should know and do", text: $message, axis: .vertical).lineLimit(3...6)
            HStack {
                Picker("How long", selection: $minutes) {
                    Text("Until I clear it").tag(0); Text("5 minutes").tag(5); Text("15 minutes").tag(15); Text("1 hour").tag(60)
                }.frame(width: 260)
                Spacer()
                Menu {
                    Button("Every channel") { target = [] }
                    Divider()
                    ForEach(model.channels) { ch in
                        Toggle("\(ch.number)  \(ch.name)", isOn: Binding(get: { target.contains(ch.id) }, set: { on in if on { target.insert(ch.id) } else { target.remove(ch.id) } }))
                    }
                } label: { Label("Where: \(scopeName)", systemImage: "tv") }
            }
            HStack {
                Spacer()
                Button {
                    let e = event.trimmingCharacters(in: .whitespaces), m = message.trimmingCharacters(in: .whitespaces)
                    let ids = Array(target), name = scopeName, mins = minutes
                    confirm = Confirmation(title: "Put the alert “\(e)” on \(name)?",
                                           detail: "It replaces whatever is airing until it ends or you resume programming.") {
                        await takeOver(ids, "alert", title: e, message: m, minutes: mins, look: (style, theme, severity, sound))
                    }
                } label: { Label("Put alert on \(scopeName)", systemImage: "exclamationmark.triangle.fill") }
                    .glassButton(prominent: true).tint(.red).disabled(busy || !canAlert)
            }
        }
        .textFieldStyle(.roundedBorder)
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
        .task(id: "\(style)|\(theme)|\(severity)|\(event)|\(message)") {
            // Redraw the picture shortly after the choices stop changing.
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard !Task.isCancelled, let client = model.client else { return }
            if let data = try? await client.alertPreview(style: style, theme: theme, severity: severity, title: event, message: message),
               let img = NSImage(data: data) { shot = img }
        }
    }

    // MARK: channels

    private var channelList: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Channels").font(.headline)
            ForEach(model.channels) { ch in
                let o = state.override(for: ch.id)
                let onAir = ch.onAir ?? false
                HStack(spacing: 12) {
                    NumberBadge(number: ch.number)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text(ch.name).fontWeight(.semibold)
                            if let o { Badge(text: o.mode == "alert" ? "\(o.modeLabel): \(o.title)" : o.modeLabel, color: .orange) } else { Badge(text: "Programming", color: .green) }
                            StatusDot(on: onAir)
                        }
                        Text(detail(ch, o)).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    Menu("Take over") {
                        ForEach(LiveMode.allCases) { m in
                            Button(m.label) { Task { await takeOver([ch.id], m.rawValue) } }
                        }
                        Divider()
                        Button("Emergency alert…") { target = [ch.id] }
                    }.menuStyle(.borderlessButton).fixedSize().disabled(busy)
                    if ch.isScheduled {
                        Button("Air now…") { airOn = ch }.glassButton().disabled(busy)
                        Button { Task { await refresh(ch) } } label: { Image(systemName: "arrow.clockwise") }.glassButton().disabled(busy)
                            .help("Put the channel on the plan for this moment, even part way through a show")
                    }
                    if o != nil { Button("Resume") { Task { await giveBack([ch.id]) } }.glassButton(prominent: true).disabled(busy) }
                    Menu {
                        Button(onAir ? "Restart stream" : "Start stream") { Task { await stream(ch, onAir ? "restart" : "start") } }
                        if onAir { Button("Stop stream") { Task { await stream(ch, "stop") } } }
                    } label: { Image(systemName: "ellipsis.circle") }.menuStyle(.borderlessButton).fixedSize().disabled(busy)
                }
                .padding(12).glassCard(cornerRadius: 16)
            }
        }
    }

    private func detail(_ ch: Channel, _ o: LiveOverride?) -> String {
        if let o {
            if let until = o.untilMs { return "Until \(Date(timeIntervalSince1970: Double(until) / 1000).formatted(date: .omitted, time: .shortened))" }
            return "Until you resume"
        }
        if let now = ch.now { return "Now: \(now.show ?? now.title)" }
        return ch.isGuide ? "Guide channel" : "Nothing scheduled right now"
    }

    // MARK: actions

    private func takeOver(_ ids: [String], _ mode: String, title: String? = nil, message: String? = nil, minutes: Int? = nil,
                          look: (style: String, theme: String, severity: String, sound: String)? = nil) async {
        guard let client = model.client else { return }
        busy = true; defer { busy = false }
        if let v = await model.run({ try await client.takeOver(channels: ids, mode: mode, title: title, message: message, minutes: minutes,
                                                                style: look?.style, theme: look?.theme, severity: look?.severity, sound: look?.sound) }) { model.liveState = v }
        await model.refreshChannels()
    }

    private func giveBack(_ ids: [String]) async {
        guard let client = model.client else { return }
        busy = true; defer { busy = false }
        if let v = await model.run({ try await client.giveBack(channels: ids) }) { model.liveState = v }
        await model.refreshChannels()
    }

    private func refresh(_ ch: Channel) async {
        guard let client = model.client else { return }
        busy = true; defer { busy = false }
        _ = await model.run { try await client.refresh(channel: ch.id) }
        await model.refreshChannels()
    }

    private func stream(_ ch: Channel, _ action: String) async {
        guard let client = model.client else { return }
        busy = true; defer { busy = false }
        _ = await model.run { try await client.stream(channel: ch.id, action: action) }
        await model.refreshChannels()
    }
}
