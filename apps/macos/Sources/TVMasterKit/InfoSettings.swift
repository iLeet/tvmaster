import Foundation

/// An info channel's pages, feeds, ticker and music, read from and written back to the server's JSON.
public struct InfoSettings: Equatable, Sendable {
    public struct Slide: Equatable, Sendable {
        /// `text`, `headlines`, `tonight`, `clock` or `photo`.
        public var kind = "text"
        public var title = ""
        public var lines: [String] = []
        public var picture = ""
        /// How long this page stays up; `nil` = the channel's own time.
        public var seconds: Int?
        public init() {}
    }

    public var about = ""
    public var title = ""
    public var slides: [Slide] = []
    public var seconds = 12
    public var feeds: [String] = []
    public var ticker: [String] = []
    public var tickerEnabled = true
    public var clock = true
    public var hour12 = true
    public var musicLibraryIds: [Int] = []

    public init() {}

    /// The kinds of page: id, name and what it is.
    public static let kinds: [(value: String, label: String, about: String)] = [
        ("text", "Notice", "A title and a few lines, and a picture if you like"),
        ("headlines", "Headlines", "The latest from the channel's news feeds"),
        ("tonight", "On tonight", "What is on now and next, on every channel"),
        ("clock", "Clock", "The time and the date, large"),
        ("photo", "Picture", "A picture across the whole screen, with a caption"),
    ]

    public init(info: JSONValue?) {
        let i = info
        func str(_ p: [String], _ d: String = "") -> String { i?.get(p)?.stringValue ?? d }
        about = str(["about"]); title = str(["title"])
        seconds = i?.get(["seconds"])?.intValue ?? 12
        feeds = i?.get(["feeds"])?.stringArray ?? []
        ticker = i?.get(["ticker"])?.stringArray ?? []
        tickerEnabled = i?.get(["ticker_enabled"])?.boolValue ?? true
        clock = i?.get(["clock"])?.boolValue ?? true
        hour12 = i?.get(["hour12"])?.boolValue ?? true
        musicLibraryIds = (i?.get(["music_library_ids"])?.arrayValue ?? []).compactMap(\.intValue)
        slides = (i?.get(["slides"])?.arrayValue ?? []).map { s in
            var x = Slide()
            x.kind = s.get(["kind"])?.stringValue ?? "text"
            x.title = s.get(["title"])?.stringValue ?? ""
            x.lines = s.get(["lines"])?.stringArray ?? []
            x.picture = s.get(["picture"])?.stringValue ?? ""
            x.seconds = s.get(["seconds"])?.intValue
            return x
        }
    }

    public static func newSlide(_ kind: String) -> Slide {
        var s = Slide()
        s.kind = kind
        switch kind {
        case "headlines": s.title = "Headlines"
        case "tonight": s.title = "On tonight"
        case "text": s.title = "Notice"
        default: break
        }
        return s
    }

    /// What the server would refuse, said before it does.
    public var problem: String? {
        if slides.contains(where: { $0.kind == "headlines" }) && feeds.isEmpty { return "A headlines page needs at least one news feed: add one under Headlines." }
        if let n = slides.firstIndex(where: { $0.kind == "photo" && $0.picture.trimmingCharacters(in: .whitespaces).isEmpty }) { return "Page \(n + 1) is a picture page and needs a picture." }
        if let n = slides.firstIndex(where: { $0.kind == "text" && $0.title.trimmingCharacters(in: .whitespaces).isEmpty && $0.lines.allSatisfy { $0.trimmingCharacters(in: .whitespaces).isEmpty } }) { return "Page \(n + 1) is a notice with nothing in it." }
        if let f = feeds.first(where: { !($0.hasPrefix("http://") || $0.hasPrefix("https://")) || $0.contains(where: \.isWhitespace) }) { return "“\(f)” is not a web address: news feeds start with http:// or https://" }
        return nil
    }

    /// `info` with these settings written in, everything else kept.
    public func apply(to info: JSONValue?) -> JSONValue {
        var o = info ?? .object([:])
        let opt: (String) -> JSONValue = { $0.trimmingCharacters(in: .whitespaces).isEmpty ? .null : .string($0.trimmingCharacters(in: .whitespaces)) }
        o.set(["about"], opt(about))
        o.set(["title"], opt(title))
        o.set(["seconds"], .number(Double(seconds)))
        o.set(["feeds"], .array(feeds.map { .string($0.trimmingCharacters(in: .whitespaces)) }.filter { $0 != .string("") }))
        o.set(["ticker"], .array(ticker.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }.map { .string($0) }))
        o.set(["ticker_enabled"], .bool(tickerEnabled))
        o.set(["clock"], .bool(clock))
        o.set(["hour12"], .bool(hour12))
        o.set(["music_library_ids"], .array(musicLibraryIds.map { .number(Double($0)) }))
        o.set(["slides"], .array(slides.map { s in
            .object([
                "kind": .string(s.kind), "title": .string(s.title),
                "lines": .array(s.lines.map { .string($0) }),
                "picture": opt(s.picture),
                "seconds": s.seconds.map { .number(Double($0)) } ?? .null,
            ])
        }))
        return o
    }

    /// Move the item at `from` to `to` (a copy; unchanged when `to` is out of range).
    public static func moved<T>(_ list: [T], from: Int, to: Int) -> [T] {
        guard list.indices.contains(from), list.indices.contains(to), from != to else { return list }
        var out = list
        out.insert(out.remove(at: from), at: to)
        return out
    }
}
