import Foundation

/// A guide channel's settings, as the editor shows them. `apply` writes them into
/// the channel's raw `guide` JSON with the server's own key names and keeps
/// anything the editor does not show.
public struct GuideSettings: Equatable, Sendable {
    public var hours = 3
    public var title = ""
    public var ticker: [String] = []
    public var musicLibraryIds: [Int] = []

    public var theme = "auto"
    public var colorPrimary = ""
    public var colorAccent = ""
    public var colorBackground = ""

    // How it is drawn.
    public var typeface = "pixel"
    public var corners = "square"
    public var textSize = "medium"
    public var backgroundStyle = "gradient"
    public var backgroundImage = ""
    public var backgroundDim = 45
    public var headerStyle = "bar"
    public var tickerPosition = "bottom"
    public var cellTimes = false
    public var nowMarker = true

    public var style = "grid"
    public var rows = 6
    public var scroll = "page"
    public var pageSeconds = 9
    public var timeStep = 30
    public var channelNumbers = true
    public var channelNames = true
    public var progress = false
    public var scanlines = true
    /// Only these channels, in this order; empty = every programming channel.
    public var channels: [String] = []

    public var headerTitle = true
    public var headerClock = true
    public var headerDate = true
    public var hour12 = true
    public var logo = ""

    public var tickerEnabled = true
    public var tickerSpeed = "normal"
    public var tickerNowOn = true

    public var pipMode = "off"
    public var pipChannel = ""
    public var pipChannels: [String] = []
    public var pipSeconds = 30
    public var pipPlacement = "side_right"
    public var pipSize = "medium"
    public var pipAudio = "music"
    public var pipInfo = true


    /// One-click looks: colours and style together.
    public struct Preset: Identifiable, Sendable {
        public let id: String
        public let label: String
        public let help: String
        public let apply: @Sendable (inout GuideSettings) -> Void
    }

    public static let presets: [Preset] = [
        Preset(id: "retro", label: "Retro", help: "The original: pixel letters, scanlines, square cells.") { g in
            g.theme = "auto"; g.typeface = "pixel"; g.corners = "square"; g.textSize = "medium"; g.headerStyle = "bar"; g.backgroundStyle = "gradient"; g.scanlines = true
        },
        Preset(id: "modern-dark", label: "Modern dark", help: "Smooth letters on midnight blue with rounded cells.") { g in
            g.theme = "midnight"; g.typeface = "smooth"; g.corners = "rounded"; g.textSize = "medium"; g.headerStyle = "bar"; g.backgroundStyle = "gradient"; g.scanlines = false
        },
        Preset(id: "modern-light", label: "Clean white", help: "Smooth letters on white, with programme times.") { g in
            g.theme = "snow"; g.typeface = "smooth"; g.corners = "rounded"; g.textSize = "medium"; g.headerStyle = "bar"; g.backgroundStyle = "gradient"; g.cellTimes = true; g.scanlines = false
        },
        Preset(id: "minimal", label: "Minimal", help: "Graphite, no header bar, big readable rows.") { g in
            g.theme = "graphite"; g.typeface = "smooth"; g.corners = "rounded"; g.textSize = "large"; g.headerStyle = "minimal"; g.backgroundStyle = "solid"; g.style = "now_next"; g.scanlines = false
        },
    ]

    public static let placements: [(value: String, label: String)] = [
        ("side_right", "Side, right"), ("side_left", "Side, left"), ("top_left", "Top, left"), ("top_right", "Top, right"),
    ]

    public init() {}

    /// Read the settings out of a channel's raw `guide` JSON.
    public init(guide g: JSONValue?) {
        func str(_ p: [String], _ d: String) -> String { g?.get(p)?.stringValue ?? d }
        func bool(_ p: [String], _ d: Bool) -> Bool { g?.get(p)?.boolValue ?? d }
        func int(_ p: [String], _ d: Int) -> Int { g?.get(p)?.intValue ?? d }
        hours = int(["hours"], 3)
        title = str(["title"], "")
        ticker = g?.get(["ticker"])?.stringArray ?? []
        musicLibraryIds = (g?.get(["music_library_ids"])?.arrayValue ?? []).compactMap(\.intValue)
        theme = str(["theme"], "auto")
        colorPrimary = str(["colors", "primary"], "")
        colorAccent = str(["colors", "accent"], "")
        colorBackground = str(["colors", "background"], "")
        typeface = str(["look", "typeface"], "pixel")
        corners = str(["look", "corners"], "square")
        textSize = str(["look", "text_size"], "medium")
        backgroundStyle = str(["look", "background", "style"], "gradient")
        backgroundImage = str(["look", "background", "image"], "")
        backgroundDim = int(["look", "background", "dim"], 45)
        headerStyle = str(["look", "header_style"], "bar")
        tickerPosition = str(["look", "ticker_position"], "bottom")
        cellTimes = bool(["look", "cell_times"], false)
        nowMarker = bool(["look", "now_marker"], true)
        style = str(["layout", "style"], "grid")
        rows = int(["layout", "rows"], 6)
        scroll = str(["layout", "scroll"], "page")
        pageSeconds = int(["layout", "page_seconds"], 9)
        timeStep = int(["layout", "time_step"], 30)
        channelNumbers = bool(["layout", "channel_numbers"], true)
        channelNames = bool(["layout", "channel_names"], true)
        progress = bool(["layout", "progress"], false)
        scanlines = bool(["layout", "scanlines"], true)
        channels = g?.get(["channels"])?.stringArray ?? []
        headerTitle = bool(["header", "title"], true)
        headerClock = bool(["header", "clock"], true)
        headerDate = bool(["header", "date"], true)
        hour12 = bool(["header", "hour12"], true)
        logo = str(["header", "logo"], "")
        tickerEnabled = bool(["ticker_options", "enabled"], true)
        tickerSpeed = str(["ticker_options", "speed"], "normal")
        tickerNowOn = bool(["ticker_options", "now_on"], true)
        pipMode = str(["pip", "mode"], "off")
        pipChannel = str(["pip", "channel"], "")
        pipChannels = g?.get(["pip", "channels"])?.stringArray ?? []
        pipSeconds = int(["pip", "seconds"], 30)
        pipPlacement = str(["pip", "placement"], "side_right")
        pipSize = str(["pip", "size"], "medium")
        pipAudio = str(["pip", "audio"], "music")
        pipInfo = bool(["pip", "info"], true)
    }

