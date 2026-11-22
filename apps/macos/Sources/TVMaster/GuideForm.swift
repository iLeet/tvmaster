import SwiftUI
import TVMasterKit

/// Everything about how a guide channel looks, with a live picture of the result.
struct GuideForm: View {
    @Environment(AppModel.self) private var model
    @Binding var gs: GuideSettings
    var id: String
    var name: String
    /// The channel's `guide` JSON as the server wrote it (so nothing the editor does not show is lost).
    var raw: JSONValue?

    @State private var preview: NSImage?
    @State private var previewError: String?

    private var programming: [Channel] { model.channels.filter { $0.kind == .programming || $0.kind == .info }.sorted { $0.number < $1.number } }
    private var showable: [Channel] { model.channels.filter { !$0.isGuide }.sorted { $0.number < $1.number } }

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Rectangle().fill(.black)
                if let preview { Image(nsImage: preview).resizable().interpolation(.none).scaledToFit() }
                else if previewError == nil { ProgressView().controlSize(.small) }
            }
            .aspectRatio(16 / 9, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .padding([.horizontal, .top], 16)
            if let previewError { Text(previewError).font(.caption).foregroundStyle(.orange).padding(.horizontal, 16).padding(.top, 4) }
            Form {
                presets
                colours
                style
                listing
                heading
                ticker
                videoWindow
                music
            }
            .formStyle(.grouped)
        }
        .task(id: gs) { await refreshPreview() }
    }

    // MARK: sections

    private var presets: some View {
        Section("Start from a look") {
            Button("Reset the look to the original", role: .destructive) {
                var fresh = GuideSettings()
                fresh.title = gs.title; fresh.ticker = gs.ticker; fresh.channels = gs.channels; fresh.musicLibraryIds = gs.musicLibraryIds
                gs = fresh
            }.glassButton()
            Text("One click sets the colours and style together; everything below still works afterwards.").font(.caption).foregroundStyle(.secondary)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], spacing: 8) {
                ForEach(GuideSettings.presets) { p in
                    Button { p.apply(&gs) } label: {
                        Text(p.label).frame(maxWidth: .infinity)
                    }
                    .glassButton().help(p.help)
                }
            }
        }
    }

    private var style: some View {
        Section("Style") {
            Picker("Letters", selection: Binding(get: { gs.typeface }, set: { v in gs.typeface = v; if v == "smooth" { gs.scanlines = false } })) {
                Text("Retro pixels").tag("pixel"); Text("Smooth").tag("smooth")
            }.pickerStyle(.segmented)
            Text(gs.typeface == "pixel" ? "Chunky pixel letters, drawn small and scaled up: the retro look." : "A smooth modern typeface, drawn at full 1280x720 resolution.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("Corners", selection: $gs.corners) { Text("Square").tag("square"); Text("Rounded").tag("rounded") }.pickerStyle(.segmented)
            Picker("Text size", selection: $gs.textSize) { Text("Small").tag("small"); Text("Medium").tag("medium"); Text("Large").tag("large") }.pickerStyle(.segmented)
            Picker("Heading", selection: $gs.headerStyle) { Text("Coloured bar").tag("bar"); Text("Just the words").tag("minimal") }.pickerStyle(.segmented)
            Picker("Background", selection: $gs.backgroundStyle) { Text("Gradient").tag("gradient"); Text("Solid").tag("solid"); Text("Stripes").tag("stripes"); Text("Soft colour").tag("aurora"); Text("Picture").tag("image") }.pickerStyle(.segmented)
            if gs.backgroundStyle == "image" {
                PictureChooser(title: "Picture", path: $gs.backgroundImage, hint: "Fitted to the screen and darkened so the listing stays readable.")
                LabeledContent("Darken the picture \(gs.backgroundDim)%") { Slider(value: Binding(get: { Double(gs.backgroundDim) }, set: { gs.backgroundDim = Int($0) }), in: 0...90, step: 1) }
            }
            Toggle("Times on each programme (when there is room)", isOn: $gs.cellTimes)
            Toggle("Line at “now” on the time grid", isOn: $gs.nowMarker)
            Toggle("Frosted glass (see-through cells, names and cards with a bright edge; best on the Soft colour background)", isOn: Binding(get: { gs.glass }, set: { on in gs.glass = on; if on { gs.typeface = "smooth"; gs.corners = "rounded" } }))
        }
    }

    private var colours: some View {
        Section("Colours") {
            ThemePickerView(theme: $gs.theme, primary: $gs.colorPrimary, accent: $gs.colorAccent, background: $gs.colorBackground,
                            autoName: "Follow the station",
                            autoAbout: "Uses the station theme (Appearance), so the guide matches your cards, bumpers and alerts. Pick a theme here to give this guide its own colours.")
            Toggle("Scanlines (a faint CRT texture)", isOn: $gs.scanlines)
        }
    }

    private var listing: some View {
        Section("Listing") {
            Picker("Style", selection: $gs.style) { Text("Time grid").tag("grid"); Text("Now & next").tag("now_next") }.pickerStyle(.segmented)
            Text(gs.style == "grid" ? "Channels down the side, programmes across a time line." : "One big row per channel: what is on now and what is next. Easy to read from the couch.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("Channels on screen", selection: $gs.rows) { ForEach(3...10, id: \.self) { Text("\($0)").tag($0) } }
            Picker("Hours of schedule", selection: $gs.hours) { ForEach(2...6, id: \.self) { Text("\($0) hours").tag($0) } }
            if gs.style == "grid" {
                Picker("Time marks", selection: $gs.timeStep) { Text("15 min").tag(15); Text("30 min").tag(30); Text("Hourly").tag(60) }.pickerStyle(.segmented)
            }
            Picker("When there are more channels than fit", selection: $gs.scroll) { Text("Page by page").tag("page"); Text("Smooth roll").tag("smooth") }.pickerStyle(.segmented)
            if gs.scroll == "page" {
                LabeledContent("Each page stays \(gs.pageSeconds) s") { Slider(value: Binding(get: { Double(gs.pageSeconds) }, set: { gs.pageSeconds = Int($0) }), in: 4...30, step: 1) }
            }
            Toggle("Channel numbers", isOn: $gs.channelNumbers)
            Toggle("Channel names", isOn: $gs.channelNames)
            Toggle("Progress bar on what is airing", isOn: $gs.progress)
            DisclosureGroup("Which channels (\(gs.channels.isEmpty ? "all" : "\(gs.channels.count) chosen"))") {
                Text(gs.channels.isEmpty ? "Every TV and info channel, by number. Tick some to list only those, in the order you tick them." : "Only these, in the order you ticked them.")
                    .font(.caption).foregroundStyle(.secondary)
                ForEach(programming) { c in
                    let pos = gs.channels.firstIndex(of: c.id)
                    Toggle(pos.map { "\($0 + 1). \(c.number) \(c.name)" } ?? "\(c.number) \(c.name)", isOn: Binding(get: { pos != nil }, set: { _ in gs.channels = GuideSettings.toggled(gs.channels, c.id) }))
                }
                if !gs.channels.isEmpty { Button("Show every channel again") { gs.channels = [] }.glassButton() }
            }
        }
    }

    private var heading: some View {
        Section("Heading") {
            TextField("Heading", text: $gs.title, prompt: Text("\(name.isEmpty ? "Channel" : name) GUIDE"))
            Toggle("Heading text", isOn: $gs.headerTitle)
            Toggle("Clock", isOn: $gs.headerClock)
            Toggle("Date", isOn: $gs.headerDate)
            Picker("Times", selection: $gs.hour12) { Text("12-hour (9:30 PM)").tag(true); Text("24-hour (21:30)").tag(false) }.pickerStyle(.segmented)
            PictureChooser(title: "Logo", path: $gs.logo, hint: "Optional. A picture at the left of the heading; a transparent png works best.")
        }
    }

    private var ticker: some View {
        Section("Ticker") {
            Toggle("Scrolling line along the bottom", isOn: $gs.tickerEnabled)
            if gs.tickerEnabled {
                Picker("Where", selection: $gs.tickerPosition) { Text("Bottom").tag("bottom"); Text("Top").tag("top") }.pickerStyle(.segmented)
                Picker("Speed", selection: $gs.tickerSpeed) { Text("Slow").tag("slow"); Text("Normal").tag("normal"); Text("Fast").tag("fast") }.pickerStyle(.segmented)
                Toggle("Announce what is on now (“NOW ON 4: Cheers”)", isOn: $gs.tickerNowOn)
                VStack(alignment: .leading, spacing: 4) {
                    Text("Your lines (one per line)").font(.caption).foregroundStyle(.secondary)
                    TextEditor(text: Binding(get: { gs.ticker.joined(separator: "\n") }, set: { gs.ticker = $0.components(separatedBy: "\n") }))
                        .font(.body).frame(height: 70).scrollContentBackground(.hidden)
                        .padding(4).background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 6))
                }
            }
        }
    }

    private var videoWindow: some View {
        Section("Video window") {
            Text("A live picture-in-picture of one of your channels inside the guide, with a card saying what is on. The guide starts that channel while it is showing it.")
                .font(.caption).foregroundStyle(.secondary)
            Picker("Show", selection: $gs.pipMode) { Text("Nothing").tag("off"); Text("One channel").tag("channel"); Text("Flip through channels").tag("rotate"); Text("Commercials only").tag("commercials"); Text("Channels and commercials").tag("mix") }.pickerStyle(.segmented)
            if gs.pipMode == "channel" {
                Picker("Channel", selection: $gs.pipChannel) {
                    Text("Choose…").tag("")
                    ForEach(showable) { Text("\($0.number) \($0.name)").tag($0.id) }
                }
            }
            if gs.pipMode == "commercials" || gs.pipMode == "mix" {
                let ads = model.libraries.filter { $0.kind == .commercials }
                Text((gs.pipMode == "commercials" ? "The window plays your commercials one after another, in random order, while the guide runs." : "The window takes turns: commercials, then a channel, then commercials, then the next channel.") + " The commercials come from a small, light stream of their own that the guide starts while it needs it: it is not a channel, has no number and is listed nowhere.")
                    .font(.caption).foregroundStyle(.secondary)
                if ads.isEmpty {
                    Label("There are no commercials yet. Add a commercials library under Libraries, or make or download some under Promos.", systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange)
                } else {
                    DisclosureGroup("Which commercials (\(gs.pipLibraries.isEmpty ? "every library" : "\(gs.pipLibraries.count) chosen"))") {
                        ForEach(ads) { lib in
                            Toggle("\(lib.name) (\(lib.items) files)", isOn: Binding(get: { gs.pipLibraries.contains(lib.id) }, set: { on in
                                if on { gs.pipLibraries.append(lib.id) } else { gs.pipLibraries.removeAll { $0 == lib.id } }
                            }))
                        }
                    }
                }
                if gs.pipMode == "mix" {
                    LabeledContent("Commercials for \(gs.commercialSeconds) s at a time") { Slider(value: Binding(get: { Double(gs.commercialSeconds) }, set: { gs.commercialSeconds = Int($0) }), in: 10...300, step: 5) }
                }
                Text("The card beside the window").fontWeight(.semibold)
                Text("Say anything you like while the commercials play: station promos, a sponsor, what is coming up. Lines turn over two at a time, so a longer list rotates. {clock} and {date} are filled in. Leave them blank for the station's own words.")
                    .font(.caption).foregroundStyle(.secondary)
                TextField("Heading", text: $gs.adTitle, prompt: Text("AD BREAK"))
                Text("Lines, one per line").font(.caption).foregroundStyle(.secondary)
                TextEditor(text: Binding(get: { gs.adLines.joined(separator: "\n") }, set: { gs.adLines = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(40)) }))
                    .frame(height: 100).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                LabeledContent("Each pair of lines stays \(gs.adRotateSeconds) s") { Slider(value: Binding(get: { Double(gs.adRotateSeconds) }, set: { gs.adRotateSeconds = Int($0) }), in: 2...60, step: 1) }
            }
            if gs.pipMode == "rotate" || gs.pipMode == "mix" {
                LabeledContent("Each channel for \(gs.pipSeconds) s") { Slider(value: Binding(get: { Double(gs.pipSeconds) }, set: { gs.pipSeconds = Int($0) }), in: 10...300, step: 5) }
                DisclosureGroup("Which channels (\(gs.pipChannels.isEmpty ? "every one in the guide" : "\(gs.pipChannels.count) chosen"))") {
                    ForEach(showable) { c in
                        Toggle("\(c.number) \(c.name)", isOn: Binding(get: { gs.pipChannels.contains(c.id) }, set: { _ in gs.pipChannels = GuideSettings.toggled(gs.pipChannels, c.id) }))
                    }
                }
            }
            if gs.pipMode != "off" {
                Picker("Where", selection: $gs.pipPlacement) { ForEach(GuideSettings.placements, id: \.value) { Text($0.label).tag($0.value) } }
                Picker("Size", selection: $gs.pipSize) { Text("Small").tag("small"); Text("Medium").tag("medium"); Text("Large").tag("large") }.pickerStyle(.segmented)
                Picker("Sound", selection: $gs.pipAudio) { Text("Guide music").tag("music"); Text("The channel").tag("channel") }.pickerStyle(.segmented)
                Toggle("A card beside the window saying what is on", isOn: $gs.pipInfo)
            }
            if let p = gs.problem { Label(p, systemImage: "exclamationmark.triangle.fill").font(.caption).foregroundStyle(.orange) }
        }
    }

    private var music: some View {
        Section("Music") {
            let libs = model.libraries.filter { $0.kind == .music }
            if libs.isEmpty { Text("Add a Music library to play music under the guide.").font(.caption).foregroundStyle(.secondary) }
            ForEach(libs) { lib in
                Toggle("\(lib.name) (\(lib.items) files)", isOn: Binding(get: { gs.musicLibraryIds.contains(lib.id) }, set: { on in
                    if on { gs.musicLibraryIds.append(lib.id) } else { gs.musicLibraryIds.removeAll { $0 == lib.id } }
                }))
            }
        }
    }

    // MARK: preview

    private func refreshPreview() async {
        try? await Task.sleep(nanoseconds: 350_000_000)
        guard !Task.isCancelled, let client = model.client else { return }
        do {
            let data = try await client.guidePreview(id: id, name: name, guide: gs.apply(to: raw))
            guard !Task.isCancelled else { return }
            preview = NSImage(data: data)
            previewError = nil
        } catch {
            guard !Task.isCancelled else { return }
            previewError = error.localizedDescription
        }
    }
}
