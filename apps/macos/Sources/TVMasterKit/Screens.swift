import Foundation

/// How a layer arrives, and what it keeps doing. Same words the server and the web admin use.
public struct DesignAnim: Equatable, Sendable {
    public var enter = "fade"
    public var enterMs = 600
    public var delayMs = 0
    public var motion = "none"
    public var speed = 100
    public init() {}

    init(json: JSONValue?) {
        enter = json?.get(["enter"])?.stringValue ?? enter
        enterMs = json?.get(["enter_ms"])?.intValue ?? enterMs
        delayMs = json?.get(["delay_ms"])?.intValue ?? delayMs
        motion = json?.get(["motion"])?.stringValue ?? motion
        speed = json?.get(["speed"])?.intValue ?? speed
    }

    var json: JSONValue {
        .object(["enter": .string(enter), "enter_ms": .number(Double(enterMs)), "delay_ms": .number(Double(delayMs)),
                 "motion": .string(motion), "speed": .number(Double(speed))])
    }

    public static let enters: [(value: String, label: String)] = [
        ("none", "Just appears"), ("fade", "Fades in"), ("slide_up", "Slides up"), ("slide_down", "Slides down"),
        ("slide_left", "Slides in from the right"), ("slide_right", "Slides in from the left"), ("zoom", "Zooms in"),
        ("type", "Types itself out"), ("wipe", "Wipes across"),
    ]
    public static let motions: [(value: String, label: String)] = [
        ("none", "Stays still"), ("pulse", "Breathes"), ("float", "Floats"), ("drift", "Drifts around the screen"),
        ("blink", "Fades in and out"), ("marquee", "Scrolls across"),
    ]
}

/// One layer of a designed screen: words, a clock, a shape, a programme list or a picture.
public struct DesignLayer: Equatable, Identifiable, Sendable {
    public var id = UUID()
    /// `text`, `clock`, `shape`, `list` or `image`.
    public var kind = "text"
    public var x = 500
    public var y = 500
    public var align = "center"
    public var opacity = 100
    public var anim = DesignAnim()
    // words, clock, list
    public var text = ""
    public var size = 48
    public var bold = true
    public var pixel = false
    public var color = "ink"
    public var shadow = false
    public var width = 900
    public var lines = 2
    // clock
    public var hour12 = true
    public var seconds = false
    public var date = false
    // shape (and picture size)
    public var w = 500
    public var h = 250
    public var color2: String?
    public var radius = 0
    public var circle = false
    public var glass = false
    // list
    public var timeColor = "accent_text"
    public var rows = 4
    // picture
    public var path = ""
    public init() {}

    /// Two layers are the same when they draw the same (the id is only for the editor's list).
    public static func == (a: DesignLayer, b: DesignLayer) -> Bool { a.json == b.json }

    public init(kind: String) {
        self.kind = kind
        switch kind {
        case "clock": y = 700; size = 72
        case "shape": color = "accent"; radius = 32
        case "list": size = 44; width = 800
        case "image": w = 250; h = 250
        default: text = "Your words"; size = 64
        }
    }

    init(json j: JSONValue) {
        kind = j.get(["type"])?.stringValue ?? "text"
        x = j.get(["x"])?.intValue ?? 500
        y = j.get(["y"])?.intValue ?? 500
        align = j.get(["align"])?.stringValue ?? "center"
        opacity = j.get(["opacity"])?.intValue ?? 100
        anim = DesignAnim(json: j.get(["anim"]))
        text = j.get(["text"])?.stringValue ?? ""
        size = j.get(["size"])?.intValue ?? 48
        bold = j.get(["bold"])?.boolValue ?? true
        pixel = j.get(["pixel"])?.boolValue ?? false
        color = j.get(["color"])?.stringValue ?? (kind == "shape" ? "accent" : "ink")
        shadow = j.get(["shadow"])?.boolValue ?? false
        width = j.get(["width"])?.intValue ?? 900
        lines = j.get(["lines"])?.intValue ?? 2
        hour12 = j.get(["hour12"])?.boolValue ?? true
        seconds = j.get(["seconds"])?.boolValue ?? false
        date = j.get(["date"])?.boolValue ?? false
        w = j.get(["w"])?.intValue ?? 500
        h = j.get(["h"])?.intValue ?? 250
        color2 = j.get(["color2"])?.stringValue
        radius = j.get(["radius"])?.intValue ?? 0
        circle = j.get(["circle"])?.boolValue ?? false
        glass = j.get(["glass"])?.boolValue ?? false
        timeColor = j.get(["time_color"])?.stringValue ?? "accent_text"
        rows = j.get(["rows"])?.intValue ?? 4
        path = j.get(["path"])?.stringValue ?? ""
    }

