import AppKit
import SwiftUI
import TVMasterKit

// MARK: - small pieces

/// A colour setting: one of the theme's colours, or a colour of your own.
struct DesignColourField: View {
    var label = "Colour"
    @Binding var value: String

    var body: some View {
        HStack(spacing: 6) {
            Picker(label, selection: Binding(get: { DesignColour.isRole(value) ? value : "#custom" }, set: { v in
                value = v == "#custom" ? DesignColour.hex(value, fallback: "#ff8800") : v
            })) {
                ForEach(DesignColour.roles, id: \.value) { Text($0.label).tag($0.value) }
                Text("A colour of my own…").tag("#custom")
            }
            if !DesignColour.isRole(value) {
                ColorPicker("", selection: Binding(get: { Color(hex: DesignColour.hex(value)) }, set: { value = $0.hexString }), supportsOpacity: false).labelsHidden()
            }
        }
    }
}

private extension Color {
    init(hex: String) {
        let s = hex.dropFirst()
        let v = UInt32(s, radix: 16) ?? 0xffffff
        self.init(red: Double((v >> 16) & 255) / 255, green: Double((v >> 8) & 255) / 255, blue: Double(v & 255) / 255)
    }
    var hexString: String {
        let c = NSColor(self).usingColorSpace(.sRGB) ?? .white
        return String(format: "#%02x%02x%02x", Int(round(c.redComponent * 255)), Int(round(c.greenComponent * 255)), Int(round(c.blueComponent * 255)))
    }
}

/// A picture of a design at one moment, redrawn a moment after each change.
struct ScreenFrame: View {
    @Environment(AppModel.self) private var model
    let kind: String
    let design: ScreenDesign
    let channel: String?
    let t: Double
    @State private var image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(16.0 / 9.0, contentMode: .fit).clipShape(RoundedRectangle(cornerRadius: 8))
            } else {
                RoundedRectangle(cornerRadius: 8).fill(.black.opacity(0.4)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(ProgressView().controlSize(.small))
            }
        }
        .task(id: "\(kind)|\(channel ?? "")|\(t)|\(design.json)") {
            try? await Task.sleep(nanoseconds: t == 0 ? 150_000_000 : 60_000_000)
            guard !Task.isCancelled, let client = model.client else { return }
            if let data = try? await client.screenPreview(kind: kind, design: design, t: t, channel: channel), let img = NSImage(data: data) { image = img }
        }
    }
}

/// The big preview with play and a scrubber.
struct ScreenPlayer: View {
    let kind: String
    let design: ScreenDesign
    let channel: String?
    let seconds: Int
    @State private var t = 2.5
    @State private var playing = false

    private var length: Double { seconds == 0 ? 10 : Double(seconds) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ScreenFrame(kind: kind, design: design, channel: channel, t: t)
            HStack {
                Button(playing ? "Pause" : "Play") {
                    if !playing && t >= length - 0.3 { t = 0 }
                    playing.toggle()
                }.glassButton()
                Slider(value: Binding(get: { t }, set: { playing = false; t = $0 }), in: 0...length, step: 0.25)
                Text(String(format: "%.1fs", t)).font(.caption.monospacedDigit()).foregroundStyle(.secondary).frame(width: 46, alignment: .trailing)
            }
            if seconds == 0 {
                Text("This screen stays up for as long as it is needed, so its entrance plays once and any motion carries on.").font(.caption).foregroundStyle(.secondary)
            }
        }
        .task(id: playing) {
            while playing, !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 250_000_000)
                if playing { t = t + 0.25 > length ? 0 : t + 0.25 }
            }
        }
    }
}

private struct IntField: View {
    var label: String
    @Binding var value: Int
    var range: ClosedRange<Int> = 0...2000
    var body: some View {
        HStack {
            Text(label)
            Spacer()
            TextField("", value: Binding(get: { value }, set: { value = min(range.upperBound, max(range.lowerBound, $0)) }), format: .number.grouping(.never))
                .frame(width: 80).multilineTextAlignment(.trailing)
        }
    }
}

// MARK: - the background

