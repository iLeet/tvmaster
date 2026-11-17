import SwiftUI
import TVMasterKit

/// Breaks between and inside shows: how much advertising, where commercials come from, and the station cards.
struct BreaksForm: View {
    @Environment(AppModel.self) private var model
    @Binding var ls: LineupSettings
    var libraries: [Library]
    /// This channel's own theme (nil = the station's).
    var theme: ThemeChoice?

    @State private var options: AdOptions?
    @State private var station = ThemeChoice()
    @State private var slot = 30
    @State private var words = ""
    @State private var more = false

    private var look: ThemeChoice { theme ?? station }

    var body: some View {
        Form {
            Section {
                Toggle("Breaks between and inside shows", isOn: Binding(get: { ls.ads != nil }, set: { on in ls.ads = on ? AdSettings() : nil }))
                Text("Each show is cut at natural points and the time left in its slot is filled to the exact second with commercials and station cards. Off = shows run clean.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let ads = ls.ads {
                let presets = options?.presets ?? []
                let current = ads.presetID(in: presets)
                Section("How much advertising") {
                    Picker("", selection: Binding(get: { current }, set: { id in if let p = presets.first(where: { $0.id == id }) { ls.ads?.apply(p) } })) {
                        ForEach(presets) { Text($0.name).tag($0.id) }
                        if current == "custom" { Text("Custom").tag("custom") }
                    }.pickerStyle(.segmented).labelsHidden()
                    Text(presets.first(where: { $0.id == current })?.about ?? "Your own numbers (below).").font(.caption).foregroundStyle(.secondary)
                    Picker("Try it on a slot of", selection: $slot) { Text("30 min").tag(30); Text("60 min").tag(60); Text("2 hours").tag(120) }.pickerStyle(.segmented)
                    if !words.isEmpty { Text(words).padding(10).frame(maxWidth: .infinity, alignment: .leading).background(.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8)) }
                    DisclosureGroup("Fine-tune…", isExpanded: $more) {
                        Stepper("About how long is each break: \(ads.podSeconds) s", value: bind(\.podSeconds), in: 20...900, step: 10)
                        Stepper("At most \(ads.maxBreaks) breaks inside one show", value: bind(\.maxBreaks), in: 0...8)
                        Stepper("Don't repeat a commercial for \(ads.noRepeat) commercials", value: bind(\.noRepeat), in: 1...200)
                        Stepper("Minutes of ads per hour: \(ads.minutesPerHour)", value: bind(\.minutesPerHour), in: 0...30)
                        Text("Only for a channel with no weekly lineup. With time slots the ads fill whatever is left of each slot.").font(.caption).foregroundStyle(.secondary)
                        Toggle("Breaks inside shows (off = all the ads after the show)", isOn: bind(\.midBreaks))
                    }
                }
                Section("Where commercials come from") {
                    let commercial = libraries.filter { $0.kind == .commercials }
                    Toggle("Air commercials (off = only station cards fill the breaks)", isOn: bind(\.commercials))
                    if commercial.isEmpty { Text("Add a library of kind “Commercials” first (Libraries), or upload some there.").font(.caption).foregroundStyle(.secondary) }
                    ForEach(commercial) { lib in
                        Toggle(lib.name + " (\(lib.items))", isOn: Binding(get: { ads.libraryIds.contains(lib.id) }, set: { on in toggle(lib.id, on, \.libraryIds) }))
                    }
                    Toggle("Only from these years", isOn: Binding(get: { ads.years != nil }, set: { on in ls.ads?.years = on ? 1980...1989 : nil }))
                    if let y = ads.years {
                        HStack {
                            TextField("From", value: Binding(get: { y.lowerBound }, set: { v in ls.ads?.years = min(v, y.upperBound)...y.upperBound }), format: .number.grouping(.never)).frame(width: 80)
                            Text("to")
                            TextField("To", value: Binding(get: { y.upperBound }, set: { v in ls.ads?.years = y.lowerBound...max(v, y.lowerBound) }), format: .number.grouping(.never)).frame(width: 80)
                        }.textFieldStyle(.roundedBorder)
                    }
                    Text("The year comes from folder or file names. Handy for a channel set in 1987.").font(.caption).foregroundStyle(.secondary)
                }
                Section("Station cards") {
                    Text("Short graphics in the station's look (change it under Appearance) that tell viewers what is happening. Turn off the ones you don't want.")
                        .font(.caption).foregroundStyle(.secondary)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 14)], spacing: 14) {
                        ForEach(options?.cards ?? []) { c in
                            VStack(alignment: .leading, spacing: 6) {
                                ThemePreviewView(choice: look, what: "bumper", id: "preview", scene: c.id)
                                Toggle(isOn: cardBinding(c.id)) {
                                    VStack(alignment: .leading, spacing: 1) { Text(c.name).fontWeight(.semibold); Text(c.about).font(.caption).foregroundStyle(.secondary) }
                                }
                            }
                        }
                    }
                    if ads.laterTodayEvery > 0 {
                        Stepper("“Later today” replaces “Up next” every \(ads.laterTodayEvery) programmes", value: bind(\.laterTodayEvery), in: 1...20)
                    }
                    let clips = libraries.filter { $0.kind == .bumpers }
                    Text(clips.isEmpty ? "Your own station clips: add a library of kind “Bumper clips” (or upload some in Libraries) to use your own videos." : "Your own station clips: short videos of your own. One opens a break, in place of the generated card, when it fits.")
                        .font(.caption).foregroundStyle(.secondary)
                    ForEach(clips) { lib in
                        Toggle("Clips: " + lib.name + " (\(lib.items))", isOn: Binding(get: { ads.bumperLibraryIds.contains(lib.id) }, set: { on in toggle(lib.id, on, \.bumperLibraryIds) }))
                    }
                }
            }
            if let p = ls.problem { Section { Label(p, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange).font(.callout) } }
        }
        .formStyle(.grouped)
        .task {
            options = try? await model.client?.adOptions()
            if theme == nil, let j = try? await model.client?.stationThemeJSON(), let t = ThemeChoice(json: j) { station = t }
        }
        .task(id: "\(ls.ads?.json.hashValue ?? 0)|\(slot)") {
            guard let ads = ls.ads else { return }
            try? await Task.sleep(nanoseconds: 200_000_000)
            guard !Task.isCancelled else { return }
            words = (try? await model.client?.explainAds(ads, slotMinutes: slot, showMinutes: max(1, Int((Double(slot) * 0.73).rounded())))) ?? ""
        }
    }

    private func bind<T>(_ key: WritableKeyPath<AdSettings, T>) -> Binding<T> {
        Binding(get: { ls.ads?[keyPath: key] ?? AdSettings()[keyPath: key] }, set: { ls.ads?[keyPath: key] = $0 })
    }

    private func cardBinding(_ id: String) -> Binding<Bool> {
        Binding(get: {
            guard let a = ls.ads else { return false }
            switch id { case "brb": return a.brb; case "back_to": return a.backTo; case "up_next": return a.upNext; case "station_id": return a.stationId; default: return a.laterTodayEvery > 0 }
        }, set: { on in
            switch id {
            case "brb": ls.ads?.brb = on
            case "back_to": ls.ads?.backTo = on
            case "up_next": ls.ads?.upNext = on
            case "station_id": ls.ads?.stationId = on
            default: ls.ads?.laterTodayEvery = on ? 3 : 0
            }
        })
    }

    private func toggle(_ id: Int, _ on: Bool, _ key: WritableKeyPath<AdSettings, [Int]>) {
        guard var a = ls.ads else { return }
        var ids = a[keyPath: key]
        if on { if !ids.contains(id) { ids.append(id) } } else { ids.removeAll { $0 == id } }
        a[keyPath: key] = ids
        ls.ads = a
    }
}
