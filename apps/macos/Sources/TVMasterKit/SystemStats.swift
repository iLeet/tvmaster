import Foundation

/// What the server's machine is and how hard it is working (the same numbers the web admin shows).
public struct SystemStats: Equatable, Sendable {
    public struct Disk: Equatable, Sendable, Identifiable {
        public var id: String { mount }
        public let mount: String
        public let name: String
        public let total: Double
        public let free: Double
        public let usedFor: [String]
    }
    public let hostName: String
    public let os: String
    public let arch: String
    public let kernel: String
    public let cpuModel: String
    public let threads: Int
    public let cores: Int?
    public let uptime: Double
    public let container: Bool
    public let cpuPct: Double
    public let perCore: [Double]
    public let load: [Double]
    public let memTotal: Double
    public let memUsed: Double
    public let memAvailable: Double
    public let memLimitedByContainer: Bool
    public let swapTotal: Double
    public let swapUsed: Double
    public let serverCpuPct: Double?
    public let serverMemory: Double?
    public let serverUptime: Double?
    public let ffmpegProcesses: Int
    public let ffmpegCores: Double
    public let ffmpegMemory: Double
    public let encoder: String?
    public let onAir: [String]
    public let disks: [Disk]
    public let perChannelCores: Double
    public let capacityMeasured: Bool
    public let moreChannels: Int
    public let limitedBy: String
    public let version: String

    public init?(json j: JSONValue) {
        guard j.get(["host"]) != nil else { return nil }
        func d(_ p: [String]) -> Double { if case .number(let n)? = j.get(p) { return n }; return 0 }
        func s(_ p: [String]) -> String { j.get(p)?.stringValue ?? "" }
        hostName = s(["host", "name"])
        os = [s(["host", "os"]), s(["host", "os_version"])].filter { !$0.isEmpty }.joined(separator: " ")
        arch = s(["host", "arch"]); kernel = s(["host", "kernel"]); cpuModel = s(["host", "cpu_model"])
        threads = Int(d(["host", "threads"]))
        cores = j.get(["host", "cores"])?.intValue
        uptime = d(["host", "uptime_s"])
        container = j.get(["host", "container"])?.boolValue ?? false
        cpuPct = d(["cpu", "usage_pct"])
        perCore = (j.get(["cpu", "per_core"])?.arrayValue ?? []).compactMap { if case .number(let n) = $0 { return n }; return nil }
        load = (j.get(["cpu", "load"])?.arrayValue ?? []).compactMap { if case .number(let n) = $0 { return n }; return nil }
        memTotal = d(["memory", "total"]); memUsed = d(["memory", "used"]); memAvailable = d(["memory", "available"])
        memLimitedByContainer = j.get(["memory", "limited_by_container"])?.boolValue ?? false
        swapTotal = d(["memory", "swap_total"]); swapUsed = d(["memory", "swap_used"])
        if j.get(["tvmaster"]) != nil, j.get(["tvmaster"]) != .null {
            serverCpuPct = d(["tvmaster", "cpu_pct"]); serverMemory = d(["tvmaster", "memory"]); serverUptime = d(["tvmaster", "uptime_s"])
        } else { serverCpuPct = nil; serverMemory = nil; serverUptime = nil }
        ffmpegProcesses = Int(d(["ffmpeg", "processes"])); ffmpegCores = d(["ffmpeg", "cpu_cores"]); ffmpegMemory = d(["ffmpeg", "memory"])
        encoder = j.get(["encoder"])?.stringValue
        onAir = j.get(["on_air"])?.stringArray ?? []
        var list: [Disk] = []
        for item in j.get(["disks"])?.arrayValue ?? [] {
            var total = 0.0, free = 0.0
            if case .number(let n)? = item.get(["total"]) { total = n }
            if case .number(let n)? = item.get(["free"]) { free = n }
            list.append(Disk(mount: item.get(["mount"])?.stringValue ?? "", name: item.get(["name"])?.stringValue ?? "",
                             total: total, free: free, usedFor: item.get(["used_for"])?.stringArray ?? []))
        }
        disks = list
        perChannelCores = d(["capacity", "per_channel_cores"])
        capacityMeasured = j.get(["capacity", "measured"])?.boolValue ?? false
        moreChannels = Int(d(["capacity", "more_channels"]))
        limitedBy = s(["capacity", "limited_by"])
        version = s(["version"])
    }

    public static func bytes(_ n: Double) -> String {
        let gb = n / 1_073_741_824
        if gb >= 1024 { return String(format: "%.1f TB", gb / 1024) }
        if gb >= 1 { return String(format: "%.1f GB", gb) }
        return String(format: "%.0f MB", n / 1_048_576)
    }

    public static func duration(_ s: Double) -> String {
        let d = Int(s) / 86400, h = (Int(s) % 86400) / 3600, m = (Int(s) % 3600) / 60
        return d > 0 ? "\(d)d \(h)h" : h > 0 ? "\(h)h \(m)m" : "\(m)m"
    }

    /// How busy it is, by the worse of the processor and the load.
    public var busyWord: (word: String, level: Int) {
        let l = threads > 0 ? (load.first ?? 0) / Double(threads) : 0
        let worst = max(cpuPct / 100, l)
        return worst < 0.55 ? ("Plenty of room", 0) : worst < 0.85 ? ("Busy", 1) : ("Struggling: expect buffering", 2)
    }

    /// The guess at how many more channels fit, in a sentence.
    public var capacitySentence: String {
        let basis = capacityMeasured ? "at what each one is using now (about \(perChannelCores) of a core)" : "at the usual \(perChannelCores) of a core for a 720p channel"
        let limit = limitedBy == "memory" ? "memory" : "the processor"
        if moreChannels <= 0 { return "Probably no more live channels at once, \(basis). \(limitedBy == "memory" ? "Memory is the limit." : "The processor is the limit.")" }
        return "Room for about \(moreChannels) more live channel\(moreChannels == 1 ? "" : "s") at once, \(basis); \(limit) runs out first. A rough guess: channels only encode while someone is watching (or when set to Always on)."
    }
}

extension TVMasterClient {
    public func systemStats() async throws -> SystemStats? {
        let data = try await send("GET", "/system/stats")
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return SystemStats(json: JSONValue(any: any))
    }
}
