import Foundation

/// One look for everything the station draws: guide, cards, bumpers, stand-by screen, alerts and graphics.
public struct ThemeChoice: Equatable, Sendable {
    public var theme = "auto"
    public var primary = ""
    public var accent = ""
    public var background = ""
    /// `retro` or `modern`.
    public var style = "retro"

    public init() {}

    /// Read a theme from the server's JSON (`nil` for a missing or null one).
    public init?(json: JSONValue?) {
        guard let json, case .object = json else { return nil }
        theme = json.get(["theme"])?.stringValue ?? "auto"
        primary = json.get(["colors", "primary"])?.stringValue ?? ""
        accent = json.get(["colors", "accent"])?.stringValue ?? ""
        background = json.get(["colors", "background"])?.stringValue ?? ""
        style = json.get(["style"])?.stringValue ?? "retro"
    }

    public var json: JSONValue {
        func opt(_ s: String) -> JSONValue { s.isEmpty ? .null : .string(s) }
        return .object([
            "theme": .string(theme),
            "colors": .object(["primary": opt(primary), "accent": opt(accent), "background": opt(background)]),
            "style": .string(style),
        ])
    }
}

/// The themes and styles the server offers.
public struct ThemeCatalogue: Decodable, Sendable {
    public struct Item: Decodable, Identifiable, Sendable {
        public let id: String
        public let name: String
        public let about: String
        /// Three colours (`#rrggbb`): background, main, highlight. Absent for a style.
        public let swatch: [String]?
    }
    public let themes: [Item]
    public let styles: [Item]
}

/// Where a new station stands.
public struct SetupState: Decodable, Sendable {
    public let needed: Bool
    public let items: Int
    public let shows: Int
    public let channels: Int
    public let stationFolder: Bool
    public let canUpload: Bool
}
