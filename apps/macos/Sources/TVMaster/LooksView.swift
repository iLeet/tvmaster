import AppKit
import SwiftUI
import TVMasterKit

/// A picture of a guide in a look.
private struct GuideThumb: View {
    @Environment(AppModel.self) private var model
    let guide: JSONValue
    @State private var image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(16.0 / 9.0, contentMode: .fit).clipShape(RoundedRectangle(cornerRadius: 8))
            } else {
                RoundedRectangle(cornerRadius: 8).fill(.black.opacity(0.4)).aspectRatio(16.0 / 9.0, contentMode: .fit).overlay(ProgressView().controlSize(.small))
            }
        }
        .task(id: guide) {
            guard let client = model.client else { return }
            if let data = try? await client.guidePreview(guide: guide), let img = NSImage(data: data) { image = img }
        }
    }
}

/// Whole appearances in one click: the colours and style, the screens, and how the guides are laid out.
struct LooksSection: View {
    @Environment(AppModel.self) private var model
    var onApplied: () async -> Void
    @State private var looks: [LookInfo] = []
    @State private var opened: String?
    @State private var parts = LookParts()
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Looks").font(.headline)
            Text("A whole appearance in one click: the colours and style, the stand-by and break screens, and how the guides are laid out. Pick one, then change anything you like below, on the Screens page or on a guide.")
                .font(.caption).foregroundStyle(.secondary)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 230), spacing: 12, alignment: .top)], alignment: .leading, spacing: 12) {
                ForEach(looks) { l in
                    VStack(alignment: .leading, spacing: 4) {
                        GuideThumb(guide: l.previewGuide)
                        HStack { Text(l.name).fontWeight(.semibold); Spacer(); if !l.builtin { Badge(text: "Mine") } }
                        Text(l.about).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                        Text("\(l.theme) · \(l.style)\(l.screens.isEmpty ? "" : " · \(l.screens.count) screens")").font(.caption2).foregroundStyle(.secondary)
                        if opened == l.id {
                            Toggle("Colours and style", isOn: $parts.theme)
                            Toggle("Stand-by and break screens", isOn: $parts.screens)
                            Toggle("Guide layouts", isOn: $parts.guides)
                            HStack {
                                Button("Use it") { Task { await apply(l) } }.glassButton(prominent: true)
                                Button("Cancel") { opened = nil }.glassButton()
                            }
                        } else {
                            HStack {
                                Button("Use this look…") { opened = l.id }.glassButton(prominent: true)
                                Button("Export") { export(l) }.glassButton()
                                if !l.builtin { Button("Delete", role: .destructive) { Task { await delete(l) } }.glassButton() }
                            }
                        }
                    }
                    .padding(8)
                    .overlay(RoundedRectangle(cornerRadius: 10).stroke(opened == l.id ? Color.accentColor : Color.secondary.opacity(0.25), lineWidth: opened == l.id ? 2 : 1))
                }
            }
            HStack {
                TextField("Name for the current look", text: $name).frame(maxWidth: 260)
                Button("Save how the station looks now") { Task { await save() } }.glassButton().disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                Button("Import a look…") { importLook() }.glassButton()
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(cornerRadius: 20)
        .task { await reload() }
    }

    private func reload() async { if let l = try? await model.client?.looks() { looks = l } }

    private func apply(_ l: LookInfo) async {
        guard let c = model.client else { return }
        if await model.run({ try await c.applyLook(l.id, parts: parts) }) != nil { opened = nil; await onApplied() }
    }

    private func save() async {
        guard let c = model.client else { return }
        let n = name.trimmingCharacters(in: .whitespaces)
        if await model.run({ try await c.saveLook(name: n, about: "") }) != nil { name = ""; await reload() }
    }

    private func delete(_ l: LookInfo) async {
        guard let c = model.client else { return }
        if await model.run({ try await c.deleteLook(l.id) }) != nil { await reload() }
    }

    private func export(_ l: LookInfo) {
        let p = NSSavePanel()
        p.nameFieldStringValue = l.name.lowercased().replacingOccurrences(of: " ", with: "-") + ".tvmaster-look.json"
        if p.runModal() == .OK, let url = p.url { try? l.exportJSON.write(to: url) }
    }

    private func importLook() {
        let p = NSOpenPanel()
        p.allowedContentTypes = [.json]
        guard p.runModal() == .OK, let url = p.url, let data = try? Data(contentsOf: url), let c = model.client else { return }
        Task { if await model.run({ try await c.importLook(data) }) != nil { await reload() } }
    }
}
