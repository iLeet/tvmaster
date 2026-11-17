import SwiftUI
import TVMasterKit

/// An info channel: its pages, news feeds, ticker, clock and music, with a picture of the page being edited.
struct InfoForm: View {
    @Environment(AppModel.self) private var model
    @Binding var info: InfoSettings
    var raw: JSONValue?
    var id: String?
    var name: String
    var feedSuggestions: [(name: String, url: String)] = []

    @State private var page = 0
    @State private var pageCount = 1
    @State private var image: NSImage?
    @State private var error: String?
    @State private var newFeed = ""

    private var musicLibraries: [Library] { model.libraries.filter { $0.kind == .music } }

    var body: some View {
        Form {
            Section("Preview") {
                HStack {
                    Button { page = (page + pageCount - 1) % pageCount } label: { Label("Page", systemImage: "chevron.left") }.glassButton().disabled(pageCount < 2)
                    Text("\(min(page, pageCount - 1) + 1) of \(pageCount)").font(.caption).foregroundStyle(.secondary)
                    Button { page = (page + 1) % pageCount } label: { Label("Page", systemImage: "chevron.right") }.glassButton().disabled(pageCount < 2)
                    Spacer()
                }
                if let error {
                    Text(error).font(.caption).foregroundStyle(.orange)
                } else if let image {
                    Image(nsImage: image).resizable().aspectRatio(16.0 / 9.0, contentMode: .fit).clipShape(RoundedRectangle(cornerRadius: 10))
                } else {
                    RoundedRectangle(cornerRadius: 10).fill(.black.opacity(0.4)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(ProgressView())
                }
                Text("Drawn in the channel's look. Headlines shown here are stand-ins until the first fetch.").font(.caption).foregroundStyle(.secondary)
            }
            .task(id: previewKey) { await redraw() }

            Section("The channel") {
                TextField("Heading", text: $info.title, prompt: Text(name))
                TextField("What the guide says it is", text: $info.about, prompt: Text("News and headlines"))
                LabeledContent("Each page stays up \(info.seconds) s") { Slider(value: Binding(get: { Double(info.seconds) }, set: { info.seconds = Int($0) }), in: 3...60, step: 1) }
                Toggle("Time and date at the top right", isOn: $info.clock)
                Toggle("12-hour clock", isOn: $info.hour12)
            }

            Section("Pages") {
                if info.slides.isEmpty { Text("No pages yet: the channel shows a clock. Add some below.").foregroundStyle(.secondary) }
                ForEach(info.slides.indices, id: \.self) { i in
                    SlideEditor(slide: $info.slides[i], index: i, count: info.slides.count,
                                onMove: { to in info.slides = InfoSettings.moved(info.slides, from: i, to: to) },
                                onRemove: { info.slides.remove(at: i) })
                        .onTapGesture { page = i }
                }
                HStack {
                    Menu("Add a page") {
                        ForEach(InfoSettings.kinds, id: \.value) { k in Button("\(k.label): \(k.about)") { info.slides.append(InfoSettings.newSlide(k.value)) } }
                    }
                    .fixedSize()
                    Text("Shown in this order, round and round.").font(.caption).foregroundStyle(.secondary)
                }
            }

            Section("Headlines") {
                Text("News feeds (RSS or Atom) for the headlines pages and the ticker, fetched every fifteen minutes by the server.\(info.slides.contains { $0.kind == "headlines" } && info.feeds.isEmpty ? " A headlines page needs at least one." : "")")
                    .font(.caption).foregroundStyle(.secondary)
                ForEach(info.feeds, id: \.self) { f in
                    HStack { Text(f).font(.caption.monospaced()).lineLimit(1).truncationMode(.middle); Spacer(); Button("Remove") { info.feeds.removeAll { $0 == f } }.glassButton() }
                }
                HStack {
                    TextField("https://example.com/news.rss", text: $newFeed).font(.body.monospaced()).onSubmit(addFeed)
                    Button("Add feed", action: addFeed).glassButton().disabled(newFeed.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                let unused = feedSuggestions.filter { !info.feeds.contains($0.url) }
                if !unused.isEmpty {
                    Menu("Or start with…") { ForEach(unused, id: \.url) { f in Button(f.name) { info.feeds.append(f.url) } } }.fixedSize()
                }
            }

            Section("Ticker") {
                Toggle("The scrolling line along the bottom: your lines, then the headlines", isOn: $info.tickerEnabled)
                if info.tickerEnabled {
                    Text("Your lines, one per row").font(.caption).foregroundStyle(.secondary)
                    TextEditor(text: Binding(get: { info.ticker.joined(separator: "\n") }, set: { info.ticker = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(20)) }))
                        .frame(height: 70).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                }
            }

            Section("Music") {
                if musicLibraries.isEmpty { Text("Add a library of kind “Music” to play music under the pages.").foregroundStyle(.secondary) }
                ForEach(musicLibraries) { lib in
                    Toggle("\(lib.name) (\(lib.items) files)", isOn: Binding(get: { info.musicLibraryIds.contains(lib.id) }, set: { on in
                        if on { info.musicLibraryIds.append(lib.id) } else { info.musicLibraryIds.removeAll { $0 == lib.id } }
                    }))
                }
            }
        }
        .formStyle(.grouped)
    }

    private func addFeed() {
        let u = newFeed.trimmingCharacters(in: .whitespaces)
        if !u.isEmpty && !info.feeds.contains(u) { info.feeds.append(u) }
        newFeed = ""
    }

    private var previewKey: String { "\(id ?? "")|\(name)|\(page)|\(String(describing: info.apply(to: raw)))" }

    private func redraw() async {
        try? await Task.sleep(nanoseconds: 400_000_000)
        guard !Task.isCancelled, let client = model.client else { return }
        do {
            let (data, count) = try await client.infoPreview(info: info.apply(to: raw), name: name, id: id, page: page)
            guard !Task.isCancelled else { return }
            image = NSImage(data: data); pageCount = max(count, 1); error = nil
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// One page: what kind it is, its words and picture, and how long it stays up.
private struct SlideEditor: View {
    @Binding var slide: InfoSettings.Slide
    let index: Int
    let count: Int
    var onMove: (Int) -> Void
    var onRemove: () -> Void

    private var kind: String { InfoSettings.kinds.first { $0.value == slide.kind }?.label ?? slide.kind }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Page \(index + 1)").fontWeight(.semibold) + Text("  \(kind)").foregroundStyle(.secondary)
                Spacer()
                Button { onMove(index - 1) } label: { Image(systemName: "chevron.up") }.buttonStyle(.borderless).disabled(index == 0)
                Button { onMove(index + 1) } label: { Image(systemName: "chevron.down") }.buttonStyle(.borderless).disabled(index == count - 1)
                Button("Remove", role: .destructive, action: onRemove).glassButton()
            }
            switch slide.kind {
            case "text":
                TextField("Title", text: $slide.title)
                Text("Text, one line per row; it shrinks to fit.").font(.caption).foregroundStyle(.secondary)
                TextEditor(text: Binding(get: { slide.lines.joined(separator: "\n") }, set: { slide.lines = Array($0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).prefix(12)) }))
                    .frame(height: 80).overlay(RoundedRectangle(cornerRadius: 6).stroke(.secondary.opacity(0.3)))
                PictureChooser(title: "Picture (beside the text)", path: $slide.picture)
            case "photo":
                PictureChooser(title: "Picture (fills the screen)", path: $slide.picture)
                TextField("Caption", text: $slide.title)
            case "headlines", "tonight":
                TextField("Title", text: $slide.title)
                Text(slide.kind == "headlines" ? "Five headlines to a page, as many pages as there are headlines." : "Five channels to a page: what is on now, and what comes next.")
                    .font(.caption).foregroundStyle(.secondary)
            default:
                Text("The time and the date, large.").font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Toggle("Own time on screen", isOn: Binding(get: { slide.seconds != nil }, set: { slide.seconds = $0 ? 12 : nil }))
                if let s = slide.seconds {
                    Slider(value: Binding(get: { Double(s) }, set: { slide.seconds = Int($0) }), in: 3...120, step: 1)
                    Text("\(s) s").font(.caption.monospacedDigit()).frame(width: 40)
                }
            }
        }
        .padding(10).background(.secondary.opacity(0.08), in: RoundedRectangle(cornerRadius: 10))
    }
}
