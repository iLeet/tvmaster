import Foundation

public enum Format {
    /// "22m", "1h 05m", "45s"
    public static func duration(_ ms: Int64) -> String {
        let s = max(0, Int((Double(ms) / 1000).rounded()))
        if s < 60 { return "\(s)s" }
        let m = s / 60
        if m < 60 { return "\(m)m" }
        let h = m / 60, rem = m % 60
        return rem == 0 ? "\(h)h" : String(format: "%dh %02dm", h, rem)
    }

    /// A slug usable as a channel or TV id: lowercase letters, digits, - and _.
    public static func slug(_ name: String) -> String {
        let folded = name.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: .current).lowercased()
        var out = ""
        var lastDash = false
        for ch in folded {
            if ch.isASCII && (ch.isLetter || ch.isNumber) {
                out.append(ch); lastDash = false
            } else if !lastDash && !out.isEmpty {
                out.append("-"); lastDash = true
            }
        }
        while out.hasSuffix("-") { out.removeLast() }
        let trimmed = String(out.prefix(32))
        let clean = trimmed.hasSuffix("-") ? String(trimmed.dropLast()) : trimmed
        return clean.isEmpty ? "channel" : clean
    }

    public static func isValidID(_ id: String) -> Bool {
        guard let first = id.first, id.count <= 32 else { return false }
        func ok(_ c: Character) -> Bool { (c.isASCII && c.isLowercase && c.isLetter) || (c.isASCII && c.isNumber) }
        return ok(first) && id.allSatisfy { ok($0) || $0 == "-" || $0 == "_" }
    }

    public static func nextFreeNumber(taken: [Int], start: Int = 2) -> Int {
        var n = start
        while taken.contains(n) { n += 1 }
        return n
    }
}
