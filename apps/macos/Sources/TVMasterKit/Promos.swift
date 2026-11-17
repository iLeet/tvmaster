import Foundation

/// A spot made in the app: what it says and how it looks and sounds.
public struct PromoDraft: Equatable, Sendable {
    public var kind = "sponsor"
    public var title = ""
    public var tagline = ""
    public var contact = ""
    public var slides: [String] = [""]
    public var seconds = 15
    public var picture = ""
    /// Its own look (nil = the station's theme).
    public var theme: ThemeChoice?
    public var look = "none"
    public var sound = "jingle"
    public var music = ""
    public var year: Int?

    public init() {}

    /// Enough to make a spot or a preview.
    public var ready: Bool {
        kind == "slides" ? slides.contains { !$0.trimmingCharacters(in: .whitespaces).isEmpty } : !title.trimmingCharacters(in: .whitespaces).isEmpty
    }

    public var json: JSONValue {
        var o: [String: JSONValue] = [
            "kind": .string(kind), "title": .string(title), "tagline": .string(tagline), "contact": .string(contact),
            "slides": .array(slides.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }.map { .string($0) }),
            "seconds": .number(Double(seconds)), "sound": .string(sound),
            "look": look == "none" ? .null : .string(look),
            "picture": picture.isEmpty ? .null : .string(picture),
            "music": music.isEmpty ? .null : .string(music),
            "year": year.map { .number(Double($0)) } ?? .null,
        ]
        if let theme { o["theme"] = theme.json }
        return .object(o)
    }
}

public struct PromoOptions: Decodable, Sendable {
    public struct Choice: Decodable, Identifiable, Sendable, Hashable {
        public let id: String
        public let name: String
        public let about: String?
    }
    public let kinds: [Choice]
    public let looks: [Choice]
    public let sounds: [Choice]
    public let seconds: [Int]
}

public struct PromoSpot: Decodable, Identifiable, Sendable, Hashable {
    public let id: Int
    public let name: String
    public let seconds: Int
    public let path: String
    public let library: Int
}

public struct PromoMusic: Decodable, Identifiable, Sendable, Hashable {
    public let path: String
    public let name: String
    public let seconds: Int
    public var id: String { path }
}
