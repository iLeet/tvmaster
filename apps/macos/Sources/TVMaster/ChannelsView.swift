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
                PageHeader(title: "Channels", subtitle: "Your stations: what they air, when, and how it looks.") {
                    Button { model.showNewChannel = true } label: { Label("New channel", systemImage: "plus") }.glassButton(prominent: true)
                        .keyboardShortcut("n", modifiers: .command)
                }
                if model.channels.isEmpty {
                    Text("No channels yet. Create your first one to put something on air.").foregroundStyle(.secondary)
                }
                ForEach(sections, id: \.title) { section in
                    VStack(alignment: .leading, spacing: 8) {
                        if sections.count > 1 {
                            (Text(section.title).font(.title3.bold()) + Text("  \(section.about)").font(.callout).foregroundStyle(.secondary))
                        }
                        GlassGroup(spacing: 10) {
                            VStack(spacing: 10) {
                                ForEach(section.channels) { ch in
                                    HStack(spacing: 12) {
                                        NumberBadge(number: ch.number)
                                        VStack(alignment: .leading, spacing: 2) {
                                            HStack(spacing: 6) {
                                                Text(ch.name).fontWeight(.semibold)
                                                if ch.isGuide { Badge(text: "Guide") }
                                                if ch.isInfo { Badge(text: "Info") }
                                                if ch.isTest { Badge(text: "Test pattern") }
                                                if ch.kind == .programming { Badge(text: ch.autopilot ? "Autopilot" : "Locked", color: ch.autopilot ? .green : .orange) }
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
                }
            }
            .padding(28)
        }
        .task { if editing == nil, let id = autoEdit, let ch = model.channels.first(where: { $0.id == id }) { editing = ch } }
        .sheet(item: $editing) { ch in ChannelEditorSheet(channel: ch) }
        .sheet(isPresented: $model.showNewChannel) { ChannelEditorSheet(channel: nil) }
    }

    /// TV channels, info channels, the guide and test patterns, each under its own heading.
    private var sections: [(title: String, about: String, channels: [Channel])] {
        let defs: [(kind: ChannelKind, title: String, about: String)] = [
            (.programming, "TV channels", "Shows from your libraries, on a schedule"),
            (.info, "Info channels", "News, notices, what is on tonight, the time and pictures"),
            (.guide, "Guide", "The scrolling listing of every channel"),
            (.test, "Test patterns", ""),
        ]
        return defs
            .map { d in (title: d.title, about: d.about, channels: model.channels.filter { $0.kind == d.kind }) }
            .filter { !$0.channels.isEmpty }
    }

    private func summary(_ ch: Channel) -> String {
        if ch.isGuide { return "\(ch.guide.hours)-hour live guide" }
        if ch.isTest { return "Test pattern" }
        if ch.isInfo { return ch.infoSummary }
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
    @State private var isInfo = false
    /// The kind chosen for a new channel (nil = still choosing).
    @State private var template: ChannelTemplate?
    @State private var templates: ChannelTemplates?
    /// An info channel's pages, read from the server's raw JSON.
    @State private var info = InfoSettings()
    @State private var infoOriginal = InfoSettings()
    @State private var rawInfo: JSONValue?
    /// A new TV channel is made with the guide unless the full editor is asked for.
    @State private var fullEditor = false
    @State private var strategy: Strategy = .sequential
    @State private var runOut: RunOut = .keepGoing
    @State private var chaos = 0.0
    @State private var autopilot = true
    @State private var alwaysOn = false
    @State private var libraryIDs: Set<Int> = []
    @State private var saving = false
    @State private var confirmDelete = false
    @State private var confirmProgressReset = false
    @State private var loaded = false
    @State private var tab: Tab = {
        switch ProcessInfo.processInfo.environment["TVM_EDIT_TAB"] { case "picture": .picture; case "guide": .guide; case "programming": .programming; case "info": .info; case "breaks": .breaks; default: .general }
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
    /// The channel's time slots, specials, ads and so on, read from the server's raw JSON.
    @State private var ls = LineupSettings()
    @State private var lsOriginal = LineupSettings()
    @State private var showNames: [String] = []
    @State private var adPresets: [AdPreset] = []
    private enum Tab: String, CaseIterable { case general = "General", programming = "Programming", picture = "Picture & sound", breaks = "Breaks", guide = "Guide", info = "Pages" }
    private var tabs: [Tab] { isGuide ? [.general, .guide] : isInfo ? [.general, .info] : (isNew || isTest) ? [.general] : [.general, .programming, .picture, .breaks] }

    private var isNew: Bool { channel == nil }
    private var numberTaken: Bool { model.channels.contains { $0.number == number && $0.id != id } }
    private var valid: Bool { !name.trimmingCharacters(in: .whitespaces).isEmpty && Format.isValidID(id) && number > 0 && !numberTaken && (!isGuide || gs.problem == nil) && (!isInfo || info.problem == nil) }

    var body: some View {
        if isNew && template == nil && templates != nil {
            VStack(spacing: 0) {
                ChannelTemplateChooser(templates: templates!) { pick($0) }
                HStack { Spacer(); Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction) }.padding(16)
            }
            .frame(width: 600, height: 680)
        } else if isNew, template?.id == "tv", !fullEditor {
            ChannelGuideView(onFullEditor: { fullEditor = true }, onOtherKind: { template = nil })
                .frame(width: 640, height: 700)
        } else {
            editor
        }
    }

    /// Start a new channel from the kind that was chosen: its name, number and the settings that make it work.
    private func pick(_ t: ChannelTemplate) {
        template = t
        name = t.suggestedName; id = t.suggestedId; idTouched = true
        number = Format.nextFreeNumber(taken: model.channels.map(\.number))
        if t.suggestedNumber > 0, !model.channels.contains(where: { $0.number == t.suggestedNumber }) { number = t.suggestedNumber }
        isGuide = t.kind == "guide"; isTest = t.kind == "test"; isInfo = t.kind == "info"
        if isTest { ps.offAirScreen = "bars"; ps.offAirTone = true }
        if isInfo { rawInfo = t.body.get(["info"]); info = InfoSettings(info: rawInfo); infoOriginal = InfoSettings() }
        tab = .general
    }

    private var editor: some View {
        VStack(spacing: 0) {
            if tabs.count > 1 {
                Picker("", selection: $tab) { ForEach(tabs, id: \.self) { Text($0.rawValue).tag($0) } }
                    .pickerStyle(.segmented).labelsHidden().padding([.horizontal, .top], 16)
            }
            if tab == .info && isInfo {
                InfoForm(info: $info, raw: rawInfo, id: isNew ? nil : id, name: name, feedSuggestions: templates?.feeds ?? [])
            } else if tab == .programming && !isGuide && !isNew && !isTest && !isInfo {
                if rawLineup == nil && !loadingRaw {
                    Text("This server did not send the channel's lineup. Update the TVMaster server.").foregroundStyle(.secondary).padding(30)
                } else {
                    ProgrammingForm(ls: $ls, strategy: $strategy, runOut: $runOut, chaos: $chaos, ps: $ps, shows: showNames, libraries: model.libraries,
                                    breaksSummary: (ls.ads?.breaksLabel(in: adPresets) ?? "No commercial breaks"), onBreaks: { tab = .breaks })
                }
            } else if tab == .breaks && !isGuide && !isNew && !isTest && !isInfo {
                BreaksForm(ls: $ls, libraries: model.libraries, theme: ps.theme)
            } else if tab == .picture && !isGuide && !isNew && !isInfo {
                PictureSoundForm(ps: $ps, sys: model.system, testOnly: isTest, oldServer: rawOptions == nil && !loadingRaw, channelID: id, channelName: name, channelNumber: number)
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
                        if let t = template {
                            HStack {
                                Badge(text: t.name)
                                Text(t.about).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                Spacer()
                                Button("Choose a different kind") { template = nil }.glassButton()
                            }
                            if !t.needs.isEmpty {
                                let words = ["libraries": "pick the libraries it draws from below", "feeds": "add a news feed on the Pages tab", "pages": "add your pages on the Pages tab", "music": "choose some music on the Pages tab"]
                                Text("Before it is worth airing: " + t.needs.map { words[$0] ?? $0 }.joined(separator: "; ") + ".").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if isInfo {
                    Section("Info channel") {
                        Text("An info channel has nothing to schedule: it draws its pages itself, round and round. Add and arrange them on the Pages tab. It appears on the guide, with what it is.")
                            .font(.caption).foregroundStyle(.secondary)
                        Button("Open the Pages tab") { tab = .info }.glassButton()
                    }
                    Section("Look") {
                        Toggle("Follow the station theme", isOn: Binding(get: { ps.theme == nil }, set: { ps.theme = $0 ? nil : ThemeChoice() }))
                        if ps.theme != nil {
                            let t = Binding(get: { ps.theme ?? ThemeChoice() }, set: { ps.theme = $0 })
                            ThemeChoiceEditorView(choice: t, autoName: "Per channel", autoAbout: "Colours made from this channel.")
                        } else {
                            Text("The pages use the look chosen under Appearance (and any day part's look).").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                if isTest {
                    Section("Test pattern") {
                        Text("A test pattern channel airs only its screen: colour bars and a tone by default. Choose the screen on the Picture & sound tab after creating it.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if !isGuide && !isTest && !isInfo {
                    Section("What it airs") {
                        let shows = model.libraries.filter { $0.kind == .shows }
                        if shows.isEmpty { Text("Add a TV shows library first.").foregroundStyle(.secondary) }
                        ForEach(shows) { lib in
                            Toggle("\(lib.name) (\(lib.items) files)", isOn: Binding(get: { libraryIDs.contains(lib.id) }, set: { on in if on { libraryIDs.insert(lib.id) } else { libraryIDs.remove(lib.id) } }))
                        }
                        if isNew {
                            Picker("Style", selection: $strategy) { ForEach(Strategy.allCases) { Text($0.label).tag($0) } }
                        } else {
                            HStack(alignment: .top) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text("How it plays").font(.headline)
                                    Text(ls.summary(strategy: strategy, runOut: runOut, breaks: (ls.ads?.breaksLabel(in: adPresets) ?? "No commercial breaks")))
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("Programming…") { tab = .programming }.glassButton()
                            }
                        }
                    }
                }
                if !isGuide && !isTest && !isNew && !isInfo {
                    Section("Controls") {
                        Text("Act on this channel right now. None of these change your settings.").font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Restart the stream") { Task { _ = await model.run { try await model.client!.stream(channel: id, action: "restart") } } }.glassButton()
                            Button("Take off air") { Task { _ = await model.run { try await model.client!.stream(channel: id, action: "stop") } } }.glassButton()
                            Button("Refresh now") { Task { _ = await model.run { try await model.client!.refresh(channel: id) } } }.glassButton()
                                .help("Put the channel on the plan for this moment, even part way through a show")
                            Button("Re-plan the schedule") { Task { _ = await model.run { try await model.client!.reset(channel: id, what: "schedule") } } }.glassButton()
                        }
                        Button("Start every show from episode 1…", role: .destructive) { confirmProgressReset = true }.glassButton()
                            .confirmationDialog("Start every show from episode 1?", isPresented: $confirmProgressReset) {
                                Button("Start over", role: .destructive) { Task { _ = await model.run { try await model.client!.reset(channel: id, what: "progress") } } }
                            } message: { Text("Every show on this channel starts again at its first episode, and the schedule is re-planned from now.") }
                    }
                }
                Section("Running it") {
                    if !isInfo { Toggle("Autopilot (edits re-roll the future automatically)", isOn: $autopilot) }
                    Toggle("Always on (keep encoding with no viewers)", isOn: $alwaysOn)
                }
                if let channel, !isGuide, !isInfo {
                    Section("More") {
                        Button("Programming: the week, gaps, specials and sign-off") { tab = .programming }
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
        .frame(width: 600, height: (isGuide && tab == .guide) || tab == .programming || tab == .info ? 780 : 680)
        .confirmationDialog("Delete “\(name)”?", isPresented: $confirmDelete) {
            Button("Delete channel", role: .destructive) { Task { await delete() } }
        } message: { Text("TVs tuned to it will lose the channel. Your files are not touched.") }
        .task {
            guard !loaded else { return }
            loaded = true
            if let c = channel {
                name = c.name; id = c.id; number = c.number; group = c.group ?? ""; isGuide = c.isGuide; isTest = c.isTest; isInfo = c.isInfo
                strategy = c.strategy; runOut = c.runOut; chaos = Double(c.chaos); autopilot = c.autopilot; alwaysOn = c.alwaysOn; libraryIDs = Set(c.libraryIds)
                // Picture and sound live in parts of the channel the typed model does not carry: fetch it raw.
                if let client = model.client, let raw = try? await client.channelRaw(c.id) {
                    if c.isGuide {
                        rawGuide = raw.get(["guide"])
                        gs = GuideSettings(guide: rawGuide)
                        gsOriginal = gs
                    } else {
                        rawInfo = raw.get(["info"])
                        if c.isInfo { info = InfoSettings(info: rawInfo); infoOriginal = info }
                        rawOptions = raw.get(["options"]); rawLineup = raw.get(["lineup"])
                        ps = PictureSoundSettings(options: rawOptions, lineup: rawLineup)
                        psOriginal = ps
                        ls = LineupSettings(lineup: rawLineup)
                        lsOriginal = ls
                        showNames = ((try? await client.shows()) ?? []).map(\.name).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
                        adPresets = (try? await client.adOptions().presets) ?? []
                    }
                }
                loadingRaw = false
            } else {
                number = Format.nextFreeNumber(taken: model.channels.map(\.number))
                libraryIDs = Set(model.libraries.filter { $0.kind == .shows }.map(\.id))
                templates = try? await model.client?.channelTemplates()
                if templates == nil || (templates?.templates.isEmpty ?? true) {
                    // An older server has no creator: fall back to a plain TV channel.
                    templates = nil
                }
            }
        }
    }

    private func save() async {
        guard let client = model.client else { return }
        saving = true
        var f: [String: JSONValue] = [
            "id": .string(id), "number": .number(Double(number)), "name": .string(name.trimmingCharacters(in: .whitespaces)),
            "autopilot": .bool(autopilot), "always_on": .bool(alwaysOn),
            "kind": .string(isGuide ? "guide" : isTest ? "test" : isInfo ? "info" : "programming"),
        ]
        if !group.trimmingCharacters(in: .whitespaces).isEmpty { f["group"] = .string(group) }
        if isTest, isNew {
            f["options"] = ps.apply(to: nil)
        }
        if isInfo, isNew || info != infoOriginal {
            if let p = info.problem { model.lastError = p; saving = false; return }
            f["info"] = info.apply(to: rawInfo)
        }
        if isInfo, ps != psOriginal || (isNew && ps.theme != nil) {
            f["options"] = ps.apply(to: rawOptions)
        }
        if !isGuide && !isTest && !isInfo {
            f["strategy"] = .string(strategy.rawValue); f["run_out"] = .string(runOut.rawValue)
            f["chaos"] = .number(chaos); f["library_ids"] = .array(libraryIDs.sorted().map { .number(Double($0)) })
        }
        // A guide's look, when it was set up or changed.
        if isGuide, isNew || gs != gsOriginal {
            if let p = gs.problem { model.lastError = p; saving = false; return }
            f["guide"] = gs.apply(to: rawGuide)
        }
        // Only touch options and the sign-off hours when the user changed them.
        if !isGuide, !isNew, !isInfo, ps != psOriginal, rawOptions != nil || rawLineup != nil || isTest {
            f["options"] = ps.apply(to: rawOptions)
            f["lineup"] = ps.apply(toLineup: rawLineup)
        }
        // Time slots, specials, ads and timing, when they were changed.
        if !isGuide, !isTest, !isNew, !isInfo, ls != lsOriginal, rawLineup != nil {
            if let p = ls.problem { model.lastError = p; saving = false; return }
            f["lineup"] = ls.apply(to: f["lineup"] ?? rawLineup)
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
    var channelID: String?
    var channelName: String?
    var channelNumber: Int?

    var body: some View {
        Form {
            if oldServer {
                Section {
                    Label("This server doesn't know about picture and sound settings yet. Update the TVMaster server (the bundled one comes with the app update).", systemImage: "exclamationmark.triangle.fill")
                        .font(.callout).foregroundStyle(.orange)
                }
            }
            Section {
                Button("Reset picture and sound to defaults", role: .destructive) {
                    // The off-air screen and sign-off hours live on the Programming tab: keep them.
                    var n = PictureSoundSettings()
                    n.offAirScreen = ps.offAirScreen; n.offAirMessage = ps.offAirMessage; n.offAirImage = ps.offAirImage; n.offAirTone = ps.offAirTone
                    n.signOffEnabled = ps.signOffEnabled; n.signOffFrom = ps.signOffFrom; n.signOffTo = ps.signOffTo; n.signOffStyle = ps.signOffStyle
                    ps = n
                }.glassButton()
                Text("Audio, subtitles, picture and graphics go back to their defaults when you save. Per-show settings are kept.").font(.caption).foregroundStyle(.secondary)
            }
            if testOnly {
                Section("What this channel shows") {
                    Text("A test pattern channel airs only its screen, all the time. (To make an ordinary channel go off the air at night, use its Programming tab.)").font(.caption).foregroundStyle(.secondary)
                    Picker("Screen", selection: $ps.offAirScreen) { ForEach(PictureSoundSettings.screens, id: \.value) { Text($0.label).tag($0.value) } }
                    if ps.offAirScreen == "card" { TextField("Message", text: $ps.offAirMessage, prompt: Text("PLEASE STAND BY")) }
                    if ps.offAirScreen == "image" { PictureChooser(title: "Picture", path: $ps.offAirImage) }
                    Toggle("Test tone (hiss with snow)", isOn: $ps.offAirTone)
                }
            }
            if !testOnly {
            Section("The basics") {
                Text("The three things most people change. Everything else is under More options below.").font(.caption).foregroundStyle(.secondary)
                Picker("Picture quality", selection: $ps.quality) { ForEach(PictureSoundSettings.qualities, id: \.value) { Text($0.label).tag($0.value) } }
                Text("Lower sizes suit old TVs, slow networks and small servers. 720p is a good all-round choice.").font(.caption).foregroundStyle(.secondary)
                if let ff = sys?.ffmpeg, ff.hasSubtitles == false, ps.subtitleMode != "off" {
                    Label("This server's FFmpeg can't draw text subtitles (no libass). Picture subtitles still work.", systemImage: "exclamationmark.triangle.fill")
                        .font(.caption).foregroundStyle(.orange)
                }
                Picker("Subtitles", selection: $ps.subtitleMode) { ForEach(PictureSoundSettings.subtitleModes, id: \.value) { Text($0.label).tag($0.value) } }
                Picker("Even out loud and quiet", selection: $ps.loudness) { ForEach(PictureSoundSettings.loudnessModes, id: \.value) { Text($0.label).tag($0.value) } }
            }
            Section("On-screen extras") {
                Text("The little touches that make it feel like a station.").font(.caption).foregroundStyle(.secondary)
                Toggle("Channel logo in a corner", isOn: $ps.bugEnabled)
                if ps.bugEnabled {
                    TextField("Logo text", text: $ps.bugText, prompt: Text("Channel number and name"))
                    PictureChooser(title: "Logo picture", path: $ps.bugImage, hint: "Optional. Shown instead of the text; a transparent png works best.")
                }
                Toggle("Now-playing card when a programme starts", isOn: $ps.titleEnabled)
                Toggle("Clock (needs FFmpeg with drawtext)", isOn: $ps.clockEnabled)
            }
            Section("Ticker") {
                Text("A strip of scrolling text across the picture, like a news channel's crawl: station notices, headlines from a news feed, and what is on next.").font(.caption).foregroundStyle(.secondary)
                Toggle("Show a ticker", isOn: $ps.tickerEnabled)
                if ps.tickerEnabled {
                    Text("Messages, one per line").font(.caption).foregroundStyle(.secondary)
                    TextEditor(text: Binding(get: { ps.tickerLines.joined(separator: "\n") }, set: { ps.tickerLines = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(20)) }))
                        .frame(height: 80).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                    Toggle("Add what is on next (“Up next: Cheers at 8:30 PM”)", isOn: $ps.tickerUpNext)
                    Text("News feeds: RSS or Atom addresses, one per line. Their latest headlines are added and refreshed every 15 minutes.").font(.caption).foregroundStyle(.secondary)
                    TextEditor(text: Binding(get: { ps.tickerFeeds.joined(separator: "\n") }, set: { ps.tickerFeeds = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(5)) }))
                        .font(.body.monospaced()).frame(height: 50).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                    TextField("Tab (a word at the left, blank = none)", text: $ps.tickerLabel).frame(maxWidth: 320)
                    Picker("Where", selection: $ps.tickerPosition) { Text("Bottom").tag("bottom"); Text("Top").tag("top") }.pickerStyle(.segmented)
                    Picker("Speed", selection: $ps.tickerSpeed) { Text("Slow").tag("slow"); Text("Normal").tag("normal"); Text("Fast").tag("fast") }.pickerStyle(.segmented)
                    Stepper(ps.tickerEvery == 0 ? "On all the time" : "Appears every \(ps.tickerEvery) minutes", value: $ps.tickerEvery, in: 0...60, step: 5)
                    ThemePreviewView(choice: ps.theme ?? ThemeChoice(), what: "ticker", text: ([ps.tickerLines.joined(separator: "   •   ")] + (ps.tickerUpNext ? ["Up next: Cheers at 8:30 PM"] : [])).filter { !$0.isEmpty }.joined(separator: "   •   "), label: ps.tickerLabel, top: ps.tickerPosition == "top")
                }
            }
            Section {
                MoreOptions(summary: "languages, subtitle style, frame rate, screen shape, look, this channel's colours") {
                    Text("Audio").font(.headline)
                LanguageList(title: "Preferred languages", empty: "Any (the file's default track)", languages: $ps.audioLanguages)
                Toggle("Skip commentary tracks", isOn: $ps.avoidCommentary)
                Toggle("Skip audio description (the narrated-picture tracks)", isOn: $ps.avoidDescription)
                Toggle("Night mode (quieter explosions, clearer speech)", isOn: $ps.nightMode)
                Toggle("Dialogue boost (surround sources)", isOn: $ps.dialogueBoost)
                Toggle("Mono, like an old set", isOn: $ps.mono)
                    Text("Subtitles").font(.headline)
                    if ps.subtitleMode == "off" {
                        Text("Subtitles are off. Choose when to show them under The basics.").font(.caption).foregroundStyle(.secondary)
                    } else {
                    LanguageList(title: "Subtitle languages", empty: "Same as audio", languages: $ps.subtitleLanguages)
                    Picker("Size", selection: $ps.subtitleSize) { Text("Small").tag("small"); Text("Medium").tag("medium"); Text("Large").tag("large") }
                    Picker("Style", selection: $ps.subtitleStyle) { Text("Outline").tag("outline"); Text("Caption box").tag("box") }
                    }
                    Text("Picture").font(.headline)
                Picker("Screen shape", selection: $ps.aspect) { ForEach(PictureSoundSettings.aspects, id: \.value) { Text($0.label).tag($0.value) } }
                Picker("Look", selection: $ps.look) { ForEach(PictureSoundSettings.looks, id: \.value) { Text($0.label).tag($0.value) } }
                TextField("Frame rate (blank = 30)", value: $ps.fps, format: .number.grouping(.never)).frame(maxWidth: 260)
                TextField("Bitrate in kbit/s (blank = the quality's default)", value: $ps.kbps, format: .number.grouping(.never)).frame(maxWidth: 360)
                    Text("This channel's colours").font(.headline)
                Toggle("Follow the station theme", isOn: Binding(get: { ps.theme == nil }, set: { ps.theme = $0 ? nil : ThemeChoice() }))
                if ps.theme != nil {
                    let t = Binding(get: { ps.theme ?? ThemeChoice() }, set: { ps.theme = $0 })
                    ThemeChoiceEditorView(choice: t, autoName: "Per channel", autoAbout: "Colours made from this channel.")
                    ThemePreviewView(choice: ps.theme ?? ThemeChoice(), what: "card", id: channelID, name: channelName, number: channelNumber)
                } else {
                    Text("Cards, break screens, the stand-by screen, alerts and on-screen graphics use the look chosen under Appearance.").font(.caption).foregroundStyle(.secondary)
                }
            Section {
                Text("Audio and subtitle settings for individual shows (anime in Japanese with English subtitles, a show with subtitles already in the picture, …) are on the Shows page of the web admin.")
                    .font(.caption).foregroundStyle(.secondary)
            }
                }
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


/// The first step of a new channel: what kind is it? Each choice starts the editor with a sensible name, number and settings.
struct ChannelTemplateChooser: View {
    let templates: ChannelTemplates
    var onPick: (ChannelTemplate) -> Void
    private let about = [
        "TV channels": "Plays shows from your libraries on a schedule.",
        "Guide": "Lists every channel, TV and info, with what is on now and next.",
        "Info channels": "Nothing to schedule: pages that draw themselves.",
        "Test patterns": "Colour bars, snow, a card or a picture, all day.",
    ]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                PageHeader(title: "New channel", subtitle: "What kind of channel is it?")
                ForEach(templates.groups, id: \.self) { g in
                    VStack(alignment: .leading, spacing: 8) {
                        (Text(g).font(.title3.bold()) + Text("  \(about[g] ?? "")").font(.callout).foregroundStyle(.secondary))
                        ForEach(templates.templates.filter { $0.group == g }) { t in
                            Button { onPick(t) } label: {
                                HStack(spacing: 12) {
                                    NumberBadge(number: t.suggestedNumber)
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(t.name).fontWeight(.semibold)
                                        Text(t.about).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.leading)
                                    }
                                    Spacer()
                                    Image(systemName: "chevron.right").foregroundStyle(.secondary)
                                }
                                .padding(12).contentShape(Rectangle())
                                .foregroundStyle(.primary)
                            }
                            .buttonStyle(.plain).glassCard(cornerRadius: 16)
                        }
                    }
                }
            }
            .padding(24)
        }
    }
}
