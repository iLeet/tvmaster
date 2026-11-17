import SwiftUI
import TVMasterKit

/// A channel's weekly time slots, specials, gap programming and commercial breaks.
struct ProgrammingForm: View {
    @Environment(AppModel.self) private var model
    @Binding var ls: LineupSettings
    var shows: [String]
    var libraries: [Library]

    private let dayNames = ["M", "T", "W", "T", "F", "S", "S"]

    var body: some View {
        Form {
            Section("Time slots") {
                Text("A weekly lineup. Each slot airs its show between the times you give. Time outside every slot follows “When nothing is scheduled”.")
                    .font(.caption).foregroundStyle(.secondary)
                ForEach($ls.slots) { $slot in
                    SlotEditor(slot: $slot, shows: shows) { ls.slots.removeAll { $0.id == slot.id } }
                }
                Button { ls.slots.append(TimeSlot()) } label: { Label("Add a time slot", systemImage: "plus") }
            }

            Section("Seasons") {
                Text("Part of the year with its own time slots (a summer schedule). While a season is on, its slots replace the ones above.")
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
                Button { ls.seasons.append(SeasonSlot()) } label: { Label("Add a season", systemImage: "plus") }
            }

            Section("Specials") {
                Text("A marathon or holiday special that takes over for a stretch and keeps its own episode counts.").font(.caption).foregroundStyle(.secondary)
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

            Section("Between slots") {
                Toggle("Choose what airs", isOn: Binding(get: { ls.fill != nil }, set: { on in ls.fill = on ? SourceChoice() : nil }))
                if ls.fill != nil {
                    SourcePicker(source: Binding(get: { ls.fill ?? SourceChoice() }, set: { ls.fill = $0 }), shows: shows)
                }
                Text(ls.fill == nil ? "Automatic: follows the channel's strategy, or the test screen when you chose that for when a show runs out." : "Airs whenever no slot is on.")
                    .font(.caption).foregroundStyle(.secondary)
            }

            Section("Timing") {
                Picker("Start times snap to", selection: Binding(get: { ls.gridMinutes ?? 0 }, set: { ls.gridMinutes = $0 == 0 ? nil : $0 })) {
                    Text("Automatic").tag(0)
                    ForEach(LineupSettings.grids, id: \.self) { Text("\($0) minutes").tag($0) }
                }
                Toggle("After downtime, act as if the channel kept airing", isOn: $ls.catchUpAdvances)
                Text("Off: carry on from where it left off (the usual choice). On: episodes advance while the server was off.").font(.caption).foregroundStyle(.secondary)
            }

            Section("Commercials and bumpers") {
                Toggle("Commercial breaks and bumpers", isOn: Binding(get: { ls.ads != nil }, set: { on in ls.ads = on ? AdSettings() : nil }))
                if let ads = ls.ads {
                    let commercial = libraries.filter { $0.kind == .commercials }
                    let clips = libraries.filter { $0.kind == .bumpers }
                    Toggle("Air commercials (off = bumpers only)", isOn: adBinding(\.commercials, ads))
                    if commercial.isEmpty { Text("Add a library of kind “Commercials” to draw ads from.").font(.caption).foregroundStyle(.secondary) }
                    ForEach(commercial) { lib in
                        Toggle(lib.name + " (\(lib.items))", isOn: Binding(get: { (ls.ads?.libraryIds ?? []).contains(lib.id) }, set: { on in toggle(lib.id, on, \.libraryIds) }))
                    }
                    ForEach(clips) { lib in
                        Toggle("Bumper clips: " + lib.name, isOn: Binding(get: { (ls.ads?.bumperLibraryIds ?? []).contains(lib.id) }, set: { on in toggle(lib.id, on, \.bumperLibraryIds) }))
                    }
                    Stepper("Break length \(ads.podSeconds) s", value: adBinding(\.podSeconds, ads), in: 20...900, step: 10)
                    Stepper("Most breaks per show: \(ads.maxBreaks)", value: adBinding(\.maxBreaks, ads), in: 0...8)
                    Stepper("No repeat within \(ads.noRepeat) plays", value: adBinding(\.noRepeat, ads), in: 1...200)
                    Stepper("Ad minutes per hour (no time grid): \(ads.minutesPerHour)", value: adBinding(\.minutesPerHour, ads), in: 0...30)
                    Toggle("Breaks inside shows (off = all ads after)", isOn: adBinding(\.midBreaks, ads))
                    Toggle("“We'll be right back”", isOn: adBinding(\.brb, ads))
                    Toggle("“Back to the show”", isOn: adBinding(\.backTo, ads))
                    Toggle("“Up next”", isOn: adBinding(\.upNext, ads))
                    Toggle("Station ID", isOn: adBinding(\.stationId, ads))
                    Stepper("“Later today” every \(ads.laterTodayEvery) slots (0 = never)", value: adBinding(\.laterTodayEvery, ads), in: 0...12)
                }
            }

            if let p = ls.problem {
                Section { Label(p, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange).font(.callout) }
            }
        }
        .formStyle(.grouped)
    }

    private func adBinding<T>(_ key: WritableKeyPath<AdSettings, T>, _ current: AdSettings) -> Binding<T> {
        Binding(get: { ls.ads?[keyPath: key] ?? current[keyPath: key] }, set: { ls.ads?[keyPath: key] = $0 })
    }

    private func toggle(_ id: Int, _ on: Bool, _ key: WritableKeyPath<AdSettings, [Int]>) {
        guard var a = ls.ads else { return }
        var ids = a[keyPath: key]
        if on { if !ids.contains(id) { ids.append(id) } } else { ids.removeAll { $0 == id } }
        a[keyPath: key] = ids
        ls.ads = a
    }
}

/// One show, several, anything, or a rotation left as it is.
struct SourcePicker: View {
    @Binding var source: SourceChoice
    var shows: [String]
    /// Rotations can take turns between shows, but not between rotations.
    var allowRotation = true

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Picker("Airs", selection: $source.kind) {
                ForEach(SourceChoice.Kind.allCases.filter { ($0 != .other || source.kind == .other) && ($0 != .rotation || allowRotation || source.kind == .rotation) }) { Text($0.label).tag($0) }
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
            Toggle("One episode only (then the rest of the slot is a gap, like the test screen)", isOn: $slot.once).font(.callout)
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
