import Foundation

/// A whole appearance in one choice: the station theme, the screens and how guides are laid out.
public struct LookInfo: Identifiable, Equatable, Sendable {
    public let id: String
    public let name: String
    public let about: String
    public let builtin: Bool
    /// The look as the server wrote it, so it can be exported and imported unchanged.
    public let raw: JSONValue
    /// A whole guide setting to draw a thumbnail from.
    public let previewGuide: JSONValue
    public let theme: String
    public let style: String
    /// Which screens it designs.
    public let screens: [String]

    init(json j: JSONValue) {
        id = j.get(["id"])?.stringValue ?? ""
        name = j.get(["name"])?.stringValue ?? id
        about = j.get(["about"])?.stringValue ?? ""
        builtin = j.get(["builtin"])?.boolValue ?? false
        raw = j
        previewGuide = j.get(["preview_guide"]) ?? .object([:])
        theme = j.get(["theme", "theme"])?.stringValue ?? "auto"
        style = j.get(["theme", "style"])?.stringValue ?? "retro"
        if case .object(let o)? = j.get(["screens"]) { screens = o.keys.sorted() } else { screens = [] }
    }

    /// The look as a file you can keep or share (the same object `import` takes back).
    public var exportJSON: Data {
        var o: [String: JSONValue] = [:]
        for k in ["name", "about", "theme", "guide", "screens"] { if let v = raw.get([k]) { o[k] = v } }
        let any = (try? JSONEncoder().encode(JSONValue.object(o))).flatMap { try? JSONSerialization.jsonObject(with: $0) } ?? [:]
        return (try? JSONSerialization.data(withJSONObject: any, options: [.prettyPrinted, .sortedKeys])) ?? Data()
    }
}

/// What applying a look changes.
public struct LookParts: Equatable, Sendable {
    public var theme = true
    public var screens = true
    public var guides = true
    public init() {}
}

extension TVMasterClient {
    public func looks() async throws -> [LookInfo] {
        let data = try await send("GET", "/looks")
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return [] }
        return (JSONValue(any: any).get(["looks"])?.arrayValue ?? []).map { LookInfo(json: $0) }
    }

    public func applyLook(_ id: String, parts: LookParts) async throws {
        _ = try await send("POST", "/looks/\(id)/apply", body: .object(["theme": .bool(parts.theme), "screens": .bool(parts.screens), "guides": .bool(parts.guides)]))
    }

    /// Save how the station looks now as a look of your own.
    public func saveLook(name: String, about: String) async throws {
        _ = try await send("POST", "/looks", body: .object(["name": .string(name), "about": .string(about)]))
    }

    /// Take in a look that was exported (its JSON).
    public func importLook(_ json: Data) async throws {
        guard let any = try? JSONSerialization.jsonObject(with: json) else { throw TVMasterError.invalidResponse }
        _ = try await send("POST", "/looks/import", body: JSONValue(any: any))
    }

    public func deleteLook(_ id: String) async throws {
        _ = try await send("DELETE", "/looks/\(id)")
    }

    /// One frame of a guide, from a whole guide setting.
    public func guidePreview(guide: JSONValue, t: Double = 2) async throws -> Data {
        try await send("POST", "/guide/preview", body: .object(["name": .string("Guide"), "id": .string("preview"), "guide": guide, "t": .number(t)]))
    }
}
