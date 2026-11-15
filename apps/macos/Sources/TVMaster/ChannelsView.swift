import SwiftUI
import TVMasterKit

struct ChannelsView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Channel?

    var body: some View {
        @Bindable var model = model
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Channels", subtitle: "Your stations. Lineups, ads and the guide are designed in the web admin.") {
                    Button { model.showNewChannel = true } label: { Label("New channel", systemImage: "plus") }.glassButton(prominent: true)
                        .keyboardShortcut("n", modifiers: .command)
                }
                if model.channels.isEmpty {
                    Text("No channels yet. Create your first one to put something on air.").foregroundStyle(.secondary)
                }
                GlassGroup(spacing: 10) {
                    VStack(spacing: 10) {
                        ForEach(model.channels) { ch in
                            HStack(spacing: 12) {
                                NumberBadge(number: ch.number)
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 6) {
                                        Text(ch.name).fontWeight(.semibold)
                                        if ch.isGuide { Badge(text: "Guide") }
                                        Badge(text: ch.autopilot ? "Autopilot" : "Locked", color: ch.autopilot ? .green : .orange)
                                        if let g = ch.group { Text(g).font(.caption).foregroundStyle(.secondary) }
                                    }
                                    Text(summary(ch)).font(.callout).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("Watch") { model.watch(ch) }.glassButton()
                                Button("Edit") { editing = ch }.glassButton()
                            }
                            .padding(14).glassCard(cornerRadius: 20)
                        }
                    }
                }
            }
            .padding(28)
        }
        .sheet(item: $editing) { ch in ChannelEditorSheet(channel: ch) }
        .sheet(isPresented: $model.showNewChannel) { ChannelEditorSheet(channel: nil) }
    }

    private func summary(_ ch: Channel) -> String {
        if ch.isGuide { return "\(ch.guide.hours)-hour live guide" }
        let l = ch.lineupSummary
        return l.isEmpty ? "\(ch.strategy.label)\(ch.chaos > 0 ? " · chaos \(ch.chaos)%" : "")" : l
    }
}

struct Badge: View {
    var text: String
    var color: Color = .accentColor
    var body: some View {
        Text(text.uppercased()).font(.system(size: 9.5, weight: .bold)).tracking(0.5).foregroundStyle(color)
            .padding(.horizontal, 7).padding(.vertical, 2).background(color.opacity(0.16), in: Capsule())
    }
}