    /// What the video window still needs before the server will take the guide.
    public var problem: String? {
        if backgroundStyle == "image" && backgroundImage.isEmpty { return "Choose a picture for the background." }
        if pipMode == "channel" && pipChannel.isEmpty { return "Choose the channel to show in the video window." }
        if theme == "custom" {
            for c in [colorPrimary, colorAccent, colorBackground] where !c.isEmpty && !Self.isHexColor(c) { return "Colours look like #3366ff." }
        }
        return nil
    }

    public static func isHexColor(_ s: String) -> Bool {
        let t = s.trimmingCharacters(in: .whitespaces)
        return t.count == 7 && t.hasPrefix("#") && t.dropFirst().allSatisfy(\.isHexDigit)
    }

    /// `guide` with these settings written in, everything else kept.
    public func apply(to guide: JSONValue?) -> JSONValue {
        var o = guide ?? .object([:])
        let strings: ([String]) -> JSONValue = { .array($0.map { .string($0) }) }
        let opt: (String) -> JSONValue = { $0.trimmingCharacters(in: .whitespaces).isEmpty ? .null : .string($0.trimmingCharacters(in: .whitespaces)) }
        o.set(["hours"], .number(Double(hours)))
        o.set(["title"], opt(title))
        o.set(["ticker"], strings(ticker.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }))
        o.set(["music_library_ids"], .array(musicLibraryIds.map { .number(Double($0)) }))
        o.set(["theme"], .string(theme))
        o.set(["colors", "primary"], opt(colorPrimary))
        o.set(["colors", "accent"], opt(colorAccent))
        o.set(["colors", "background"], opt(colorBackground))
        o.set(["look", "typeface"], .string(typeface))
        o.set(["look", "corners"], .string(corners))
        o.set(["look", "text_size"], .string(textSize))
        o.set(["look", "background", "style"], .string(backgroundStyle))
        o.set(["look", "background", "image"], opt(backgroundImage))
        o.set(["look", "background", "dim"], .number(Double(backgroundDim)))
        o.set(["look", "header_style"], .string(headerStyle))
        o.set(["look", "ticker_position"], .string(tickerPosition))
        o.set(["look", "cell_times"], .bool(cellTimes))
        o.set(["look", "now_marker"], .bool(nowMarker))
        o.set(["layout", "style"], .string(style))
        o.set(["layout", "rows"], .number(Double(rows)))
        o.set(["layout", "scroll"], .string(scroll))
        o.set(["layout", "page_seconds"], .number(Double(pageSeconds)))
        o.set(["layout", "time_step"], .number(Double(timeStep)))
        o.set(["layout", "channel_numbers"], .bool(channelNumbers))
        o.set(["layout", "channel_names"], .bool(channelNames))
        o.set(["layout", "progress"], .bool(progress))
        o.set(["layout", "scanlines"], .bool(scanlines))
        o.set(["channels"], strings(channels))
        o.set(["header", "title"], .bool(headerTitle))
        o.set(["header", "clock"], .bool(headerClock))
        o.set(["header", "date"], .bool(headerDate))
        o.set(["header", "hour12"], .bool(hour12))
        o.set(["header", "logo"], opt(logo))
        o.set(["ticker_options", "enabled"], .bool(tickerEnabled))
        o.set(["ticker_options", "speed"], .string(tickerSpeed))
        o.set(["ticker_options", "now_on"], .bool(tickerNowOn))
        o.set(["pip", "mode"], .string(pipMode))
        o.set(["pip", "channel"], opt(pipChannel))
        o.set(["pip", "channels"], strings(pipChannels))
        o.set(["pip", "seconds"], .number(Double(pipSeconds)))
        o.set(["pip", "placement"], .string(pipPlacement))
        o.set(["pip", "size"], .string(pipSize))
        o.set(["pip", "audio"], .string(pipAudio))
        o.set(["pip", "info"], .bool(pipInfo))
        return o
    }

    /// Add `id` to the end of an ordered list, or take it out if it is there.
    public static func toggled(_ list: [String], _ id: String) -> [String] {
        list.contains(id) ? list.filter { $0 != id } : list + [id]
    }
}
