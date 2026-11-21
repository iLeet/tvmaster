import SwiftUI

/// The less common settings of a section, folded away until asked for (or until "Show all options" is on).
/// The switch is in the sidebar and is remembered on this Mac.
struct MoreOptions<Content: View>: View {
    var title = "More options"
    var summary: String?
    @ViewBuilder var content: Content
    @AppStorage("showAllOptions") private var showAll = false
    @State private var open = false

    var body: some View {
        DisclosureGroup(isExpanded: Binding(get: { showAll || open }, set: { if !showAll { open = $0 } })) {
            content
        } label: {
            HStack(spacing: 8) {
                Text(title)
                if !(showAll || open), let summary { Text(summary).font(.caption).foregroundStyle(.secondary) }
            }
        }
    }
}