struct ChannelEditorSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var channel: Channel?

    @State private var name = ""
    @State private var id = ""
    @State private var idTouched = false
    @State private var number = 2
    @State private var group = ""
    @State private var isGuide = false
    @State private var strategy: Strategy = .sequential
    @State private var runOut: RunOut = .keepGoing
    @State private var chaos = 0.0
    @State private var autopilot = true
    @State private var alwaysOn = false
    @State private var libraryIDs: Set<Int> = []
    @State private var saving = false
    @State private var confirmDelete = false
    @State private var loaded = false

    private var isNew: Bool { channel == nil }
    private var numberTaken: Bool { model.channels.contains { $0.number == number && $0.id != id } }
    private var valid: Bool { !name.trimmingCharacters(in: .whitespaces).isEmpty && Format.isValidID(id) && number > 0 && !numberTaken }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section {
                    TextField("Name", text: $name).onChange(of: name) { _, n in if isNew && !idTouched { id = Format.slug(n) } }
                    HStack {
                        TextField("Number", value: $number, format: .number).frame(maxWidth: 140)
                        if numberTaken { Text("In use").foregroundStyle(.red).font(.caption) }
                    }
                    TextField("Group", text: $group, prompt: Text("Kids, Movies, …"))
                    if isNew {
                        TextField("Id", text: $id).font(.body.monospaced())
                        Picker("Kind", selection: $isGuide) { Text("Programming channel").tag(false); Text("Guide channel").tag(true) }
                    }
                }
                if !isGuide {
                    Section("What it airs") {
                        let shows = model.libraries.filter { $0.kind == .shows }
                        if shows.isEmpty { Text("Add a TV shows library first.").foregroundStyle(.secondary) }
                        ForEach(shows) { lib in
                            Toggle("\(lib.name) (\(lib.items) files)", isOn: Binding(get: { libraryIDs.contains(lib.id) }, set: { on in if on { libraryIDs.insert(lib.id) } else { libraryIDs.remove(lib.id) } }))
                        }
                        Picker("Strategy", selection: $strategy) { ForEach(Strategy.allCases) { Text($0.label).tag($0) } }
                        LabeledContent("Chaos \(Int(chaos))%") { Slider(value: $chaos, in: 0...100, step: 1) }
                        Picker("When a show runs out", selection: $runOut) { ForEach(RunOut.allCases) { Text($0.label).tag($0) } }
                    }
                }
                Section("Running it") {
                    Toggle("Autopilot (edits re-roll the future automatically)", isOn: $autopilot)
                    Toggle("Always on (keep encoding with no viewers)", isOn: $alwaysOn)
                }
                if let channel, !isGuide {
                    Section("More") {
                        Button("Edit lineup, seasons and events in the web admin…") { model.openInBrowser("/admin/#/channels/\(channel.id)?tab=programming") }
                        Button("Ads and bumpers in the web admin…") { model.openInBrowser("/admin/#/channels/\(channel.id)?tab=ads") }
                        Button("Re-roll the schedule now") {
                            Task { if let c = model.client { _ = await model.run { try await c.reroll(channel: channel.id) } } }
                        }
                    }
                } else if let channel, isGuide {
                    Section("More") { Button("Design the guide in the web admin…") { model.openInBrowser("/admin/#/channels/\(channel.id)?tab=guide") } }
                }
            }
            .formStyle(.grouped)

            HStack {
                if !isNew { Button("Delete…", role: .destructive) { confirmDelete = true } }
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(isNew ? "Create" : "Save") { Task { await save() } }.glassButton(prominent: true)
                    .keyboardShortcut(.defaultAction).disabled(!valid || saving)
            }
            .padding(16)
        }
        .frame(width: 520, height: 620)
        .confirmationDialog("Delete “\(name)”?", isPresented: $confirmDelete) {
            Button("Delete channel", role: .destructive) { Task { await delete() } }
        } message: { Text("TVs tuned to it will lose the channel. Your files are not touched.") }
        .task {
            guard !loaded else { return }
            loaded = true
            if let c = channel {
                name = c.name; id = c.id; number = c.number; group = c.group ?? ""; isGuide = c.isGuide
                strategy = c.strategy; runOut = c.runOut; chaos = Double(c.chaos); autopilot = c.autopilot; alwaysOn = c.alwaysOn; libraryIDs = Set(c.libraryIds)
            } else {
                number = Format.nextFreeNumber(taken: model.channels.map(\.number))
                libraryIDs = Set(model.libraries.filter { $0.kind == .shows }.map(\.id))
            }
        }
    }

    private func save() async {
        guard let client = model.client else { return }
        saving = true
        var f: [String: JSONValue] = [
            "id": .string(id), "number": .number(Double(number)), "name": .string(name.trimmingCharacters(in: .whitespaces)),
            "autopilot": .bool(autopilot), "always_on": .bool(alwaysOn),
            "kind": .string(isGuide ? "guide" : "programming"),
        ]
        if !group.trimmingCharacters(in: .whitespaces).isEmpty { f["group"] = .string(group) }
        if !isGuide {
            f["strategy"] = .string(strategy.rawValue); f["run_out"] = .string(runOut.rawValue)
            f["chaos"] = .number(chaos); f["library_ids"] = .array(libraryIDs.sorted().map { .number(Double($0)) })
        }
        let ok = await model.run { try await client.saveChannel(f) } != nil
        saving = false
        if ok { await model.refreshChannels(); dismiss() }
    }

    private func delete() async {
        guard let client = model.client else { return }
        if await model.run({ try await client.deleteChannel(id) }) != nil {
            await model.refreshChannels(); dismiss()
        }
    }
}
