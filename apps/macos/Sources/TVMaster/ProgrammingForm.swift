import SwiftUI
import TVMasterKit

/// Everything about what plays when, in the order a person asks: how it plays, the week, what
/// happens in gaps, what happens when a show runs out, then the rarely-touched rest.
struct ProgrammingForm: View {
    @Environment(AppModel.self) private var model
    @Binding var ls: LineupSettings
    @Binding var strategy: Strategy
    @Binding var runOut: RunOut
    @Binding var chaos: Double
    @Binding var ps: PictureSoundSettings
    var shows: [String]
    var libraries: [Library]
    /// The amount of advertising in words, and a way to the Breaks tab.
    var breaksSummary: String
    var onBreaks: () -> Void

    private var weekly: Bool { ls.hasSchedule(strategy: strategy) }
    private var gap: GapChoice { ls.gapChoice(runOut: runOut) }

    var body: some View {
        Form {
            Section("How it plays") {
                Picker("Style", selection: Binding(get: { strategy }, set: { setStrategy($0) })) {
                    ForEach(Strategy.allCases) { Text($0.label).tag($0) }
                }
                Text(Self.strategyHelp(strategy)).font(.caption).foregroundStyle(.secondary)
                if strategy != .blockGrid && weekly {
                    Text("This channel also has timed slots, which take priority at their times.").font(.caption).foregroundStyle(.secondary)
                }
            }

            if weekly {
                Section("Your week") {
                    Text("Drag on the grid to place shows: “Weeknights at 8: sitcoms”, “Saturday morning cartoons”. Click a slot to choose what airs.")
                        .font(.caption).foregroundStyle(.secondary)
                    WeekGridView(slots: $ls.slots, shows: shows, step: ls.gridMinutes ?? 30)
                    Text("Drag on an empty spot to add a slot, drag a slot to move it, drag its bottom edge to resize it, click it to edit.").font(.caption).foregroundStyle(.secondary)
                    DisclosureGroup("The slots as a list (\(ls.slots.count))") {
                        ForEach($ls.slots) { $slot in
                            SlotEditor(slot: $slot, shows: shows) { ls.slots.removeAll { $0.id == slot.id } }
                        }
                        Button { ls.slots.append(TimeSlot()) } label: { Label("Add a time slot", systemImage: "plus") }
                    }
                }

                Section("When nothing is scheduled") {
                    Text("Time between your slots, and slots whose show has no episodes left.").font(.caption).foregroundStyle(.secondary)
                    Picker("Then the channel will", selection: Binding(get: { gap }, set: { ls.setGap($0, strategy: strategy, runOut: runOut) })) {
                        ForEach(GapChoice.allCases) { Text($0.label).tag($0) }
                    }
                    Text(gap.help).font(.caption).foregroundStyle(.secondary)
                    if gap == .these, ls.fill != nil {
                        SourcePicker(source: Binding(get: { ls.fill ?? SourceChoice() }, set: { ls.fill = $0 }), shows: shows, playOnly: true)
                    }
                    if gap == .screen { Text("Set what the screen looks like under “The off-air screen” below.").font(.caption).foregroundStyle(.secondary) }
                }
            }

            Section("When a show runs out of episodes") {
                Text("After its last unaired episode.").font(.caption).foregroundStyle(.secondary)
                Picker("Then the channel will", selection: Binding(get: { runOut.choice }, set: { runOut = $0 })) {
                    ForEach(RunOut.choices) { Text($0.label).tag($0) }
                }
                Text(runOut.choice.help).font(.caption).foregroundStyle(.secondary)
                if runOut == .hybrid {
                    Text("This channel was set to “hybrid”: something random for finished shows, and random shows in unscheduled time. Pick any choice above to replace that.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }

            Section("The off-air screen") {
                Text("What the channel shows whenever there is nothing to play: time nothing is scheduled (if you chose that), a finished show, sign-off hours, a file that cannot be played.")
                    .font(.caption).foregroundStyle(.secondary)
                Picker("Screen", selection: $ps.offAirScreen) { ForEach(PictureSoundSettings.screens, id: \.value) { Text($0.label).tag($0.value) } }
                if ps.offAirScreen == "card" { TextField("Message", text: $ps.offAirMessage, prompt: Text("PLEASE STAND BY")) }
                if ps.offAirScreen == "image" { PictureChooser(title: "Picture", path: $ps.offAirImage) }
                Toggle("Test tone (hiss with snow)", isOn: $ps.offAirTone)
            }

            Section("Commercial breaks") {
                HStack {
                    Text(breaksSummary).font(.callout).foregroundStyle(.secondary)
                    Spacer()
                    Button("Set up breaks…", action: onBreaks).glassButton()
                }
            }

            Section {
                MoreOptions(summary: moreSummary) {
                    if strategy != .chaos {
                        LabeledContent("Surprises \(Int(chaos))%") { Slider(value: $chaos, in: 0...100, step: 1) }
                        Text("Swaps this share of picks for something random, so an orderly channel now and then airs something unexpected. 0 = never.").font(.caption).foregroundStyle(.secondary)
                    }

                    Picker("Start times", selection: Binding(get: { ls.gridMinutes ?? 0 }, set: { ls.gridMinutes = $0 == 0 ? nil : $0 })) {
                        Text(weekly ? "Automatic (every 30 minutes)" : "Back to back").tag(0)
                        ForEach(LineupSettings.grids, id: \.self) { Text("Every \($0) minutes").tag($0) }
                    }
                    Toggle("After the server was off, skip ahead as if the channel kept airing", isOn: $ls.catchUpAdvances)
                    Text("Off: carry on from where it left off (the usual choice). On: episodes advance while the server was off.").font(.caption).foregroundStyle(.secondary)

                    Toggle("Go off the air overnight", isOn: $ps.signOffEnabled)
                    if ps.signOffEnabled {
                        HStack {
                            TextField("Off at", text: $ps.signOffFrom).frame(width: 90)
                            TextField("Back at", text: $ps.signOffTo).frame(width: 90)
                            Picker("", selection: $ps.signOffStyle) { ForEach(PictureSoundSettings.screens, id: \.value) { Text($0.label).tag($0.value) } }.labelsHidden()
                        }
                        Text("24-hour times like 02:00, in the channel's time zone. Bars and a tone (or snow) instead of programmes, and no episodes are used up.").font(.caption).foregroundStyle(.secondary)
                    }

                    seasonsSection
                    specialsSection
                }
            }

            if let p = ls.problem {
                Section { Label(p, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange).font(.callout) }
            }
        }
        .formStyle(.grouped)
        // Putting slots on the grid makes it a weekly schedule.
        .onChange(of: ls.slots.count) { old, new in if new > old && strategy != .blockGrid { strategy = .blockGrid } }
    }

    private var moreSummary: String {
        [strategy != .chaos ? "\(Int(chaos))% surprises" : "",
         ps.signOffEnabled ? "signs off overnight" : "",
         ls.seasons.isEmpty ? "" : "\(ls.seasons.count) seasonal change\(ls.seasons.count == 1 ? "" : "s")",
         ls.events.isEmpty ? "" : "\(ls.events.count) special\(ls.events.count == 1 ? "" : "s")"]
            .filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private func setStrategy(_ s: Strategy) {
        strategy = s
        // Choosing a weekly schedule means unscheduled time is a real gap: the off-air screen, unless a choice was made.
        if s == .blockGrid, ls.fill == nil, runOut != .offAir { ls.setGap(.screen, strategy: s, runOut: runOut) }
    }

    static func strategyHelp(_ s: Strategy) -> String {
        switch s {
        case .sequential: "Episodes in order, one show after another, remembering where each left off."
        case .shuffle: "Every episode once per pass, in random order."
        case .chaos: "Anything, at random."
        case .blockGrid: "A real TV schedule: shows at set times on set days, with seasonal changes and holiday specials."
        }
    }

    private var seasonsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Seasonal changes").font(.headline)
            Text("Part of the year with its own time slots (a summer schedule). While one is on, its slots replace the weekly ones. (Called “seasons” in the API.)")
                .font(.caption).foregroundStyle(.secondary)
            ForEach($ls.seasons) { $season in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        TextField("Name", text: $season.name, prompt: Text("Summer schedule")).textFieldStyle(.roundedBorder)
                        Button(role: .destructive) { ls.seasons.removeAll { $0.id == season.id } } label: { Image(systemName: "trash") }.buttonStyle(.borderless)
                    }
                    HStack {
                        MonthDayField(label: "From", value: $season.from)
                        MonthDayField(label: "To", value: $season.to)
                    }
                    ForEach($season.slots) { $slot in
                        SlotEditor(slot: $slot, shows: shows) { season.slots.removeAll { $0.id == slot.id } }
                    }
                    Button { season.slots.append(TimeSlot()) } label: { Label("Add a slot to this season", systemImage: "plus") }
                }
                .padding(10).background(.quaternary.opacity(0.2), in: RoundedRectangle(cornerRadius: 10))
            }
            Button { ls.seasons.append(SeasonSlot()) } label: { Label("Add a seasonal change", systemImage: "plus") }
        }
    }

