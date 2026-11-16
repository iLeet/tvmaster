import AVKit
import Combine
import SwiftUI
import TVMasterKit

/// AVKit's player view with floating controls and picture-in-picture.
struct PlayerSurface: NSViewRepresentable {
    var player: AVPlayer?

    func makeNSView(context: Context) -> AVPlayerView {
        let v = AVPlayerView()
        v.controlsStyle = .floating
        v.allowsPictureInPicturePlayback = true
        v.showsFullScreenToggleButton = true
        v.videoGravity = .resizeAspect
        return v
    }

    func updateNSView(_ v: AVPlayerView, context: Context) {
        if v.player !== player { v.player = player }
    }
}

/// Owns the AVPlayer and follows the status of the current stream.
@MainActor
@Observable
final class PlayerController {
    enum Status: Equatable { case idle, loading, playing, failed(String) }

    private(set) var player: AVPlayer?
    private(set) var status: Status = .idle
    private var channelID: String?
    private var observers: Set<AnyCancellable> = []

    func play(channel: Channel, url: URL) {
        if channelID == channel.id, player != nil, status != .idle { return }
        channelID = channel.id
        stop(keepChannel: true)
        status = .loading
        let item = AVPlayerItem(url: url)
        item.preferredForwardBufferDuration = 6
        let p = AVPlayer(playerItem: item)
        p.automaticallyWaitsToMinimizeStalling = true
        item.publisher(for: \.status).receive(on: DispatchQueue.main).sink { [weak self] s in
            switch s {
            case .readyToPlay: self?.status = .playing
            case .failed: self?.status = .failed(item.error?.localizedDescription ?? "The stream failed")
            default: break
            }
        }.store(in: &observers)
        player = p
        p.play()
    }

    func stop(keepChannel: Bool = false) {
        player?.pause()
        player?.replaceCurrentItem(with: nil)
        player = nil
        observers.removeAll()
        if !keepChannel { channelID = nil; status = .idle }
    }

    func retry(channel: Channel, url: URL) {
        channelID = nil
        play(channel: channel, url: url)
    }
}

struct WatchView: View {
    @Environment(AppModel.self) private var model
    @State private var controller = PlayerController()
    @State private var showChannels = true

    var body: some View {
        HStack(spacing: 0) {
            ZStack {
                Color.black
                PlayerSurface(player: controller.player)
                overlay
            }
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
            .padding(.leading, 18).padding(.vertical, 18).padding(.trailing, showChannels ? 10 : 18)

            if showChannels {
                channelList.frame(width: 250).padding(.trailing, 18).padding(.vertical, 18)
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(.snappy, value: showChannels)
        .toolbar {
            ToolbarItemGroup {
                Button { model.zap(-1) } label: { Label("Previous channel", systemImage: "chevron.up") }
                    .keyboardShortcut(.upArrow, modifiers: .command)
                Button { model.zap(1) } label: { Label("Next channel", systemImage: "chevron.down") }
                    .keyboardShortcut(.downArrow, modifiers: .command)
                Button { showChannels.toggle() } label: { Label("Channel list", systemImage: "sidebar.right") }
            }
        }
        .onAppear(perform: start)
        .onChange(of: model.watchingID) { _, _ in start() }
        .onChange(of: model.client?.baseURL) { _, _ in controller.stop(); start() }
        .onDisappear { controller.stop() }
    }

    private func start() {
        guard model.client != nil, let ch = model.watching else { controller.stop(); return }
        Task {
            guard let url = await model.playableURL(for: ch) else { return }
            // The channel may have changed while the relay started.
            if model.watching?.id == ch.id { controller.play(channel: ch, url: url) }
        }
    }

    @ViewBuilder private var overlay: some View {
        VStack {
            if let ch = model.watching {
                HStack(spacing: 10) {
                    NumberBadge(number: ch.number)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(ch.name).fontWeight(.semibold)
                        Text(subtitle(ch)).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                }
                .padding(10).frame(maxWidth: 420, alignment: .leading).glassCard(cornerRadius: 16).padding(14)
            }
            Spacer()
            switch controller.status {
            case .loading:
                VStack(spacing: 10) { ProgressView(); Text("Tuning in…").font(.callout) }
                    .padding(18).glassCard(cornerRadius: 18)
                Spacer()
            case .failed(let message):
                VStack(spacing: 10) {
                    Label("This channel isn't available", systemImage: "exclamationmark.triangle").font(.headline)
                    Text(message).font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Button("Try again") { if let ch = model.watching { Task { if let url = await model.playableURL(for: ch) { controller.retry(channel: ch, url: url) } } } }.glassButton()
                }
                .padding(20).frame(maxWidth: 360).glassCard(cornerRadius: 20)
                Spacer()
            case .idle where model.watching == nil:
                Text("Pick a channel").foregroundStyle(.secondary)
                Spacer()
            default: EmptyView()
            }
        }
        .allowsHitTesting(controller.status != .playing)
        .opacity(controller.status == .playing ? 0 : 1)
        .animation(.easeOut(duration: 0.4), value: controller.status)
    }

    private func subtitle(_ ch: Channel) -> String {
        if ch.isGuide { return "Live guide" }
        if ch.isTest { return "Test pattern" }
        guard let now = ch.now else { return "" }
        return now.subtitle.map { "\(now.headline) — \($0)" } ?? now.headline
    }

    private var channelList: some View {
        ScrollView {
            GlassGroup(spacing: 8) {
                VStack(spacing: 8) {
                    ForEach(model.channels) { ch in
                        Button { model.watch(ch) } label: {
                            HStack(spacing: 10) {
                                NumberBadge(number: ch.number)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(ch.name).fontWeight(.semibold).lineLimit(1)
                                    Text(ch.isGuide ? "Live guide" : (ch.now?.headline ?? "—")).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                }
                                Spacer(minLength: 0)
                            }
                            .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(RoundedRectangle(cornerRadius: 16))
                            .glassCard(cornerRadius: 16, tint: ch.id == model.watchingID ? .accentColor : nil)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .scrollIndicators(.hidden)
    }
}
