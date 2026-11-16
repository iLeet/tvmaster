import SwiftUI
import TVMasterKit

struct HomeView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                PageHeader(title: "Home", subtitle: "What is on across the house right now")

                if let system = model.system, system.ffmpeg == nil {
                    FFmpegBanner()
                }

                GlassGroup {
                    HStack(spacing: 14) {
                        StatCard(title: "Channels", value: "\(model.channels.count)", detail: "\(model.system?.onAir.count ?? 0) on air now")
                        StatCard(title: "Library", value: model.libraries.reduce(0) { $0 + $1.items }.formatted(), detail: "files in \(model.libraries.count) libraries")
                        StatCard(title: "Server", value: model.system.map { "TVMaster \($0.version)" } ?? "—", detail: model.system?.ffmpeg.map { "FFmpeg \($0.version) · \($0.encoder ?? "software")" } ?? "FFmpeg missing", compact: true)
                    }
                }

                if model.libraries.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Welcome").font(.headline)
                        Text("Start by adding a folder of shows, then make a channel.").foregroundStyle(.secondary)
                        Button("Add your first library") { model.selection = .libraries }.glassButton(prominent: true)
                    }
                    .padding(18).frame(maxWidth: .infinity, alignment: .leading).glassCard()
                }

                Text("ON AIR").font(.caption.weight(.semibold)).foregroundStyle(.secondary).tracking(1.2)
                GlassGroup {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 14)], spacing: 14) {
                        ForEach(model.channels) { ChannelCard(channel: $0) }
                    }
                }
                if model.channels.isEmpty {
                    Text("No channels yet.").foregroundStyle(.secondary)
                    Button("Create a channel") { model.selection = .channels; model.showNewChannel = true }.glassButton()
                }

                if !model.tvs.isEmpty {
                    Text("TVS").font(.caption.weight(.semibold)).foregroundStyle(.secondary).tracking(1.2)
                    VStack(spacing: 0) {
                        ForEach(model.tvs) { tv in
                            HStack {
                                StatusDot(on: tv.online)
                                Text(tv.name).fontWeight(.semibold)
                                Spacer()
                                Text(tvStatus(tv)).foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 10).padding(.horizontal, 16)
                            if tv.id != model.tvs.last?.id { Divider().opacity(0.4) }
                        }
                    }
                    .glassCard()
                }
            }
            .padding(28)
        }
    }

    private func tvStatus(_ tv: Tv) -> String {
        guard tv.power else { return "Off" }
        if let n = tv.channel, let ch = model.channels.first(where: { $0.number == n }) { return "\(n) · \(ch.name)" }
        return tv.channel.map { "Channel \($0)" } ?? "No channel"
    }
}

struct StatCard: View {
    var title: String
    var value: String
    var detail: String
    var compact = false
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased()).font(.caption.weight(.semibold)).foregroundStyle(.secondary).tracking(1.1)
            Text(value).font(compact ? .title3.weight(.bold) : .system(size: 32, weight: .heavy, design: .rounded))
            Text(detail).font(.callout).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading).padding(18).glassCard()
    }
}

struct ChannelCard: View {
    @Environment(AppModel.self) private var model
    var channel: Channel

    var body: some View {
        Button { model.watch(channel) } label: {
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    NumberBadge(number: channel.number)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text(channel.name).fontWeight(.semibold).lineLimit(1)
                            if channel.isGuide { Text("GUIDE").font(.caption2.bold()).padding(.horizontal, 6).padding(.vertical, 2).background(.tint.opacity(0.2), in: Capsule()) }
                            if let g = channel.group { Text(g).font(.caption2).foregroundStyle(.secondary) }
                        }
                        Text(nowText).font(.callout).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    StatusDot(on: channel.onAir ?? false)
                }
                if let next = channel.next {
                    Text("Next \(next.start.formatted(date: .omitted, time: .shortened)): \(next.headline) · \(Format.duration(next.endMs - next.startMs))")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            .padding(16).frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(RoundedRectangle(cornerRadius: 22))
            .glassCard()
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Watch") { model.watch(channel) }
            Button("Schedule") { model.selection = .schedule }
            Button("Edit lineup in browser") { model.openInBrowser("/admin/#/channels/\(channel.id)?tab=programming") }
        }
    }

    private var nowText: String {
        if channel.isGuide { return "Live listing of every channel" }
        if channel.isTest { return "Test pattern" }
        guard let now = channel.now else { return "Nothing scheduled" }
        return now.subtitle.map { "\(now.headline) — \($0)" } ?? now.headline
    }
}

/// Shown while the server has no FFmpeg: offers to install it.
struct FFmpegBanner: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("FFmpeg isn't installed, so channels can't stream yet.", systemImage: "exclamationmark.triangle.fill")
                .font(.headline)
            if FFmpegSetup.brew() != nil {
                Text("TVMaster can install it for you with Homebrew. It takes a minute or two.")
                    .foregroundStyle(.secondary)
                HStack {
                    Button {
                        Task { await model.installFFmpeg() }
                    } label: {
                        if model.installingFFmpeg {
                            HStack(spacing: 8) { ProgressView().controlSize(.small); Text("Installing FFmpeg…") }
                        } else {
                            Text("Install FFmpeg")
                        }
                    }
                    .glassButton(prominent: true)
                    .disabled(model.installingFFmpeg)
                }
            } else {
                Text("Install Homebrew (brew.sh) and run `brew install ffmpeg`, or download FFmpeg yourself, then restart the server.")
                    .foregroundStyle(.secondary)
                HStack {
                    Button("Get Homebrew") { openURL(URL(string: "https://brew.sh")!) }.glassButton(prominent: true)
                    Button("Other ways to get FFmpeg") { openURL(FFmpegSetup.downloadPage) }.glassButton()
                }
            }
        }
        .padding().frame(maxWidth: .infinity, alignment: .leading).glassCard(tint: .orange)
    }
}