    private var specialsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Holidays and specials").font(.headline)
            Text("A marathon or holiday special that takes over for a stretch and keeps its own episode counts. (Called “events” in the API.)").font(.caption).foregroundStyle(.secondary)
            ForEach($ls.events) { $ev in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        TextField("Name", text: $ev.name, prompt: Text("Halloween marathon")).textFieldStyle(.roundedBorder)
                        Button(role: .destructive) { ls.events.removeAll { $0.id == ev.id } } label: { Image(systemName: "trash") }.buttonStyle(.borderless)
                    }
                    HStack {
                        DateTimeField(label: "Starts", value: $ev.start)
                        DateTimeField(label: "Ends", value: $ev.end)
                    }
                    Toggle("Repeat every year", isOn: $ev.yearly)
                    SourcePicker(source: $ev.source, shows: shows)
                }
                .padding(10).background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 10))
            }
            Button {
                var e = EventSlot()
                let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd'T'HH:mm"
                e.start = f.string(from: Date()); e.end = f.string(from: Date().addingTimeInterval(6 * 3600))
                ls.events.append(e)
            } label: { Label("Add a special", systemImage: "plus") }
        }
    }
}

/// One show, several, anything, or a rotation left as it is.
struct SourcePicker: View {
    @Binding var source: SourceChoice
    var shows: [String]
    /// Rotations can take turns between shows, but not between rotations.
    var allowRotation = true
    /// Shows only: no "anything" and no off-air screen (what "play these shows" chooses).
    var playOnly = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Picker("Airs", selection: $source.kind) {
                ForEach(SourceChoice.Kind.allCases.filter { ($0 != .other || source.kind == .other) && ($0 != .rotation || allowRotation || source.kind == .rotation) && (!playOnly || ($0 != .chaos && $0 != .offAir)) }) { Text($0.label).tag($0) }
            }
            switch source.kind {
            case .show:
                Picker("Show", selection: $source.show) {
                    if !source.show.isEmpty && !shows.contains(source.show) { Text(source.show + " (not in a library)").tag(source.show) }
                    Text("Choose…").tag("")
                    ForEach(shows, id: \.self) { Text($0).tag($0) }
                }
            case .pool:
                Picker("Order", selection: $source.order) { Text("In turn, episodes in order").tag("sequential"); Text("Shuffled").tag("shuffle") }
                DisclosureGroup("Shows (\(source.shows.isEmpty ? "all" : "\(source.shows.count)"))") {
                    ForEach(shows, id: \.self) { name in
                        Toggle(name, isOn: Binding(get: { source.shows.contains(name) }, set: { on in
                            if on { source.shows.append(name) } else { source.shows.removeAll { $0 == name } }
                        }))
                    }
                    Text("None ticked means every show.").font(.caption).foregroundStyle(.secondary)
                }
            case .chaos:
                Text("Anything, at random.").font(.caption).foregroundStyle(.secondary)
            case .offAir:
                Text("The channel's off-air screen: title card, bars, snow or your picture. No programme plays.").font(.caption).foregroundStyle(.secondary)
            case .rotation:
                Picker("Next one takes over", selection: $source.advance) {
                    Text("Every few weeks").tag("weeks"); Text("When the current show finishes a full pass").tag("finale")
                }
                if source.advance == "weeks" { Stepper("Every \(source.weeks) week\(source.weeks == 1 ? "" : "s")", value: $source.weeks, in: 1...52) }
                ForEach(Array(source.items.enumerated()), id: \.offset) { i, _ in
                    HStack(alignment: .top) {
                        Text("\(i + 1)").foregroundStyle(.secondary).frame(width: 18)
                        SourcePicker(source: $source.items[i], shows: shows, allowRotation: false)
                        Button(role: .destructive) { source.items.remove(at: i) } label: { Image(systemName: "minus.circle") }.buttonStyle(.borderless)
                    }
                }
                Button { source.items.append({ var c = SourceChoice(); c.kind = .show; return c }()) } label: { Label("Add to the rotation", systemImage: "plus") }
            case .other:
                Text(source.summary).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// "HH:MM" as a time-of-day picker ("24:00" for midnight at the end of a slot).
struct TimeField: View {
    var label: String
    @Binding var value: String
    var allow24 = false

    private static let f: DateFormatter = { let f = DateFormatter(); f.dateFormat = "HH:mm"; f.locale = Locale(identifier: "en_US_POSIX"); return f }()

    var body: some View {
        if value == "24:00" {
            HStack {
                Text(label).frame(width: 50, alignment: .leading)
                Text("Midnight")
                Button("Change") { value = "23:00" }.buttonStyle(.link)
            }
        } else {
            HStack {
                DatePicker(label, selection: Binding(
                    get: { Self.f.date(from: value) ?? Date() },
                    set: { value = Self.f.string(from: $0) }), displayedComponents: .hourAndMinute)
                if allow24 { Button("Midnight") { value = "24:00" }.buttonStyle(.link).font(.caption) }
            }
        }
    }
}

/// "YYYY-MM-DDTHH:MM" as a date and time picker.
struct DateTimeField: View {
    var label: String
    @Binding var value: String
    private static let f: DateFormatter = { let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd'T'HH:mm"; f.locale = Locale(identifier: "en_US_POSIX"); return f }()

    var body: some View {
        DatePicker(label, selection: Binding(
            get: { Self.f.date(from: value) ?? Date() },
            set: { value = Self.f.string(from: $0) }))
    }
}


/// One weekly time slot.
struct SlotEditor: View {
    @Binding var slot: TimeSlot
    var shows: [String]
    var onDelete: () -> Void
    private let dayNames = ["M", "T", "W", "T", "F", "S", "S"]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                TextField("Name", text: $slot.name, prompt: Text("Weeknight sitcoms")).textFieldStyle(.roundedBorder)
                Button(role: .destructive, action: onDelete) { Image(systemName: "trash") }.buttonStyle(.borderless)
            }
            HStack(spacing: 4) {
                ForEach(0..<7, id: \.self) { d in
                    Toggle(dayNames[d], isOn: Binding(get: { slot.days.contains(d) }, set: { on in if on { slot.days.insert(d) } else { slot.days.remove(d) } }))
                        .toggleStyle(.button).controlSize(.small)
                }
                Text(LineupSettings.daysLabel(slot.days)).font(.caption).foregroundStyle(.secondary).padding(.leading, 6)
            }
            HStack {
                TimeField(label: "Starts", value: $slot.start)
                TimeField(label: "Ends", value: $slot.end, allow24: true)
            }
            SourcePicker(source: $slot.source, shows: shows)
            Toggle("One episode only (then the rest of the slot counts as unscheduled time)", isOn: $slot.once).font(.callout)
        }
        .padding(10).background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 10))
    }
}

/// "MM-DD" as a month and a day.
struct MonthDayField: View {
    var label: String
    @Binding var value: String
    private let months = Calendar.current.monthSymbols

    private var parts: (m: Int, d: Int) {
        let p = value.split(separator: "-").compactMap { Int($0) }
        return (p.count == 2 ? p[0] : 1, p.count == 2 ? p[1] : 1)
    }

    var body: some View {
        HStack {
            Text(label).frame(width: 40, alignment: .leading)
            Picker("", selection: Binding(get: { parts.m }, set: { value = String(format: "%02d-%02d", $0, parts.d) })) {
                ForEach(1...12, id: \.self) { Text(months[$0 - 1]).tag($0) }
            }.labelsHidden().frame(width: 130)
            Picker("", selection: Binding(get: { parts.d }, set: { value = String(format: "%02d-%02d", parts.m, $0) })) {
                ForEach(1...31, id: \.self) { Text("\($0)").tag($0) }
            }.labelsHidden().frame(width: 70)
        }
    }
}
