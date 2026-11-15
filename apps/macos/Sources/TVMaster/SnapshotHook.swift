import AppKit
import SwiftUI

/// Developer aid: with `TVM_SNAPSHOT_DIR` set the app visits each screen, writes a PNG of the
/// window, and (with `TVM_SNAPSHOT_QUIT=1`) quits.
@MainActor
enum SnapshotHook {
    static var directory: URL? {
        ProcessInfo.processInfo.environment["TVM_SNAPSHOT_DIR"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    static func run(model: AppModel) async {
        guard let dir = directory else { return }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        while model.phase != .ready { try? await Task.sleep(for: .milliseconds(300)) }
        try? await Task.sleep(for: .seconds(2))
        for item in SidebarItem.allCases {
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
