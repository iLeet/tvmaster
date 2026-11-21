import SwiftUI
import TVMasterKit

/// A new TV channel, one plain question at a time. Everything it sets can be changed later in the full editor.
struct ChannelGuideView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    var onFullEditor: () -> Void
    var onOtherKind: () -> Void

    private let steps = ["Name", "What it plays", "How it plays", "Commercials", "Look", "Ready"]
    @State private var a = ChannelGuideAnswers()
    @State private var step = 0
    @State private var idTouched = false
    @State private var shows: [ShowSummary] = []
    @State private var presets: [AdPreset] = []
    @State private var words = ""
    @State private var search = ""
    @State private var lookChoice = "station"
    @State private var saving = false
    @State private var started = false

    private var world: ChannelGuideAnswers.World {
        ChannelGuideAnswers.World(
            timezone: model.system?.timezone ?? TimeZone.current.identifier,
            showLibraryIds: model.libraries.filter { $0.kind == .shows }.map(\.id),
            commercialLibraryIds: model.libraries.filter { $0.kind == .commercials }.map(\.id),
            presets: presets)
    }
    private var problem: String? { a.problem(step: step, takenNumbers: model.channels.map(\.number)) }
    private var noShows: Bool { shows.isEmpty || world.showLibraryIds.isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("New TV channel").font(.title2.bold())
                    Spacer()
                    Button("Choose a different kind", action: onOtherKind).glassButton()
                    Button("Use the full editor instead", action: onFullEditor).glassButton()
                }
                HStack(spacing: 6) {
                    ForEach(steps.indices, id: \.self) { i in
                        Badge(text: "\(i + 1). \(steps[i])", color: i == step ? .accentColor : .secondary)
                    }
                }
            }
            .padding([.horizontal, .top], 16)

            Form {
                switch step {
                case 0: nameStep
                case 1: playsStep
                case 2: howStep
                case 3: breaksStep
                case 4: lookStep
                default: readyStep
                }
            }
            .formStyle(.grouped)

            HStack {
                Button("← Back") { step -= 1 }.disabled(step == 0)
                Spacer()
                if let problem { Text(problem).font(.caption).foregroundStyle(.secondary) }
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                if step < steps.count - 1 {
                    Button("Next →") { step += 1 }.glassButton(prominent: true).keyboardShortcut(.defaultAction)
                        .disabled(problem != nil || (step == 1 && noShows))
                }
            }
            .padding(16)
        }
        .task {
            guard !started else { return }
            started = true
            a.number = Format.nextFreeNumber(taken: model.channels.map(\.number), start: 1)
            shows = (try? await model.client?.shows()) ?? []
            presets = (try? await model.client?.adOptions().presets) ?? []
        }
        .task(id: "\(a.breaks)|\(step)") {
            guard step == 3, let ads = a.ads(world), let client = model.client else { words = ""; return }
            words = (try? await client.explainAds(ads, slotMinutes: 30, showMinutes: 22)) ?? ""
        }
    }

    // MARK: steps

    @ViewBuilder private var nameStep: some View {
        Section("What is it called?") {
            TextField("Name", text: $a.name, prompt: Text("Saturday Cartoons")).onChange(of: a.name) { _, n in if !idTouched { a.id = Format.slug(n) } }
            TextField("Channel number (the next free one is filled in)", value: $a.number, format: .number.grouping(.never)).frame(maxWidth: 360)
            MoreOptions(summary: "id \(a.id)\(a.group.isEmpty ? "" : " · group \(a.group)")") {
                TextField("Group", text: $a.group, prompt: Text("Kids, Movies, …"))
                TextField("Id (made from the name)", text: Binding(get: { a.id }, set: { idTouched = true; a.id = $0 })).font(.body.monospaced())
            }
        }
    }

    @ViewBuilder private var playsStep: some View {
        Section("What should it play?") {
            if noShows {
                VStack(alignment: .leading, spacing: 6) {
                    Text("There are no shows to play yet.").fontWeight(.semibold)
                    Text("Add a folder of TV shows first (or upload some), then come back.").font(.caption).foregroundStyle(.secondary)
                    Button("Add shows") { dismiss(); model.selection = .libraries }.glassButton()
                }
            } else {
                Picker("", selection: Binding(get: { a.shows.isEmpty ? "all" : "some" }, set: { v in a.shows = v == "all" ? [] : Array(shows.prefix(1).map(\.name)) })) {
                    Text("Everything (\(shows.count) shows)").tag("all")
                    Text("Only some shows").tag("some")
                }.pickerStyle(.segmented).labelsHidden()
                if !a.shows.isEmpty {
                    HStack {
                        TextField("Find a show", text: $search).frame(maxWidth: 240)
                        Button("All") { a.shows = shows.map(\.name) }.glassButton()
                        Button("Back to everything") { a.shows = [] }.glassButton()
                        Text("\(a.shows.count) chosen").font(.caption).foregroundStyle(.secondary)
                    }
                    ForEach(shows.filter { search.isEmpty || $0.name.localizedCaseInsensitiveContains(search) }) { s in
                        Toggle("\(s.name)  (\(s.episodes) episodes)", isOn: Binding(get: { a.shows.contains(s.name) }, set: { on in
                            if on { a.shows.append(s.name) } else { a.shows.removeAll { $0 == s.name } }
                        }))
                    }
                }
            }
        }
    }

    @ViewBuilder private var howStep: some View {
        Section("How should it play?") {
            ForEach(ChannelGuideAnswers.Style.allCases) { s in
                Button { a.style = s } label: {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: a.style == s ? "largecircle.fill.circle" : "circle").foregroundStyle(a.style == s ? Color.accentColor : .secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(s.title).fontWeight(.semibold)
                            Text(s.about).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.leading)
                        }
                        Spacer()
                    }.contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
            if a.style != .weekly {
                MoreOptions(summary: a.chaos > 0 ? "\(a.chaos)% surprises" : "no surprises") {
                    LabeledContent("Surprises \(a.chaos)%") { Slider(value: Binding(get: { Double(a.chaos) }, set: { a.chaos = Int($0) }), in: 0...100, step: 1) }
                    Text("Swaps this share of picks for something random, so an orderly channel now and then airs something unexpected. 0 = never.").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder private var breaksStep: some View {
        Section("Commercial breaks?") {
            Picker("", selection: $a.breaks) {
                Text("No breaks").tag("clean")
                ForEach(presets) { Text($0.id == "none" ? "Station cards only" : $0.name).tag($0.id) }
            }.pickerStyle(.segmented).labelsHidden()
            Text(a.breaks == "clean" ? "Shows run clean, with only the padding to the next time slot." : presets.first(where: { $0.id == a.breaks })?.about ?? "").font(.caption).foregroundStyle(.secondary)
            if !words.isEmpty { Text(words).padding(10).frame(maxWidth: .infinity, alignment: .leading).background(.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8)) }
            if a.breaks != "clean", presets.first(where: { $0.id == a.breaks })?.commercials == true, world.commercialLibraryIds.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("There are no commercials yet.").fontWeight(.semibold)
                    Text("Breaks will use station cards until you add some: a folder of old commercials, or ones you make.").font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("Add a commercials folder") { dismiss(); model.selection = .libraries }.glassButton()
                        Button("Make or download commercials") { dismiss(); model.selection = .promos }.glassButton()
                    }
                }
            }
            Text("Everything about breaks (timing, years, station cards) is under the channel's Breaks tab afterwards.").font(.caption).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder private var lookStep: some View {
        Section("How should it look?") {
            Picker("", selection: Binding(get: { lookChoice }, set: { v in
                lookChoice = v
                a.look = v == "station" ? nil : v == "kids" ? .kids : ThemeChoice()
            })) {
                Text("Same as the station").tag("station"); Text("The kids look").tag("kids"); Text("Pick a theme").tag("own")
            }.pickerStyle(.segmented).labelsHidden()
            Text(lookChoice == "station" ? "Cards, bumpers, alerts and the guide follow the look chosen under Appearance." : lookChoice == "kids" ? "Bright colours, smooth type and gentle alerts, just for this channel." : "Choose colours and style for this channel.")
                .font(.caption).foregroundStyle(.secondary)
            if lookChoice == "own" {
                ThemeChoiceEditorView(choice: Binding(get: { a.look ?? ThemeChoice() }, set: { a.look = $0 }), autoName: "Per channel", autoAbout: "Colours made from this channel.")
            }
        }
    }

    @ViewBuilder private var readyStep: some View {
        Section("Ready") {
            ForEach(a.summary(world, totalShows: shows.count), id: \.self) { Text("• " + $0) }
            MoreOptions(summary: a.alwaysOn ? "always on" : "starts when someone tunes in") {
                Toggle("Always on", isOn: $a.alwaysOn)
                Text("Keep encoding even with no viewers. It uses processor time all day; leave it off unless the channel must be instant.").font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button { Task { await create(openSchedule: false) } } label: { saving ? AnyView(ProgressView().controlSize(.small)) : AnyView(Text("Create the channel")) }
                    .glassButton(prominent: true).disabled(saving)
                Button("Create it and open the schedule") { Task { await create(openSchedule: true) } }.glassButton().disabled(saving)
            }
        }
    }

    private func create(openSchedule: Bool) async {
        guard let client = model.client else { return }
        saving = true
        let ok = await model.run { try await client.saveChannel(a.body(world)) } != nil
        saving = false
        guard ok else { return }
        await model.refreshChannels()
        dismiss()
        model.selection = (openSchedule || a.style == .weekly) ? .schedule : .channels
    }
}