struct DesignBackdropEditor: View {
    @Binding var backdrop: DesignBackdrop

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Picker("Background", selection: Binding(get: { backdrop.kind }, set: { setKind($0) })) {
                ForEach(DesignBackdrop.kinds, id: \.value) { Text($0.label).tag($0.value) }
            }
            switch backdrop.kind {
            case "solid": DesignColourField(label: "Colour", value: $backdrop.color)
            case "gradient":
                DesignColourField(label: "Top", value: $backdrop.top)
                DesignColourField(label: "Bottom", value: $backdrop.bottom)
            case "aurora":
                Text("Colours (blank = the station theme's). Up to five.").font(.caption).foregroundStyle(.secondary)
                ForEach(backdrop.colors.indices, id: \.self) { i in
                    HStack {
                        DesignColourField(label: "Colour \(i + 1)", value: Binding(get: { backdrop.colors[i] }, set: { backdrop.colors[i] = $0 }))
                        Button { backdrop.colors.remove(at: i) } label: { Image(systemName: "xmark.circle.fill") }.buttonStyle(.borderless)
                    }
                }
                if backdrop.colors.count < 5 { Button { backdrop.colors.append("accent") } label: { Label("Add a colour", systemImage: "plus") } }
            case "pattern":
                Picker("Pattern", selection: $backdrop.pattern) { Text("Dots").tag("dots"); Text("Grid").tag("grid"); Text("Stripes").tag("stripes") }.pickerStyle(.segmented)
                DesignColourField(label: "Pattern colour", value: $backdrop.color)
                DesignColourField(label: "Behind it", value: $backdrop.bg)
                IntField(label: "Size", value: $backdrop.size, range: 8...200)
            case "picture":
                PictureChooser(title: "Picture", path: $backdrop.path, hint: "Fills the screen, cropped to fit.")
            default: EmptyView()
            }
        }
    }

    private func setKind(_ k: String) {
        guard k != backdrop.kind else { return }
        var b = DesignBackdrop()
        b.kind = k
        switch k {
        case "pattern": b.color = "primary"
        default: break
        }
        backdrop = b
    }
}

// MARK: - one layer

struct DesignLayerEditor: View {
    @Binding var layer: DesignLayer
    var onRemove: () -> Void
    var onUp: () -> Void
    var onDown: () -> Void

    var body: some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 8) {
                switch layer.kind {
                case "text":
                    TextField("Words", text: $layer.text, prompt: Text("{channel} {number} {show} {title} {time} {message} {clock} {date}"))
                    IntField(label: "Size", value: $layer.size, range: 8...400)
                    DesignColourField(label: "Colour", value: $layer.color)
                    IntField(label: "Widest (‰ of the screen)", value: $layer.width, range: 50...1000)
                    IntField(label: "Lines", value: $layer.lines, range: 1...6)
                    Toggle("Bold", isOn: $layer.bold)
                    Toggle("Chunky pixel letters", isOn: $layer.pixel)
                    Toggle("Shadow", isOn: $layer.shadow)
                case "clock":
                    IntField(label: "Size", value: $layer.size, range: 8...400)
                    DesignColourField(label: "Colour", value: $layer.color)
                    Toggle("Bold", isOn: $layer.bold)
                    Toggle("12-hour", isOn: $layer.hour12)
                    Toggle("Seconds", isOn: $layer.seconds)
                    Toggle("Date underneath", isOn: $layer.date)
                case "shape":
                    IntField(label: "Width (‰ of the screen)", value: $layer.w, range: 1...2000)
                    IntField(label: "Height (‰ of the screen)", value: $layer.h, range: 1...2000)
                    IntField(label: "Rounded corners (px)", value: $layer.radius, range: 0...400)
                    Toggle("Frosted glass (blurs what is behind it)", isOn: $layer.glass)
                    Toggle("Oval / circle", isOn: $layer.circle)
                    DesignColourField(label: layer.glass ? "Tint" : "Colour", value: $layer.color)
                    if !layer.glass {
                        Toggle("Fades to a second colour (top to bottom)", isOn: Binding(get: { layer.color2 != nil }, set: { layer.color2 = $0 ? "dark" : nil }))
                        if layer.color2 != nil { DesignColourField(label: "Second colour", value: Binding(get: { layer.color2 ?? "dark" }, set: { layer.color2 = $0 })) }
                    }
                case "list":
                    IntField(label: "Size", value: $layer.size, range: 8...400)
                    IntField(label: "Lines", value: $layer.rows, range: 1...8)
                    IntField(label: "Widest (‰ of the screen)", value: $layer.width, range: 100...1000)
                    DesignColourField(label: "Titles", value: $layer.color)
                    DesignColourField(label: "Times", value: $layer.timeColor)
                default:
                    PictureChooser(title: "Picture", path: $layer.path, hint: "A logo or photo. See-through parts of a png stay see-through.")
                    IntField(label: "Width (‰ of the screen)", value: $layer.w, range: 1...2000)
                    IntField(label: "Height (‰ of the screen)", value: $layer.h, range: 1...2000)
                }
                Divider()
                IntField(label: "Across (‰: 0 = left edge, 1000 = right)", value: $layer.x, range: -500...1500)
                IntField(label: "Down (‰: 0 = top, 1000 = bottom)", value: $layer.y, range: -500...1500)
                Picker("Anchored at", selection: $layer.align) { Text("Its left").tag("left"); Text("Middle").tag("center"); Text("Its right").tag("right") }.pickerStyle(.segmented)
                IntField(label: "Opacity %", value: $layer.opacity, range: 0...100)
                Picker("Arrives", selection: $layer.anim.enter) { ForEach(DesignAnim.enters, id: \.value) { Text($0.label).tag($0.value) } }
                IntField(label: "Takes (ms)", value: $layer.anim.enterMs, range: 0...20000)
                IntField(label: "After (ms)", value: $layer.anim.delayMs, range: 0...60000)
                Picker("Then", selection: $layer.anim.motion) { ForEach(DesignAnim.motions, id: \.value) { Text($0.label).tag($0.value) } }
                IntField(label: "Speed %", value: $layer.anim.speed, range: 10...500)
            }
            .padding(.top, 6)
        } label: {
            HStack {
                Text(layer.title).fontWeight(.semibold)
                Spacer()
                Button(action: onUp) { Image(systemName: "chevron.up") }.buttonStyle(.borderless).help("Move behind")
                Button(action: onDown) { Image(systemName: "chevron.down") }.buttonStyle(.borderless).help("Move in front")
                Button(role: .destructive, action: onRemove) { Image(systemName: "trash") }.buttonStyle(.borderless)
            }
        }
        .padding(10).background(.quaternary.opacity(0.3), in: RoundedRectangle(cornerRadius: 10))
    }
}

