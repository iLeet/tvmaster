import SwiftUI

// Liquid Glass on macOS 26 and later; the system's translucent materials before that.

extension View {
    /// A floating glass panel.
    @ViewBuilder
    func glassCard(cornerRadius: CGFloat = 22, tint: Color? = nil) -> some View {
        if #available(macOS 26, *) {
            self.glassEffect(tint.map { Glass.regular.tint($0.opacity(0.25)) } ?? .regular, in: .rect(cornerRadius: cornerRadius))
        } else {
            self.background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous).strokeBorder(.white.opacity(0.12)))
        }
    }

    /// A glass capsule (pills, badges, overlays).
    @ViewBuilder
    func glassPill() -> some View {
        if #available(macOS 26, *) {
            self.glassEffect(.regular, in: .capsule)
        } else {
            self.background(.ultraThinMaterial, in: Capsule())
        }
    }

    @ViewBuilder
    func glassButton(prominent: Bool = false) -> some View {
        if #available(macOS 26, *) {
            if prominent { self.buttonStyle(.glassProminent) } else { self.buttonStyle(.glass) }
        } else {
            if prominent { self.buttonStyle(.borderedProminent) } else { self.buttonStyle(.bordered) }
        }
    }
}

/// Groups glass shapes so they blend and morph together on macOS 26.
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = 16
    @ViewBuilder var content: Content
    var body: some View {
        if #available(macOS 26, *) {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
    }
}

/// A soft colour field behind everything, so the glass has something to refract.
struct AppBackground: View {
    var body: some View {
        if #available(macOS 15, *) {
            MeshGradient(
                width: 3, height: 3,
                points: [[0, 0], [0.5, 0], [1, 0], [0, 0.5], [0.55, 0.45], [1, 0.5], [0, 1], [0.5, 1], [1, 1]],
                colors: [
                    Color(red: 0.10, green: 0.14, blue: 0.30), Color(red: 0.16, green: 0.12, blue: 0.34), Color(red: 0.08, green: 0.12, blue: 0.26),
                    Color(red: 0.09, green: 0.18, blue: 0.34), Color(red: 0.28, green: 0.16, blue: 0.40), Color(red: 0.12, green: 0.14, blue: 0.30),
                    Color(red: 0.05, green: 0.08, blue: 0.18), Color(red: 0.10, green: 0.10, blue: 0.24), Color(red: 0.06, green: 0.09, blue: 0.20),
                ]
            )
            .ignoresSafeArea()
        } else {
            LinearGradient(colors: [.indigo.opacity(0.5), .black], startPoint: .topLeading, endPoint: .bottomTrailing).ignoresSafeArea()
        }
    }
}

struct StatusDot: View {
    var on: Bool
    var body: some View {
        Circle().fill(on ? Color.green : Color.secondary.opacity(0.6)).frame(width: 9, height: 9)
            .shadow(color: on ? .green.opacity(0.8) : .clear, radius: 4)
            .accessibilityLabel(on ? "On" : "Off")
    }
}

struct NumberBadge: View {
    var number: Int
    var body: some View {
        Text("\(number)").font(.system(.headline, design: .rounded).weight(.heavy)).foregroundStyle(.tint)
            .frame(minWidth: 38, minHeight: 38).padding(.horizontal, 4)
            .background(.tint.opacity(0.18), in: RoundedRectangle(cornerRadius: 11, style: .continuous))
    }
}
