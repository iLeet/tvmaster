import SwiftUI
import TVMasterKit

struct ChannelsView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Channel?
    /// Developer aid (screenshots): TVM_EDIT_CHANNEL=<id> opens that channel's editor.
    private var autoEdit: String? { ProcessInfo.processInfo.environment["TVM_EDIT_CHANNEL"] }

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
                                        if ch.isTest { Badge(text: "Test pattern") }
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
        .task { if editing == nil, let id = autoEdit, let ch = model.channels.first(where: { $0.id == id }) { editing = ch } }
        .sheet(item: $editing) { ch in ChannelEditorSheet(channel: ch) }
        .sheet(isPresented: $model.showNewChannel) { ChannelEditorSheet(channel: nil) }
    }

    private func summary(_ ch: Channel) -> String {
        if ch.isGuide { return "\(ch.guide.hours)-hour live guide" }
        if ch.isTest { return "Test pattern" }
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
    @State private var isTest = false
    @State private var strategy: Strategy = .sequential
    @State private var runOut: RunOut = .keepGoing
    @State private var chaos = 0.0
    @State private var autopilot = true
    @State private var alwaysOn = false
    @State private var libraryIDs: Set<Int> = []
    @State private var saving = false
    @State private var confirmDelete = false
    @State private var loaded = false
    @State private var tab: Tab = {
        switch ProcessInfo.processInfo.environment["TVM_EDIT_TAB"] { case "picture": .picture; case "guide": .guide; default: .general }
    }()
    /// The channel's picture and sound settings, read from the server's raw JSON.
    @State private var ps = PictureSoundSettings()
    @State private var psOriginal = PictureSoundSettings()
    @State private var rawOptions: JSONValue?
    @State private var rawLineup: JSONValue?
    @State private var loadingRaw = true
    /// A guide channel's look, read from the server's raw JSON.
    @State private var gs = GuideSettings()
    @State private var gsOriginal = GuideSettings()
    @State private var rawGuide: JSONValue?
    private enum Tab: String, CaseIterable { case general = "General", picture = "Picture & sound", guide = "Guide" }
    private var tabs: [Tab] { isGuide ? [.general, .guide] : isNew ? [.general] : [.general, .picture] }

    private var isNew: Bool { channel == nil }
    private var numberTaken: Bool { model.channels.contains { $0.number == number && $0.id != id } }
    private var valid: Bool { !name.trimmingCharacters(in: .whitespaces).isEmpty && Format.isValidID(id) && number > 0 && !numberTaken && (!isGuide || gs.problem == nil) }

    var body: some View {
        VStack(spacing: 0) {
            if tabs.count > 1 {
                Picker("", selection: $tab) { ForEach(tabs, id: \.self) { Text($0.rawValue).tag($0) } }
                    .pickerStyle(.segmented).labelsHidden().padding([.horizontal, .top], 16)
            }
            if tab == .picture && !isGuide && !isNew {
                PictureSoundForm(ps: $ps, sys: model.system, testOnly: isTest, oldServer: rawOptions == nil && !loadingRaw)
            } else if tab == .guide && isGuide {
                GuideForm(gs: $gs, id: id, name: name, raw: rawGuide)
            } else {
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
                        Picker("Kind", selection: Binding(get: { isGuide ? 2 : isTest ? 1 : 0 }, set: { isGuide = $0 == 2; isTest = $0 == 1; if isTest { ps.offAirScreen = "bars"; ps.offAirTone = true } })) {
                            Text("Programming channel").tag(0); Text("Test pattern").tag(1); Text("Guide channel").tag(2)
                        }
                    }
                }
                if isTest {
                    Section("Test pattern") {
                        Text("A test pattern channel airs only its screen: colour bars and a tone by default. Choose the screen on the Picture & sound tab after creating it.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if !isGuide && !isTest {
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
                } else if isGuide {
                    Section("The guide") {
                        Text("Colours, layout, heading, ticker, a live video window and more are on the Guide tab.").font(.caption).foregroundStyle(.secondary)
                        Button("Open the Guide tab") { tab = .guide }.glassButton()
                    }
                }
            }
            .formStyle(.grouped)
            }

            HStack {
                if !isNew { Button("Delete…", role: .destructive) { confirmDelete = true } }
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(isNew ? "Create" : "Save") { Task { await save() } }.glassButton(prominent: true)
                    .keyboardShortcut(.defaultAction).disabled(!valid || saving)
            }
            .padding(16)
        }
        .frame(width: 560, height: isGuide && tab == .guide ? 780 : 680)
        .confirmationDialog("Delete “\(name)”?", isPresented: $confirmDelete) {
            Button("Delete channel", role: .destructive) { Task { await delete() } }
        } message: { Text("TVs tuned to it will lose the channel. Your files are not touched.") }
        .task {
            guard !loaded else { return }
            loaded = true
            if let c = channel {
                name = c.name; id = c.id; number = c.number; group = c.group ?? ""; isGuide = c.isGuide; isTest = c.isTest
                strategy = c.strategy; runOut = c.runOut; chaos = Double(c.chaos); autopilot = c.autopilot; alwaysOn = c.alwaysOn; libraryIDs = Set(c.libraryIds)
                // Picture and sound live in parts of the channel the typed model does not carry: fetch it raw.
                if let client = model.client, let raw = try? await client.channelRaw(c.id) {
                    if c.isGuide {
                        rawGuide = raw.get(["guide"])
                        gs = GuideSettings(guide: rawGuide)
                        gsOriginal = gs
                    } else {
                        rawOptions = raw.get(["options"]); rawLineup = raw.get(["lineup"])
                        ps = PictureSoundSettings(options: rawOptions, lineup: rawLineup)
                        psOriginal = ps
                    }
                }
                loadingRaw = false
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
            "kind": .string(isGuide ? "guide" : isTest ? "test" : "programming"),
        ]
        if !group.trimmingCharacters(in: .whitespaces).isEmpty { f["group"] = .string(group) }
        if isTest, isNew {
            f["options"] = ps.apply(to: nil)
        }
        if !isGuide && !isTest {
            f["strategy"] = .string(strategy.rawValue); f["run_out"] = .string(runOut.rawValue)
            f["chaos"] = .number(chaos); f["library_ids"] = .array(libraryIDs.sorted().map { .number(Double($0)) })
        }
        // A guide's look, when it was set up or changed.
        if isGuide, isNew || gs != gsOriginal {
            if let p = gs.problem { model.lastError = p; saving = false; return }
            f["guide"] = gs.apply(to: rawGuide)
        }
        // Only touch options and the sign-off hours when the user changed them.
        if !isGuide, !isNew, ps != psOriginal, rawOptions != nil || rawLineup != nil || isTest {
            f["options"] = ps.apply(to: rawOptions)
            f["lineup"] = ps.apply(toLineup: rawLineup)
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


/// Audio, subtitles, picture, graphics and sign-off for one channel.
struct PictureSoundForm: View {
    @Binding var ps: PictureSoundSettings
    var sys: SystemInfo?
    /// A test-pattern channel has only its screen to choose.
    var testOnly = false
    /// The server did not send picture and sound settings: it is older than this app.
    var oldServer = false

    var body: some View {
        Form {
            if oldServer {
                Section {
                    Label("This server doesn't know about picture and sound settings yet. Update the TVMaster server (the bundled one comes with the app update).", systemImage: "exclamationmark.triangle.fill")
                        .font(.callout).foregroundStyle(.orange)
                }
            }
            Section("When nothing is on air") {
                Picker("Screen", selection: $ps.offAirScreen) { ForEach(PictureSoundSettings.screens, id: \.value) { Text($0.label).tag($0.value) } }
                if ps.offAirScreen == "card" { TextField("Message", text: $ps.offAirMessage, prompt: Text("PLEASE STAND BY")) }
                if ps.offAirScreen == "image" { PictureChooser(title: "Picture", path: $ps.offAirImage) }
                Toggle("Test tone (hiss with snow)", isOn: $ps.offAirTone)
            }
            if !testOnly {
            Section("Audio") {
                LanguageList(title: "Preferred languages", empty: "Any (the file's default track)", languages: $ps.audioLanguages)
                Toggle("Skip commentary tracks", isOn: $ps.avoidCommentary)
                Picker("Volume levelling", selection: $ps.loudness) { ForEach(PictureSoundSettings.loudnessModes, id: \.value) { Text($0.label).tag($0.value) } }
                Toggle("Night mode (quieter explosions, clearer speech)", isOn: $ps.nightMode)
                Toggle("Dialogue boost (surround sources)", isOn: $ps.dialogueBoost)
                Toggle("Mono, like an old set", isOn: $ps.mono)
            }
            Section("Subtitles") {
                if let ff = sys?.ffmpeg, ff.hasSubtitles == false {
                    Label("This server's FFmpeg can't draw text subtitles (no libass). Picture subtitles still work.", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption).foregroundStyle(.orange)
                }
                Picker("Show them", selection: $ps.subtitleMode) { ForEach(PictureSoundSettings.subtitleModes, id: \.value) { Text($0.label).tag($0.value) } }
                if ps.subtitleMode != "off" {
                    LanguageList(title: "Subtitle languages", empty: "Same as audio", languages: $ps.subtitleLanguages)
                    Picker("Size", selection: $ps.subtitleSize) { Text("Small").tag("small"); Text("Medium").tag("medium"); Text("Large").tag("large") }
                    Picker("Style", selection: $ps.subtitleStyle) { Text("Outline").tag("outline"); Text("Caption box").tag("box") }
                }
            }
            Section("Picture") {
                Picker("Quality", selection: $ps.quality) { ForEach(PictureSoundSettings.qualities, id: \.value) { Text($0.label).tag($0.value) } }
                Picker("Screen shape", selection: $ps.aspect) { ForEach(PictureSoundSettings.aspects, id: \.value) { Text($0.label).tag($0.value) } }
                Picker("Look", selection: $ps.look) { ForEach(PictureSoundSettings.looks, id: \.value) { Text($0.label).tag($0.value) } }
            }
            Section("On-screen graphics") {
                Toggle("Channel logo in a corner", isOn: $ps.bugEnabled)
                if ps.bugEnabled {
                    TextField("Logo text", text: $ps.bugText, prompt: Text("Channel number and name"))
                    PictureChooser(title: "Logo picture", path: $ps.bugImage, hint: "Optional. Shown instead of the text; a transparent png works best.")
                }
                Toggle("Now-playing card when a programme starts", isOn: $ps.titleEnabled)
                Toggle("Clock (needs FFmpeg with drawtext)", isOn: $ps.clockEnabled)
            }
            Section("Sign-off") {
                Toggle("Go off the air overnight", isOn: $ps.signOffEnabled)
                if ps.signOffEnabled {
                    HStack {
                        TextField("Off at", text: $ps.signOffFrom).frame(width: 90)
                        TextField("Back at", text: $ps.signOffTo).frame(width: 90)
                        Picker("", selection: $ps.signOffStyle) { ForEach(PictureSoundSettings.screens, id: \.value) { Text($0.label).tag($0.value) } }.labelsHidden()
                    }
                    Text("24-hour times like 02:00, in the channel's time zone.").font(.caption).foregroundStyle(.secondary)
                }
            }
            Section {
                Text("Audio and subtitle settings for individual shows (anime in Japanese with English subtitles, a show with subtitles already in the picture, …) are on the Shows page of the web admin.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            }
        }
        .formStyle(.grouped)
    }
}

/// An ordered list of languages: reorder, remove, add from a menu.
struct LanguageList: View {
    var title: String
    var empty: String
    @Binding var languages: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
            if languages.isEmpty { Text(empty).font(.caption).foregroundStyle(.secondary) }
            ForEach(Array(languages.enumerated()), id: \.element) { i, code in
                HStack(spacing: 8) {
                    Text("\(i + 1)").font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 16)
                    Text(Language.name(code))
                    Spacer()
                    Button { languages.swapAt(i, i - 1) } label: { Image(systemName: "chevron.up") }.buttonStyle(.borderless).disabled(i == 0)
                    Button { languages.swapAt(i, i + 1) } label: { Image(systemName: "chevron.down") }.buttonStyle(.borderless).disabled(i == languages.count - 1)
                    Button { languages.remove(at: i) } label: { Image(systemName: "xmark.circle.fill") }.buttonStyle(.borderless)
                }
            }
            Menu(languages.isEmpty ? "Choose a language…" : "Add another…") {
                ForEach(Language.all.filter { !languages.contains($0.code) }) { lang in
                    Button(lang.name) { languages.append(lang.code) }
                }
            }
            .fixedSize()
        }
    }
}