    var json: JSONValue {
        var o: [String: JSONValue] = [
            "type": .string(kind), "x": .number(Double(x)), "y": .number(Double(y)), "align": .string(align),
            "opacity": .number(Double(opacity)), "anim": anim.json,
        ]
        func n(_ v: Int) -> JSONValue { .number(Double(v)) }
        switch kind {
        case "text":
            o["text"] = .string(text); o["size"] = n(size); o["bold"] = .bool(bold); o["pixel"] = .bool(pixel)
            o["color"] = .string(color); o["shadow"] = .bool(shadow); o["width"] = n(width); o["lines"] = n(lines)
        case "clock":
            o["size"] = n(size); o["bold"] = .bool(bold); o["color"] = .string(color); o["hour12"] = .bool(hour12)
            o["seconds"] = .bool(seconds); o["date"] = .bool(date)
        case "shape":
            o["w"] = n(w); o["h"] = n(h); o["color"] = .string(color); o["color2"] = color2.map { .string($0) } ?? .null
            o["radius"] = n(radius); o["circle"] = .bool(circle); o["glass"] = .bool(glass)
        case "list":
            o["size"] = n(size); o["color"] = .string(color); o["time_color"] = .string(timeColor); o["rows"] = n(rows); o["width"] = n(width)
        default:
            o["path"] = .string(path); o["w"] = n(w); o["h"] = n(h)
        }
        return .object(o)
    }

    public var title: String {
        switch kind {
        case "text": "Words: " + (text.isEmpty ? "(empty)" : String(text.prefix(28)))
        case "clock": "Clock"
        case "shape": glass ? "Shape: frosted glass" : "Shape"
        case "list": "Programme list"
        default: "Picture"
        }
    }
}

/// What is behind everything.
public struct DesignBackdrop: Equatable, Sendable {
    /// `theme`, `solid`, `gradient`, `aurora`, `bars`, `pattern` or `picture`.
    public var kind = "theme"
    public var color = "dark"
    public var top = "primary"
    public var bottom = "dark"
    public var colors: [String] = []
    public var pattern = "dots"
    public var bg = "dark"
    public var size = 48
    public var path = ""
    public init() {}

    init(json j: JSONValue?) {
        kind = j?.get(["type"])?.stringValue ?? "theme"
        color = j?.get(["color"])?.stringValue ?? (kind == "pattern" ? "primary" : "dark")
        top = j?.get(["top"])?.stringValue ?? "primary"
        bottom = j?.get(["bottom"])?.stringValue ?? "dark"
        colors = j?.get(["colors"])?.stringArray ?? []
        pattern = j?.get(["kind"])?.stringValue ?? "dots"
        bg = j?.get(["bg"])?.stringValue ?? "dark"
        size = j?.get(["size"])?.intValue ?? 48
        path = j?.get(["path"])?.stringValue ?? ""
    }

    var json: JSONValue {
        switch kind {
        case "solid": .object(["type": .string("solid"), "color": .string(color)])
        case "gradient": .object(["type": .string("gradient"), "top": .string(top), "bottom": .string(bottom)])
        case "aurora": .object(["type": .string("aurora"), "colors": .array(colors.map { .string($0) })])
        case "pattern": .object(["type": .string("pattern"), "kind": .string(pattern), "color": .string(color), "bg": .string(bg), "size": .number(Double(size))])
        case "picture": .object(["type": .string("picture"), "path": .string(path)])
        case "bars": .object(["type": .string("bars")])
        default: .object(["type": .string("theme")])
        }
    }

    public static let kinds: [(value: String, label: String)] = [
        ("theme", "Station theme"), ("aurora", "Soft colour"), ("gradient", "Gradient"), ("solid", "One colour"),
        ("pattern", "Pattern"), ("bars", "Colour bars"), ("picture", "My picture"),
    ]
}

/// A designed screen: a backdrop and layers with their own motion.
public struct ScreenDesign: Equatable, Sendable {
    public var name = ""
    public var backdrop = DesignBackdrop()
    public var layers: [DesignLayer] = []
    public var fadeOutMs = 0
    public init() {}

    public init(json j: JSONValue) {
        name = j.get(["name"])?.stringValue ?? ""
        backdrop = DesignBackdrop(json: j.get(["backdrop"]))
        layers = (j.get(["layers"])?.arrayValue ?? []).map { DesignLayer(json: $0) }
        fadeOutMs = j.get(["fade_out_ms"])?.intValue ?? 0
    }

    public var json: JSONValue {
        .object(["name": .string(name), "backdrop": backdrop.json, "layers": .array(layers.map(\.json)), "fade_out_ms": .number(Double(fadeOutMs))])
    }

    /// The same design apart from its name.
    public func same(as other: ScreenDesign) -> Bool {
        var a = self, b = other
        a.name = ""; b.name = ""
        return a.json == b.json
    }

    /// How lively the motion is, set on every layer at once (10-500 percent).
    public var motionSpeed: Int {
        get { layers.first?.anim.speed ?? 100 }
        set { let s = max(10, min(500, newValue)); for i in layers.indices { layers[i].anim.speed = s } }
    }

    public var hasMotion: Bool { layers.contains { $0.anim.motion != "none" } }

    public mutating func keepStill() { for i in layers.indices { layers[i].anim.motion = "none" } }

