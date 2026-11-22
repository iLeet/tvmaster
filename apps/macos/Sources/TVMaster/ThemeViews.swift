import SwiftUI
import TVMasterKit

private func swatchColor(_ hex: String) -> Color {
    let h = hex.trimmingCharacters(in: CharacterSet(charactersIn: "# "))
    guard h.count == 6, let v = UInt32(h, radix: 16) else { return .gray }
    return Color(red: Double((v >> 16) & 255) / 255, green: Double((v >> 8) & 255) / 255, blue: Double(v & 255) / 255)
}

private func hexString(_ c: Color) -> String {
    let n = NSColor(c).usingColorSpace(.sRGB) ?? .gray
    return String(format: "#%02x%02x%02x", Int(n.redComponent * 255), Int(n.greenComponent * 255), Int(n.blueComponent * 255))
}

/// Swatches for every theme the server offers, and the colours to pick for a custom one.
struct ThemePickerView: View {
    @Environment(AppModel.self) private var model
    @Binding var theme: String
    @Binding var primary: String
    @Binding var accent: String
    @Binding var background: String
    var autoName: String?
    var autoAbout: String?
    @State private var catalogue: ThemeCatalogue?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 84), spacing: 8)], spacing: 8) {
                ForEach(catalogue?.themes ?? []) { t in
                    let chosen = theme == t.id
                    let custom = t.id == "custom"
                    let sw = t.swatch ?? ["#444444", "#888888", "#cccccc"]
                    Button { theme = t.id } label: {
                        VStack(spacing: 4) {
                            HStack(spacing: 0) {
                                Rectangle().fill(swatchColor(custom && !primary.isEmpty ? primary : sw[0]))
                                Rectangle().fill(swatchColor(custom && !primary.isEmpty ? primary : sw[1])).frame(width: 18)
                                Rectangle().fill(swatchColor(custom && !accent.isEmpty ? accent : sw[2])).frame(width: 18)
                            }
                            .frame(height: 24).clipShape(RoundedRectangle(cornerRadius: 5))
                            Text(t.id == "auto" ? (autoName ?? t.name) : t.name).font(.caption)
                        }
                        .padding(6).frame(maxWidth: .infinity)
                        .background(chosen ? Color.accentColor.opacity(0.18) : .clear, in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(chosen ? Color.accentColor : .secondary.opacity(0.25), lineWidth: chosen ? 2 : 1))
                    }
                    .buttonStyle(.plain).help(t.id == "auto" ? (autoAbout ?? t.about) : t.about)
                }
            }
            if let t = catalogue?.themes.first(where: { $0.id == theme }) {
                Text(theme == "auto" ? (autoAbout ?? t.about) : t.about).font(.caption).foregroundStyle(.secondary)
            }
            if theme == "custom" {
                ColorPicker("Main colour", selection: Binding(get: { swatchColor(primary.isEmpty ? "#3366ff" : primary) }, set: { primary = hexString($0) }), supportsOpacity: false)
                ColorPicker("Highlight", selection: Binding(get: { swatchColor(accent.isEmpty ? "#ffcc00" : accent) }, set: { accent = hexString($0) }), supportsOpacity: false)
                ColorPicker("Background", selection: Binding(get: { swatchColor(background.isEmpty ? "#101830" : background) }, set: { background = hexString($0) }), supportsOpacity: false)
                if !background.isEmpty { Button("Use a dark shade of the main colour") { background = "" }.glassButton() }
            }
        }
        .task { catalogue = try? await model.client?.themeCatalogue() }
    }
}

