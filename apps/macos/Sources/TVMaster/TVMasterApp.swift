import AppKit
import SwiftUI
import TVMasterKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    var onQuit: (() async -> Void)?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let onQuit else { return .terminateNow }
        // Give the local server a moment to stop its encoders cleanly.
        Task { await onQuit(); sender.reply(toApplicationShouldTerminate: true) }
        return .terminateLater
    }
}

@main
struct TVMasterApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup("TVMaster", id: "main") {
            RootView()
                .environment(model)
                .task {
                    delegate.onQuit = { [model] in await model.shutdown() }
                    await SnapshotHook.run(model: model)
                }
        }
        .defaultSize(width: 1180, height: 760)
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(after: .sidebar) {
                ForEach(Array(SidebarItem.allCases.prefix(9).enumerated()), id: \.element) { i, item in
                    Button(item.title) { model.selection = item }.keyboardShortcut(KeyEquivalent(Character("\(i + 1)")), modifiers: .command)
                }
            }
            CommandGroup(after: .newItem) {
                Button("New Channel…") { model.selection = .channels; model.showNewChannel = true }
                    .keyboardShortcut("n", modifiers: [.command, .shift]).disabled(model.phase != .ready)
                Button("Refresh") { Task { await model.refreshAll() } }.keyboardShortcut("r", modifiers: .command)
            }
            CommandGroup(replacing: .help) {
                Button("Open the Web Admin") { model.openInBrowser("/admin/") }.disabled(model.client == nil)
            }
        }

        Settings { SettingsView().environment(model) }

        MenuBarExtra("TVMaster", systemImage: "tv") { MenuBarRemote().environment(model) }
            .menuBarExtraStyle(.window)
    }
}
