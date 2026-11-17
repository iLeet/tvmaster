import AppKit
import TVMasterKit
import SwiftUI

/// Developer aid: with `TVM_SNAPSHOT_DIR` set the app visits each screen, writes a PNG of the
/// window, and (with `TVM_SNAPSHOT_QUIT=1`) quits.
@MainActor
enum SnapshotHook {
    static var directory: URL? {
        ProcessInfo.processInfo.environment["TVM_SNAPSHOT_DIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    /// Pictures of views that need no server: the weekly grid with a few slots.
    static func renderStandalone(_ dir: URL) {
        var a = TimeSlot(); a.name = "Weeknight sitcoms"; a.days = [0, 1, 2, 3, 4]; a.start = "20:00"; a.end = "21:00"; a.source.kind = .show; a.source.show = "Cheers"
        var b = TimeSlot(); b.days = [5]; b.start = "08:00"; b.end = "11:30"; b.source.kind = .chaos
        var c = TimeSlot(); c.days = [4]; c.start = "22:00"; c.end = "02:00"; c.source.kind = .pool; c.source.shows = ["Taxi", "Bebop"]
        let view = WeekGridView(slots: .constant([a, b, c]), shows: ["Cheers", "Taxi"], step: 30)
            .frame(width: 900, height: 460).padding(20).background(Color(nsColor: .windowBackgroundColor))
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(x: 0, y: 0, width: 940, height: 500)
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.contentView = host
        window.orderBack(nil)
        host.layoutSubtreeIfNeeded()
        RunLoop.current.run(until: Date().addingTimeInterval(0.6))
        host.layoutSubtreeIfNeeded()
        if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
            host.cacheDisplay(in: host.bounds, to: rep)
            if let png = rep.representation(using: .png, properties: [:]) { try? png.write(to: dir.appendingPathComponent("weekgrid.png")) }
        }
        window.orderOut(nil)
    }

    static func run(model: AppModel) async {
        guard let dir = directory else { return }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        if ProcessInfo.processInfo.environment["TVM_SNAPSHOT_GRID"] == "1" { renderStandalone(dir) }
        while model.phase != .ready { try? await Task.sleep(for: .milliseconds(300)) }
        try? await Task.sleep(for: .seconds(2))
        let only = ProcessInfo.processInfo.environment["TVM_SNAPSHOT_ONLY"]?.split(separator: ",").map(String.init)
        for item in SidebarItem.allCases where only == nil || only!.contains(item.rawValue) {
            model.selection = item
            try? await Task.sleep(for: .seconds(item == .watch ? 5 : 1.5))
            shoot(item.rawValue, dir)
        }
        model.selection = .channels
        model.showNewChannel = true
        try? await Task.sleep(for: .seconds(1.2))
        shoot("new-channel", dir)
        model.showNewChannel = false
        if ProcessInfo.processInfo.environment["TVM_SNAPSHOT_QUIT"] == "1" { NSApp.terminate(nil) }
    }

    private static func shoot(_ name: String, _ dir: URL) {
        for (i, window) in NSApp.windows.enumerated() where window.isVisible && window.contentView != nil {
            guard let view = window.contentView?.superview ?? window.contentView else { continue }
            guard let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { continue }
            view.cacheDisplay(in: view.bounds, to: rep)
            if let data = rep.representation(using: .png, properties: [:]) {
                try? data.write(to: dir.appendingPathComponent("\(name)-\(i).png"))
            }
        }
    }
}
