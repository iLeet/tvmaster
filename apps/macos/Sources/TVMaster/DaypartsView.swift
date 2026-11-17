import SwiftUI
import TVMasterKit

/// Day parts: named stretches of the broadcast day, each with the strongest thing allowed to air in it, whether there
/// are commercial breaks, and a look of its own for the station's graphics and alerts.
struct DaypartsView: View {
    @Environment(AppModel.self) private var model
    @State private var state = DaypartsState()
    @State private var parts: [DaypartSetting] = []
    @State private var saved: [DaypartSetting] = []
    @State private var loaded = false
    @State private var saving = false
    @State private var error: String?

    private var dirty: Bool { parts != saved }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Day parts", subtitle: "Kids in the morning, a darker look late at night") {
                    HStack {
                        if dirty { Text("Unsaved").foregroundStyle(.orange) }
                        Button { Task { await save() } } label: { saving ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Save")) }
                            .glassButton(prominent: true).disabled(!dirty || saving)
                    }
                }
                Text("A day part is a stretch of the broadcast day (\(state.timezone)) with its own rules: the strongest rating that may air, whether there are commercial breaks, and a look for the station's graphics and alerts. The first one that covers a moment, for a channel, applies. Outside them, channels carry on as usual.")
                    .foregroundStyle(.secondary)
                if let error { Text(error).foregroundStyle(.red) }

                if parts.isEmpty && loaded {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Start with one").font(.headline)
                        ForEach(state.presets.indices, id: \.self) { i in
                            let p = state.presets[i]
                            Button { parts.append(p.part) } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("\(p.part.name)  ").fontWeight(.semibold) + Text(p.part.summary).foregroundStyle(.secondary)
                                    Text(p.about).font(.caption).foregroundStyle(.secondary)
                                }.frame(maxWidth: .infinity, alignment: .leading)
                            }
                            .buttonStyle(.plain).padding(12).glassCard(cornerRadius: 16)
                        }
                    }
                }

                ForEach(parts.indices, id: \.self) { i in
                    PartCard(part: $parts[i], index: i, count: parts.count, state: state, saved: saved.indices.contains(i) && saved[i] == parts[i],
                             onMove: { to in parts = InfoSettings.moved(parts, from: i, to: to) },
                             onRemove: { parts.remove(at: i) })
                }

                if !parts.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Add a day part").font(.headline)
                        HStack {
                            ForEach(state.presets.indices, id: \.self) { i in
                                Button("+ \(state.presets[i].part.name)") { parts.append(state.presets[i].part) }.glassButton().help(state.presets[i].about)
                            }
                            Button("+ Blank") { parts.append(DaypartSetting()) }.glassButton()
                        }
                    }
                    .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
                }
            }
            .padding(24)
        }
        .task { await load() }
    }

    private func load() async {
        guard let s = try? await model.client?.dayparts() else { return }
        state = s
        if !loaded { parts = s.parts }
        saved = s.parts
        loaded = true
    }

    private func save() async {
        guard let client = model.client else { return }
        saving = true; error = nil
        defer { saving = false }
        do {
            let s = try await client.setDayparts(parts)
            state = s; parts = s.parts; saved = s.parts
        } catch { self.error = error.localizedDescription }
    }
}

private struct PartCard: View {
    @Binding var part: DaypartSetting
    let index: Int
    let count: Int
    let state: DaypartsState
    /// This part is as the server has it, so its counts are up to date.
    let saved: Bool
    var onMove: (Int) -> Void
    var onRemove: () -> Void
    @State private var confirmRemove = false