// MARK: - one screen

struct ScreenEditor: View {
    @Environment(AppModel.self) private var model
    let kind: ScreenKindInfo
    let channel: String?
    var onSaved: () async -> Void
    @State private var design: ScreenDesign?
    @State private var preset = "classic"
    @State private var json: String?
    @State private var jsonError: String?
    @State private var saving = false

    private var saved: ScreenDesign? { kind.current }
    private var dirty: Bool {
        switch (design, saved) {
        case (nil, nil): false
        case (let a?, let b?): !a.same(as: b)
        default: true
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(kind.name).font(.title2.bold())
                        Text(kind.about).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    switch kind.source {
                    case "channel": Badge(text: "This channel's own")
                    case "station": if channel != nil { Badge(text: "Follows the station") }
                    default: Badge(text: "Classic")
                    }
                }
                if let design {
                    ScreenPlayer(kind: kind.id, design: design, channel: channel, seconds: kind.seconds)
                } else {
                    Text("The classic screen is used, as before: nothing here to preview. Choose a design below to try one.").font(.callout).foregroundStyle(.secondary)
                }
                HStack {
                    Button { Task { await save() } } label: { saving ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Use this on \(channel == nil ? "the whole station" : "this channel")")) }
                        .glassButton(prominent: true).disabled(!dirty || saving)
                    if dirty { Button("Undo changes") { design = saved; preset = kind.currentPreset ?? "classic" }.glassButton() }
                    if channel != nil, kind.source == "channel" {
                        Button("Follow the station instead") { Task { await clear() } }.glassButton()
                    }
                    if channel == nil, kind.source == "station" {
                        Button("Back to the classic screen") { Task { await clear() } }.glassButton()
                    }
                }
            }
            .padding(14).background(.quaternary.opacity(0.25), in: RoundedRectangle(cornerRadius: 12))

            VStack(alignment: .leading, spacing: 10) {
                Text("Choose a design").font(.headline)
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 190), spacing: 12, alignment: .top)], alignment: .leading, spacing: 12) {
                    tile(selected: design == nil) { design = nil; preset = "classic" } content: {
                        RoundedRectangle(cornerRadius: 8).fill(.black.opacity(0.35)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(Text("As before").font(.caption).foregroundStyle(.secondary))
                    } title: { "Classic" } about: { "The built-in screen." }
                    ForEach(kind.presets) { p in
                        tile(selected: design != nil && preset == p.id) { design = p.design; preset = p.id } content: {
                            ScreenFrame(kind: kind.id, design: p.design, channel: channel, t: 3.2)
                        } title: { p.name } about: { p.about }
                    }
                }
            }
            .padding(14).background(.quaternary.opacity(0.25), in: RoundedRectangle(cornerRadius: 12))

            if design != nil { makeItYours() }
        }
        .onAppear(perform: reset)
        .onChange(of: kind) { _, _ in reset() }
    }

    private func reset() { design = kind.current; preset = kind.currentPreset ?? "classic"; json = nil }

    private func tile<C: View>(selected: Bool, action: @escaping () -> Void, @ViewBuilder content: () -> C, title: () -> String, about: () -> String) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 4) {
                content()
                Text(title()).fontWeight(.semibold)
                Text(about()).font(.caption).foregroundStyle(.secondary).lineLimit(3).multilineTextAlignment(.leading)
            }
            .padding(8)
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Color.accentColor : Color.secondary.opacity(0.25), lineWidth: selected ? 2 : 1))
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func makeItYours() -> some View {
        let binding = Binding<ScreenDesign>(get: { design ?? ScreenDesign() }, set: { design = $0; preset = "custom" })
        VStack(alignment: .leading, spacing: 14) {
            Text("Make it yours").font(.headline)
            if !binding.wrappedValue.wordLayers.isEmpty {
                Text("Words").fontWeight(.semibold)
                ForEach(binding.wrappedValue.wordLayers, id: \.self) { i in
                    HStack {
                        TextField("Words", text: Binding(get: { binding.wrappedValue.layers[i].text }, set: { var x = binding.wrappedValue; x.layers[i].text = $0; binding.wrappedValue = x }))
                        DesignColourField(label: "Colour", value: Binding(get: { binding.wrappedValue.layers[i].color }, set: { var x = binding.wrappedValue; x.layers[i].color = $0; binding.wrappedValue = x })).frame(maxWidth: 260)
                        Slider(value: Binding(get: { Double(binding.wrappedValue.layers[i].size) }, set: { var x = binding.wrappedValue; x.layers[i].size = Int($0); binding.wrappedValue = x }), in: 16...220).frame(width: 110).help("Size")
                    }
                }
                Text("Fill-ins: " + kind.words.map { "{\($0)} (\(designWordHelp[$0] ?? $0))" }.joined(separator: ", ") + ".").font(.caption).foregroundStyle(.secondary)
            }
            DesignBackdropEditor(backdrop: Binding(get: { binding.wrappedValue.backdrop }, set: { var x = binding.wrappedValue; x.backdrop = $0; binding.wrappedValue = x }))
            LabeledContent("Motion speed \(binding.wrappedValue.motionSpeed)%") {
                Slider(value: Binding(get: { Double(binding.wrappedValue.motionSpeed) }, set: { var x = binding.wrappedValue; x.motionSpeed = Int($0); binding.wrappedValue = x }), in: 20...300, step: 10)
            }
            if binding.wrappedValue.hasMotion {
                Button("Keep it still after it arrives") { var x = binding.wrappedValue; x.keepStill(); binding.wrappedValue = x }.glassButton()
            }
            MoreOptions(summary: "every layer, fading out, the design as text") {
                Text("Layers").font(.headline)
                Text("The bottom of the list is in front. Open a layer to change where it sits, how it arrives and what it keeps doing.").font(.caption).foregroundStyle(.secondary)
                ForEach(binding.wrappedValue.layers.indices, id: \.self) { i in
                    DesignLayerEditor(
                        layer: Binding(get: { binding.wrappedValue.layers[i] }, set: { var x = binding.wrappedValue; x.layers[i] = $0; binding.wrappedValue = x }),
                        onRemove: { var x = binding.wrappedValue; x.layers.remove(at: i); binding.wrappedValue = x },
                        onUp: { var x = binding.wrappedValue; x.move(i, by: -1); binding.wrappedValue = x },
                        onDown: { var x = binding.wrappedValue; x.move(i, by: 1); binding.wrappedValue = x })
                }
                HStack {
                    ForEach([("text", "Words"), ("shape", "Shape"), ("clock", "Clock"), ("list", "Programme list"), ("image", "Picture")], id: \.0) { k in
                        Button { var x = binding.wrappedValue; x.layers.append(DesignLayer(kind: k.0)); binding.wrappedValue = x } label: { Label(k.1, systemImage: "plus") }.glassButton()
                    }
                }
                if kind.seconds > 0 {
                    IntField(label: "Fade everything out over (ms) — 0 = it just cuts", value: Binding(get: { binding.wrappedValue.fadeOutMs }, set: { var x = binding.wrappedValue; x.fadeOutMs = $0; binding.wrappedValue = x }), range: 0...5000)
                }
                Text("The design as text (JSON)").font(.headline)
                if let text = json {
                    TextEditor(text: Binding(get: { text }, set: { json = $0 })).font(.body.monospaced()).frame(height: 220)
                        .overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                    if let e = jsonError { Text(e).foregroundStyle(.red).font(.caption) }
                    HStack {
                        Button("Apply") {
                            if let data = text.data(using: .utf8), let any = try? JSONSerialization.jsonObject(with: data) {
                                binding.wrappedValue = ScreenDesign(json: JSONValue(any: any)); jsonError = nil
                            } else { jsonError = "That is not valid JSON." }
                        }.glassButton()
                        Button("Hide") { json = nil }.glassButton()
                    }
                } else {
                    Button("Show") { json = String(data: (try? JSONEncoder().encode(binding.wrappedValue.json)) ?? Data(), encoding: .utf8).flatMap(prettify) ?? "" }.glassButton()
                }
            }
        }
        .padding(14).background(.quaternary.opacity(0.25), in: RoundedRectangle(cornerRadius: 12))
    }

    private func prettify(_ s: String) -> String? {
        guard let d = s.data(using: .utf8), let o = try? JSONSerialization.jsonObject(with: d),
              let p = try? JSONSerialization.data(withJSONObject: o, options: [.prettyPrinted, .sortedKeys]) else { return nil }
        return String(data: p, encoding: .utf8)
    }

    private func save() async {
        guard let client = model.client else { return }
        saving = true
        defer { saving = false }
        let ok: Bool
        if let design {
            ok = await model.run { try await client.setScreen(kind: kind.id, channel: channel, preset: preset, design: design) } != nil
        } else {
            ok = await model.run { try await client.clearScreen(kind: kind.id, channel: channel) } != nil
        }
        if ok { await onSaved() }
    }

    private func clear() async {
        guard let client = model.client else { return }
        if await model.run({ try await client.clearScreen(kind: kind.id, channel: channel) }) != nil { await onSaved() }
    }
}