    /// The layers made of words, in order.
    public var wordLayers: [Int] { layers.indices.filter { layers[$0].kind == "text" } }

    public mutating func move(_ index: Int, by d: Int) {
        let j = index + d
        guard layers.indices.contains(index), layers.indices.contains(j) else { return }
        layers.swapAt(index, j)
    }
}

/// A design from the gallery.
public struct ScreenPreset: Identifiable, Equatable, Sendable {
    public let id: String
    public let name: String
    public let about: String
    public let design: ScreenDesign
}

/// One kind of screen (stand-by, up next, ...), its gallery and what is chosen.
public struct ScreenKindInfo: Identifiable, Equatable, Sendable {
    public let id: String
    public let name: String
    public let about: String
    public let words: [String]
    public let seconds: Int
    public let presets: [ScreenPreset]
    public let currentPreset: String?
    public let current: ScreenDesign?
    /// `none`, `station` or `channel`.
    public let source: String

    init(json j: JSONValue) {
        id = j.get(["id"])?.stringValue ?? ""
        name = j.get(["name"])?.stringValue ?? id
        about = j.get(["about"])?.stringValue ?? ""
        words = j.get(["words"])?.stringArray ?? []
        seconds = j.get(["seconds"])?.intValue ?? 0
        presets = (j.get(["presets"])?.arrayValue ?? []).map {
            ScreenPreset(id: $0.get(["id"])?.stringValue ?? "", name: $0.get(["name"])?.stringValue ?? "", about: $0.get(["about"])?.stringValue ?? "",
                         design: ScreenDesign(json: $0.get(["design"]) ?? .null))
        }
        if let c = j.get(["current"]), c != .null {
            currentPreset = c.get(["preset"])?.stringValue
            current = c.get(["design"]).map { ScreenDesign(json: $0) }
        } else {
            currentPreset = nil
            current = nil
        }
        source = j.get(["source"])?.stringValue ?? "none"
    }
}

/// Colour roles follow the station theme; anything starting with `#` is a colour of your own.
public enum DesignColour {
    public static let roles: [(value: String, label: String)] = [
        ("primary", "Theme main colour"), ("accent", "Theme highlight"), ("accent_text", "Theme highlight (easy to read)"),
        ("primary_text", "Theme main (easy to read)"), ("ink", "Theme text"), ("dark", "Theme dark"), ("bottom", "Theme darkest"),
        ("on_accent", "Text on the highlight"), ("white", "White"), ("black", "Black"),
    ]
    public static func isRole(_ c: String) -> Bool { roles.contains { $0.value == c } }
    /// `#rrggbb` (three-digit forms widened), or a fallback.
    public static func hex(_ c: String, fallback: String = "#ffffff") -> String {
        if c.range(of: "^#[0-9a-fA-F]{6}$", options: .regularExpression) != nil { return c }
        if c.range(of: "^#[0-9a-fA-F]{3}$", options: .regularExpression) != nil {
            let d = Array(c.dropFirst())
            return "#" + d.map { "\($0)\($0)" }.joined()
        }
        return fallback
    }
}

/// What a fill-in word means, for the editor's hint.
public let designWordHelp: [String: String] = [
    "channel": "the channel's name", "number": "its number", "message": "the stand-by message", "clock": "the time",
    "date": "today's date", "show": "the show that is coming back", "title": "the next programme", "time": "when it starts",
]

extension TVMasterClient {
    /// The screens, their galleries and what is chosen (for one channel, or the whole station).
    public func screens(channel: String?) async throws -> [ScreenKindInfo] {
        let data = try await send("GET", "/screens", query: channel.map { [URLQueryItem(name: "channel", value: $0)] } ?? [])
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return [] }
        return (JSONValue(any: any).get(["kinds"])?.arrayValue ?? []).map { ScreenKindInfo(json: $0) }
    }

    /// Choose a design for a screen.
    public func setScreen(kind: String, channel: String?, preset: String, design: ScreenDesign) async throws {
        _ = try await send("PUT", "/screens/\(kind)", query: channel.map { [URLQueryItem(name: "channel", value: $0)] } ?? [],
                           body: .object(["preset": .string(preset), "design": design.json]))
    }

    /// Back to the classic screen (or, for a channel, to the station's design).
    public func clearScreen(kind: String, channel: String?) async throws {
        _ = try await send("DELETE", "/screens/\(kind)", query: channel.map { [URLQueryItem(name: "channel", value: $0)] } ?? [])
    }

    /// One frame of a design as a PNG, `t` seconds in. Nothing goes on air.
    /// `design` nil = the classic screen.
    public func screenPreview(kind: String, design: ScreenDesign?, t: Double, channel: String?) async throws -> Data {
        var body: [String: JSONValue] = ["kind": .string(kind), "design": design?.json ?? .null, "t": .number(t)]
        if let channel { body["channel"] = .string(channel) }
        return try await send("POST", "/screens/preview", body: .object(body))
    }
}