    private func date(_ hm: String) -> Date {
        let p = hm.split(separator: ":").compactMap { Int($0) }
        return Calendar.current.date(bySettingHour: p.first ?? 0, minute: p.count > 1 ? p[1] : 0, second: 0, of: Date()) ?? Date()
    }
    private func hm(_ d: Date) -> String {
        let c = Calendar.current.dateComponents([.hour, .minute], from: d)
        return String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                TextField("Name", text: $part.name).textFieldStyle(.roundedBorder).frame(maxWidth: 240).fontWeight(.semibold)
                Text(part.summary).foregroundStyle(.secondary).font(.callout)
                Spacer()
                Button { onMove(index - 1) } label: { Image(systemName: "chevron.up") }.buttonStyle(.borderless).disabled(index == 0).help("Higher priority")
                Button { onMove(index + 1) } label: { Image(systemName: "chevron.down") }.buttonStyle(.borderless).disabled(index == count - 1).help("Lower priority")
                Button("Remove…", role: .destructive) { confirmRemove = true }.glassButton()
                    .confirmationDialog("Remove “\(part.name)”?", isPresented: $confirmRemove) {
                        Button("Remove", role: .destructive, action: onRemove)
                    } message: { Text("Channels go back to their usual programming in these hours.") }
            }
            HStack(spacing: 18) {
                DatePicker("From", selection: Binding(get: { date(part.start) }, set: { part.start = hm($0) }), displayedComponents: .hourAndMinute)
                DatePicker("Until", selection: Binding(get: { date(part.end == "24:00" ? "00:00" : part.end) }, set: { part.end = hm($0) }), displayedComponents: .hourAndMinute)
                Text("Earlier than the start = runs overnight.").font(.caption).foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(part.days.isEmpty ? "Days: every day. Pick some to limit it." : "Days").font(.caption).foregroundStyle(.secondary)
                HStack(spacing: 6) {
                    ForEach(0..<7, id: \.self) { d in
                        Toggle(DaypartSetting.dayNames[d], isOn: Binding(get: { part.days.contains(d) }, set: { on in
                            if on { part.days = (part.days + [d]).sorted() } else { part.days.removeAll { $0 == d } }
                        })).toggleStyle(.button)
                    }
                }
            }
            VStack(alignment: .leading, spacing: 4) {
                Text(part.channels.isEmpty ? "Channels: every TV channel. Pick some to limit it." : "Channels: only these.").font(.caption).foregroundStyle(.secondary)
                HStack(spacing: 6) {
                    ForEach(state.channels) { c in
                        Toggle(c.name, isOn: Binding(get: { part.channels.contains(c.id) }, set: { on in
                            if on { part.channels.append(c.id) } else { part.channels.removeAll { $0 == c.id } }
                        })).toggleStyle(.button)
                    }
                }
            }

            Divider()
            Text("What may air").font(.headline)
            Picker("Strongest rating", selection: $part.maxRating) {
                Text("No limit").tag("")
                ForEach(state.ratings) { r in Text("\(r.name) (\(r.about))").tag(r.id) }
            }
            Text("Shows rated above this are held back in these hours. Rate a show under Shows.").font(.caption).foregroundStyle(.secondary)
            if !part.maxRating.isEmpty {
                Toggle("Let shows without a rating air", isOn: $part.allowUnrated)
                if saved, state.check.indices.contains(index) {
                    let c = state.check[index]
                    Text("\(c.airing) of \(c.shows) shows can air under this limit.\(c.heldBack > 0 ? " \(c.heldBack) held back" : "")\(c.unrated > 0 ? " (\(c.unrated) \(c.unrated == 1 ? "has" : "have") no rating)" : "").\(c.airing == 0 ? " Nothing would air: rate some shows, or let unrated ones air." : "")")
                        .font(.callout).foregroundStyle(c.airing == 0 ? .orange : .secondary)
                }
            }
            Picker("Commercial breaks", selection: $part.breaksOff) {
                Text("As the channel is set up").tag(false)
                Text("None: shows run clean").tag(true)
            }.pickerStyle(.segmented)

            Divider()
            Toggle("A look of its own (cards, bumpers, bug, lower thirds, tickers and alerts in these hours)", isOn: Binding(get: { part.look != nil }, set: { part.look = $0 ? ThemeChoice.kids : nil }))
            if part.look != nil {
                let look = Binding(get: { part.look ?? ThemeChoice() }, set: { part.look = $0 })
                Button("Use the kids look") { part.look = .kids }.glassButton()
                ThemeChoiceEditorView(choice: look)
                HStack(alignment: .top, spacing: 12) {
                    ThemePreviewView(choice: look.wrappedValue, what: "card")
                    ThemePreviewView(choice: look.wrappedValue, what: "alert")
                }
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
    }
}
