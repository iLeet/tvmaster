import AppKit
import SwiftUI
import TVMasterKit

/// Every address for using this server from other apps and devices, with copy,
/// share and QR code. The addresses come from what the server says about itself.
struct ShareView: View {
    @Environment(AppModel.self) private var model
    @State private var options: ShareLinks.Options?
    @State private var qrFor: ShareLinks.Link?
    @State private var copied: String?

    private var links: ShareLinks? {
        guard let client = model.client else { return nil }
        return ShareLinks(network: model.system?.network, token: client.token, baseURL: client.baseURL)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                PageHeader(title: "Share", subtitle: "Addresses for other apps, TVs and phones")
                if let links {
                    let o = options ?? links.defaultOptions
                    controls(links, o)
                    notices(links, o)
                    ForEach(links.sections(o, channels: model.channels, tvs: model.tvs)) { section in
                        SectionCard(section: section, copied: $copied, qrFor: $qrFor)
                    }
                } else {
                    Text("Connect to a server to see its addresses.").foregroundStyle(.secondary)
                }
            }
            .padding(28).frame(maxWidth: 940, alignment: .leading).frame(maxWidth: .infinity)
        }
        .sheet(item: $qrFor) { link in QRSheet(link: link) }
    }

    // MARK: controls

    @ViewBuilder private func controls(_ links: ShareLinks, _ o: ShareLinks.Options) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 18) {
                Picker("Address", selection: Binding(get: { o.host }, set: { var n = o; n.host = $0; options = n })) {
                    ForEach(links.hosts, id: \.self) { Text($0).tag($0) }
                }
                .frame(maxWidth: 320)
                if links.schemes.count > 1 {
                    Picker("Connection", selection: Binding(get: { o.scheme }, set: { var n = o; n.scheme = $0; options = n })) {
                        ForEach(links.schemes) { Text($0.label).tag($0) }
                    }
                    .pickerStyle(.segmented).labelsHidden().frame(width: 160)
                }
                if links.token != nil {
                    Toggle("Include access token", isOn: Binding(get: { o.includeToken }, set: { var n = o; n.includeToken = $0; options = n }))
                }
            }
            if links.token != nil && o.includeToken {
                Label("Links with the token in them can control your server. Share them like a password.", systemImage: "key.fill")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard()
    }

    @ViewBuilder private func notices(_ links: ShareLinks, _ o: ShareLinks.Options) -> some View {
        if model.client?.isLocal == true && !model.shareOnNetwork {
            VStack(alignment: .leading, spacing: 8) {
                Label("This server only accepts connections from this Mac.", systemImage: "network.slash").font(.headline)
                Text("To use these addresses on other devices, let the server listen on your network. TVMaster will protect it with an access token.")
                    .foregroundStyle(.secondary)
                Button("Share on my network") {
                    model.shareOnNetwork = true
                    Task { await model.restartLocalServer() }
                }.glassButton(prominent: true)
            }
            .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard(tint: .orange)
        }
        if let tls = model.system?.network?.tls {
            if tls.enabled, o.scheme == .https, let fp = tls.fingerprint {
                VStack(alignment: .leading, spacing: 6) {
                    Label(tls.selfSigned ? "HTTPS uses a self-signed certificate." : "HTTPS uses your certificate.", systemImage: "lock.fill").font(.headline)
                    if tls.selfSigned {
                        Text("Browsers show a warning to click through once, and IPTV apps and Plex usually refuse self-signed certificates: use HTTP for those. The Mac app and its fingerprint check work fine. Fingerprint:")
                            .foregroundStyle(.secondary)
                        Text(fp).font(.caption.monospaced()).textSelection(.enabled)
                    }
                }
                .padding(16).frame(maxWidth: .infinity, alignment: .leading).glassCard()
            } else if !tls.enabled && model.client?.isLocal == true {
                HStack {
                    Label("HTTPS is off. It encrypts the access token on your network.", systemImage: "lock.open")
                    Spacer()
                    Button("Turn on HTTPS") {
                        model.serveHTTPS = true
                        Task { await model.restartLocalServer() }
                    }.glassButton()
                }
                .padding(16).glassCard()
            }
        }
    }
}

private struct SectionCard: View {
    let section: ShareLinks.Section
    @Binding var copied: String?
    @Binding var qrFor: ShareLinks.Link?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(section.title.uppercased()).font(.caption.weight(.semibold)).foregroundStyle(.secondary).tracking(1.2)
            if let note = section.note { Text(note).font(.callout).foregroundStyle(.secondary) }
            if section.links.isEmpty {
                Text("Nothing here yet.").foregroundStyle(.secondary)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(section.links.enumerated()), id: \.element.id) { i, link in
                        if i > 0 { Divider().opacity(0.4) }
                        LinkRow(link: link, copied: $copied, qrFor: $qrFor)
                    }
                }
                .padding(.horizontal, 14).padding(.vertical, 4).glassCard()
            }
        }
    }
}

private struct LinkRow: View {
    let link: ShareLinks.Link
    @Binding var copied: String?
    @Binding var qrFor: ShareLinks.Link?

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(link.title).font(.body.weight(.semibold))
                    if link.hasToken { Image(systemName: "key.fill").font(.caption2).foregroundStyle(.secondary).help("This link contains your access token") }
                }
                Text(link.detail).font(.caption).foregroundStyle(.secondary)
                Text(link.url).font(.caption.monospaced()).textSelection(.enabled).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 8)
            if link.qr {
                Button { qrFor = link } label: { Image(systemName: "qrcode") }.buttonStyle(.borderless).help("Show a QR code")
            }
            if let url = URL(string: link.url), link.url.contains("://") {
                ShareLink(item: url) { Image(systemName: "square.and.arrow.up") }.buttonStyle(.borderless).help("Share…")
            }
            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(link.url, forType: .string)
                copied = link.id
                Task { try? await Task.sleep(nanoseconds: 1_500_000_000); if copied == link.id { copied = nil } }
            } label: {
                Label(copied == link.id ? "Copied" : "Copy", systemImage: copied == link.id ? "checkmark" : "doc.on.doc")
                    .labelStyle(.titleAndIcon).frame(minWidth: 72)
            }
            .glassButton()
        }
        .padding(.vertical, 9)
    }
}

private struct QRSheet: View {
    @Environment(\.dismiss) private var dismiss
    let link: ShareLinks.Link

    var body: some View {
        VStack(spacing: 14) {
            Text(link.title).font(.title3.bold())
            if let image = QRImage.image(for: link.url, size: 280) {
                Image(nsImage: image).interpolation(.none).resizable().frame(width: 280, height: 280)
                    .padding(12).background(.white, in: RoundedRectangle(cornerRadius: 12))
            }
            Text("Point a phone's camera at this.").foregroundStyle(.secondary)
            Text(link.url).font(.caption.monospaced()).textSelection(.enabled).lineLimit(2).truncationMode(.middle).frame(maxWidth: 320)
            Button("Done") { dismiss() }.glassButton(prominent: true).keyboardShortcut(.defaultAction)
        }
        .padding(26).frame(width: 380)
    }
}

enum QRImage {
    /// A crisp QR code for `text`, `size` points square.
    static func image(for text: String, size: CGFloat) -> NSImage? {
        guard let cg = QRCode.cgImage(for: text, size: size) else { return nil }
        return NSImage(cgImage: cg, size: NSSize(width: size, height: size))
    }
}
