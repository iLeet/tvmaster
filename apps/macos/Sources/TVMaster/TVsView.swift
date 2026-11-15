import SwiftUI
import TVMasterKit

struct TVsView: View {
    @Environment(AppModel.self) private var model
    @State private var adding = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                PageHeader(title: "TVs", subtitle: "Screens around the house, each on its own channel") {
                    Button { adding = true } label: { Label("Add TV", systemImage: "plus") }.glassButton(prominent: true)
                }
                Text("Open \(model.client.map { "\($0.baseURL.absoluteString.trimmingCharacters(in: CharacterSet(charactersIn: "/")))/?tv=name" } ?? "the server") on a screen (a smart TV's browser, a tablet) and it registers itself. This Mac can be a TV too: turn it on in Settings.")
                    .font(.callout).foregroundStyle(.secondary)
                if model.tvs.isEmpty { Text("No TVs yet.").foregroundStyle(.secondary) }
                GlassGroup {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 320), spacing: 14)], spacing: 14) {
                        ForEach(model.tvs) { TVCard(tv: $0) }
                    }
                }
            }
            .padding(28)
        }
        .sheet(isPresented: $adding) { AddTVSheet() }
    }
}

struct TVCard: View {
    @Environment(AppModel.self) private var model
    var tv: Tv
    @State private var volume: Double = 50
    @State private var editing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                StatusDot(on: tv.online).help(tv.online ? "A player is connected" : "No player connected")
                Text(tv.name).font(.headline)
                Text(tv.id).font(.caption.monospaced()).foregroundStyle(.secondary)
                Spacer()
                Button(role: .destructive) { send { try await $0.deleteTv(tv.id) } } label: { Image(systemName: "trash") }.buttonStyle(.borderless)
            }
            HStack {
                Toggle("Power", isOn: Binding(get: { tv.power }, set: { on in send { try await $0.power(tv: tv.id, on: on) } })).toggleStyle(.switch)
                Spacer()
                Picker("Channel", selection: Binding(get: { tv.channel ?? 0 }, set: { n in if n > 0 { send { try await $0.tune(tv: tv.id, number: n) } } })) {
                    Text("Channel…").tag(0)
                    ForEach(model.channels) { Text("\($0.number) · \($0.name)").tag($0.number) }
                }.labelsHidden().frame(width: 180)
            }
            HStack {
                Button { send { try await $0.volume(tv: tv.id, muted: !tv.muted) } } label: { Image(systemName: tv.muted ? "speaker.slash.fill" : "speaker.wave.2.fill") }.glassButton()
                Slider(value: $volume, in: 0...100, step: 1) { editing = $0; if !$0 { let v = Int(volume); send { try await $0.volume(tv: tv.id, level: v) } } }
                Text("\(Int(volume))").monospacedDigit().foregroundStyle(.secondary).frame(width: 28)
            }
            HStack {
                Button { send { try await $0.step(tv: tv.id, up: false) } } label: { Label("Ch", systemImage: "chevron.down") }.glassButton()
                Button { send { try await $0.step(tv: tv.id, up: true) } } label: { Label("Ch", systemImage: "chevron.up") }.glassButton()
                Spacer()
                Button("Open player") { model.openInBrowser("/?tv=\(tv.id)") }.glassButton()
            }
        }
        .padding(16).glassCard(cornerRadius: 22)
        .onAppear { volume = Double(tv.volume) }
        .onChange(of: tv.volume) { _, v in if !editing { volume = Double(v) } }
    }

    private func send(_ action: @escaping @Sendable (TVMasterClient) async throws -> some Any) {
        guard let c = model.client else { return }
        Task { _ = await model.run { try await action(c) } }
    }
}

struct AddTVSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var id = ""
    @State private var idTouched = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add a TV").font(.title2.bold())
            TextField("Name", text: $name, prompt: Text("Living Room")).onChange(of: name) { _, n in if !idTouched { id = Format.slug(n) } }
            TextField("Id", text: $id).font(.body.monospaced()).onChange(of: id) { _, _ in }
            Text("The id is used in the player address: /?tv=\(id.isEmpty ? "living-room" : id)").font(.caption).foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Add") {
                    Task {
                        guard let c = model.client else { return }
                        if await model.run({ try await c.registerTv(id: id, name: name.isEmpty ? id : name) }) != nil {
                            model.tvs = (try? await c.tvs()) ?? model.tvs
                            dismiss()
                        }
                    }
                }.glassButton(prominent: true).keyboardShortcut(.defaultAction).disabled(!Format.isValidID(id))
            }
        }
        .padding(24).frame(width: 420)
    }
}
