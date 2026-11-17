import SwiftUI
import TVMasterKit

/// Make your own commercials and promos, or use old ones.
struct PromosView: View {
    @Environment(AppModel.self) private var model
    @State private var options: PromoOptions?
    @State private var spots: [PromoSpot] = []
    @State private var music: [PromoMusic] = []
    @State private var draft = PromoDraft()
    @State private var own = false
    @State private var preview: NSImage?
    @State private var previewError: String?
    @State private var busy = false
    @State private var note: String?
    @State private var link = ""
    @State private var station = ThemeChoice()

    private var commercials: [Library] { model.libraries.filter { $0.kind == .commercials } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "Promos", subtitle: "Make your own commercials and promos, or use old ones")
                if let note { Label(note, systemImage: "checkmark.circle").foregroundStyle(.green) }
                maker
                madeHere
                oldOnes
            }
            .padding(28)
        }
        .task { await load() }
        .task(id: previewKey) {
            try? await Task.sleep(nanoseconds: 350_000_000)
            guard !Task.isCancelled, let c = model.client else { return }
            do {
                let data = try await c.promoPreview(effective)
                if let img = NSImage(data: data) { preview = img; previewError = nil }
            } catch { previewError = error.localizedDescription }
        }
    }

    private var effective: PromoDraft { var d = draft; if !own { d.theme = nil } else if d.theme == nil { d.theme = station }; return d }
    private var previewKey: String { "\(effective.json.hashValue)" }

    private var maker: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Make a spot").font(.headline)
            Picker("", selection: $draft.kind) { ForEach(options?.kinds ?? []) { Text($0.name).tag($0.id) } }.pickerStyle(.segmented).labelsHidden()
            Text(options?.kinds.first { $0.id == draft.kind }?.about ?? "").font(.caption).foregroundStyle(.secondary)
            HStack(alignment: .top, spacing: 22) {
                VStack(alignment: .leading, spacing: 10) {
                    if draft.kind == "slides" {
                        TextField("Heading (optional)", text: $draft.title)
                        Text("Slides, one line each").font(.caption).foregroundStyle(.secondary)
                        TextEditor(text: Binding(get: { draft.slides.joined(separator: "\n") }, set: { draft.slides = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(10)) }))
                            .frame(height: 110).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                    } else {
                        TextField(draft.kind == "coming_up" ? "Programme" : "Name", text: $draft.title, prompt: Text(draft.kind == "coming_up" ? "Cheers" : "Mel's Diner"))
                        TextField(draft.kind == "coming_up" ? "When" : "Tagline", text: $draft.tagline, prompt: Text(draft.kind == "coming_up" ? "Thursdays at 8" : "Best pie in town"))
                        TextField(draft.kind == "coming_up" ? "Small print (optional)" : "Contact line (optional)", text: $draft.contact, prompt: Text("Call 555-0134 · open daily"))
                    }
                    Picker("Length", selection: $draft.seconds) { ForEach(options?.seconds ?? [15, 30], id: \.self) { Text("\($0) s").tag($0) } }.pickerStyle(.segmented)
                    TextField("Year (optional: lets a channel set in one era use it)", value: $draft.year, format: .number.grouping(.never)).frame(maxWidth: 380)
                    PictureChooser(title: "Picture behind the words", path: $draft.picture)
                    Picker("Make it look old", selection: $draft.look) { ForEach(options?.looks ?? []) { Text($0.name).tag($0.id) } }
                    Picker("Sound", selection: $draft.sound) { ForEach((options?.sounds ?? []).filter { $0.id != "music" || !music.isEmpty }) { Text($0.name).tag($0.id) } }
                    if draft.sound == "music" {
                        Picker("Music", selection: $draft.music) { Text("Choose…").tag(""); ForEach(music) { Text("\($0.name) (\($0.seconds)s)").tag($0.path) } }
                    }
                    Toggle("Its own look (otherwise the station's theme)", isOn: $own)
                    if own {
                        let t = Binding(get: { draft.theme ?? station }, set: { draft.theme = $0 })
                        ThemePickerView(theme: t.theme, primary: t.primary, accent: t.accent, background: t.background, autoName: "From the title", autoAbout: "Colours made from the title.")
                        StylePickerView(style: t.style)
                    }
                }
                .textFieldStyle(.roundedBorder).frame(maxWidth: 440)
                VStack(alignment: .leading, spacing: 10) {
                    Group {
                        if let preview { Image(nsImage: preview).resizable().aspectRatio(16.0 / 9.0, contentMode: .fit).clipShape(RoundedRectangle(cornerRadius: 10)) }
                        else { RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.4)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(ProgressView()) }
                    }
                    if let previewError { Text(previewError).font(.caption).foregroundStyle(.orange) }
                    Button { Task { await make() } } label: { busy ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Make the \(draft.seconds)-second spot")) }
                        .glassButton(prominent: true).disabled(busy || !draft.ready)
                    Text("It is saved as a video in your commercials (Station promos), so it airs in breaks like any commercial. Channels that limit ads to certain years use it only if you gave it a year.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
    }

    private var madeHere: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Made here").font(.headline)
            if spots.isEmpty { Text("Nothing yet.").foregroundStyle(.secondary) }
            ForEach(spots) { p in
                HStack {
                    Text(p.name).lineLimit(1)
                    Spacer()
                    Text("\(p.seconds) s").foregroundStyle(.secondary)
                    Button(role: .destructive) { Task { _ = await model.run { try await model.client?.deletePromo(p.id) }; await load() } } label: { Image(systemName: "trash") }.glassButton()
                }
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
    }

    private var oldOnes: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Use old commercials").font(.headline)
            Text("Bring in real ones from the era: upload files from your own tapes and downloads (Libraries → your commercials library → Upload), or give the server a direct link to a video file and it downloads it. The Prelinger Archives and the Internet Archive have public-domain commercials and station breaks from the 1950s to the 80s (use the file's direct download link). To make anything look like it came off a tape, set the channel's Look to VHS or Tube TV under Picture & sound. Put a year in the folder name, such as Commercials/1986/, to let channels pick an era.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                TextField("https://…/commercial.mp4", text: $link).textFieldStyle(.roundedBorder)
                Button("Download it") {
                    Task {
                        guard let lib = commercials.first else { return }
                        busy = true; defer { busy = false }
                        if await model.run({ try await model.client?.fetchIntoLibrary(lib.id, url: link) }) != nil { note = "Downloaded and added to the commercials."; link = ""; await model.refreshLibraries() }
                    }
                }.glassButton().disabled(busy || commercials.isEmpty || !(link.lowercased().hasPrefix("http://") || link.lowercased().hasPrefix("https://")))
            }
            if commercials.isEmpty { Text("There is no commercials library yet. Add one, or create the station media folder, under Libraries.").font(.caption).foregroundStyle(.orange) }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
    }

    private func load() async {
        guard let c = model.client else { return }
        options = try? await c.promoOptions()
        spots = (try? await c.promos()) ?? []
        music = (try? await c.promoMusic()) ?? []
        if let j = try? await c.stationThemeJSON(), let t = ThemeChoice(json: j) { station = t }
    }

    private func make() async {
        guard let c = model.client else { return }
        busy = true; defer { busy = false }
        if await model.run({ try await c.makePromo(effective) }) != nil { note = "Made. It is in your commercials and will air in breaks."; await model.refreshLibraries(); await load() }
    }
}