/// Retro or Modern graphics.
struct StylePickerView: View {
    @Binding var style: String
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Picker("Graphics style", selection: $style) { Text("Retro").tag("retro"); Text("Modern").tag("modern") }.pickerStyle(.segmented)
            Text(style == "retro" ? "Chunky pixel type and scanlines, like a broadcast from another era." : "Smooth type, clean shapes and gentle motion.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

/// Gentle alerts: soft colours and words, a soft chime, nothing flashing (always on in the kids theme).
struct GentleToggleView: View {
    @Binding var choice: ThemeChoice
    var body: some View {
        let kids = choice.theme == "kids"
        VStack(alignment: .leading, spacing: 2) {
            Toggle("Gentle alerts", isOn: Binding(get: { kids || choice.gentle }, set: { choice.gentle = $0 })).disabled(kids)
            Text("\(kids ? "Always on in the kids theme: " : "")soft colours and words, a soft chime, nothing flashing, so a child is not frightened.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }
}

/// The whole look in one place: colours, style and gentle alerts. Choosing the kids theme switches to the smooth style.
struct ThemeChoiceEditorView: View {
    @Binding var choice: ThemeChoice
    var autoName: String?
    var autoAbout: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ThemePickerView(theme: Binding(get: { choice.theme }, set: { new in
                if new == "kids" && choice.theme != "kids" { choice.style = "modern" }
                choice.theme = new
            }), primary: $choice.primary, accent: $choice.accent, background: $choice.background, autoName: autoName, autoAbout: autoAbout)
            StylePickerView(style: $choice.style)
            GentleToggleView(choice: $choice)
        }
    }
}

/// A picture of one of the station's graphics in a theme, redrawn shortly after each change.
struct ThemePreviewView: View {
    @Environment(AppModel.self) private var model
    let choice: ThemeChoice
    let what: String
    var id: String?
    var name: String?
    var number: Int?
    var scene: String?
    var text: String?
    var label: String?
    var top = false
    @State private var image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(16.0 / 9.0, contentMode: .fit).clipShape(RoundedRectangle(cornerRadius: 10))
            } else {
                RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.4)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(ProgressView())
            }
        }
        .task(id: "\(choice.theme)|\(choice.primary)|\(choice.accent)|\(choice.background)|\(choice.style)|\(choice.gentle)|\(what)|\(scene ?? "")|\(id ?? "")|\(name ?? "")|\(text ?? "")|\(label ?? "")|\(top)") {
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard !Task.isCancelled, let client = model.client else { return }
            if let data = try? await client.themePreview(what: what, choice: choice, id: id, name: name, number: number, scene: scene, text: text, label: label, top: top), let img = NSImage(data: data) { image = img }
        }
    }
}

/// One look for the whole station.
struct AppearanceView: View {
    @Environment(AppModel.self) private var model
    @State private var saved = ThemeChoice()
    @State private var draft = ThemeChoice()
    @State private var loaded = false
    @State private var what = "card"
    @State private var saving = false

    private let kinds: [(String, String)] = [("card", "Station card"), ("bumper", "Break card"), ("guide", "Guide"), ("alert", "Alert"), ("bar", "Alert bar")]

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Appearance", subtitle: "One look for the guide, cards, stand-by screen, alerts and on-screen graphics") {
                    HStack {
                        if draft != saved { Text("Unsaved").foregroundStyle(.orange) }
                        Button("Reset") { draft = ThemeChoice() }.glassButton().disabled(draft == ThemeChoice())
                        Button { Task { await save() } } label: { saving ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Save")) }
                            .glassButton(prominent: true).disabled(draft == saved || saving)
                    }
                }
                Text("A channel can choose its own under its settings. Everything else follows this.").foregroundStyle(.secondary)
                LooksSection(onApplied: { loaded = false; if let json = try? await model.client?.stationThemeJSON(), let t = ThemeChoice(json: json) { saved = t; draft = t }; loaded = true })
                VStack(alignment: .leading, spacing: 12) {
                    Text("Theme").font(.headline)
                    ThemeChoiceEditorView(choice: $draft, autoName: "Per channel", autoAbout: "Colours made from each channel, so every channel has its own look.")
                    HStack {
                        Button("Use the kids look") { draft = .kids }.glassButton()
                        Text("Bright colours, smooth type and gentle alerts. To use it for some hours of the day, or some channels, set it under Day parts or on a channel.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("Preview").font(.headline)
                        Spacer()
                        Picker("", selection: $what) { ForEach(kinds, id: \.0) { Text($0.1).tag($0.0) } }.pickerStyle(.segmented).labelsHidden().frame(maxWidth: 460)
                    }
                    ThemePreviewView(choice: draft, what: what)
                }
                .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
            }
            .padding(24)
        }
        .task {
            guard !loaded, let c = try? await model.client?.themeCatalogue() else { return }
            _ = c
            if let json = try? await model.client?.stationThemeJSON(), let t = ThemeChoice(json: json) { saved = t; draft = t }
            loaded = true
        }
    }

    private func save() async {
        saving = true
        defer { saving = false }
        do {
            try await model.client?.setStationTheme(draft)
            saved = draft
        } catch { model.lastError = error.localizedDescription }
    }
}