// MARK: - the page

struct ScreensView: View {
    @Environment(AppModel.self) private var model
    @State private var channel: String?
    @State private var kinds: [ScreenKindInfo] = []
    @State private var selected = "standby"
    @State private var loaded = false

    private var channels: [Channel] { model.channels.filter { $0.kind == .programming || $0.kind == .test } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Screens", subtitle: "Stand-by, sign-off and the screens around breaks") { EmptyView() }
                HStack {
                    Text("Applies to").fontWeight(.semibold)
                    Picker("", selection: Binding(get: { channel == nil ? "" : channel! }, set: { channel = $0.isEmpty ? nil : $0; Task { await load() } })) {
                        Text("The whole station").tag("")
                        ForEach(channels) { Text("\($0.number) \($0.name)").tag($0.id) }
                    }.labelsHidden().frame(maxWidth: 280)
                    Text(channel == nil ? "Every channel follows these unless it has its own." : "Choose a design for just this channel.").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                }
                if kinds.isEmpty && !loaded {
                    ProgressView()
                } else {
                    HStack(alignment: .top, spacing: 18) {
                        VStack(spacing: 6) {
                            ForEach(kinds) { k in
                                Button { selected = k.id } label: {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(k.name).fontWeight(.semibold)
                                        Text(k.source == "none" ? "Classic" : (k.current?.name ?? "Designed") + (channel != nil && k.source == "channel" ? " · its own" : ""))
                                            .font(.caption).foregroundStyle(.secondary)
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading).padding(10)
                                    .background(selected == k.id ? Color.accentColor.opacity(0.18) : Color.clear, in: RoundedRectangle(cornerRadius: 10))
                                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected == k.id ? Color.accentColor : Color.secondary.opacity(0.25)))
                                }.buttonStyle(.plain)
                            }
                        }
                        .frame(width: 190)
                        if let k = kinds.first(where: { $0.id == selected }) ?? kinds.first {
                            ScreenEditor(kind: k, channel: channel, onSaved: { await load() }).id("\(k.id)|\(channel ?? "")")
                        }
                    }
                }
            }
            .padding(24)
        }
        .task { await load() }
    }

    private func load() async {
        guard let client = model.client else { return }
        if let k = try? await client.screens(channel: channel) { kinds = k }
        loaded = true
    }
}
