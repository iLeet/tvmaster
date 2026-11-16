import Foundation

/// A TVMaster server found on the local network.
public struct FoundServer: Identifiable, Hashable, Sendable {
    public var url: URL
    public var host: String
    /// What the server calls itself (its machine name), if it said.
    public var name: String?
    public var version: String?
    public var id: String { url.absoluteString }

    public var title: String { name.flatMap { $0.isEmpty ? nil : $0 } ?? host }
}

/// Looks for TVMaster servers on the Mac's own networks by asking each address on the
/// same subnet whether a TVMaster answers on its usual port. This finds a server whatever
/// it runs in (Docker, a package, the app) and needs nothing from the server but its
/// public health page.
public enum ServerDiscovery {
    public static let port = 8787

    /// A /24 around `ip`: every other address on it. Larger networks are cut to the /24 the
    /// Mac is in (a scan of thousands of addresses would be slow and rude).
    public static func candidates(ip: UInt32, mask: UInt32) -> [UInt32] {
        // Only private ranges: never scan a public address.
        let first = ip >> 24
        let second = (ip >> 16) & 255
        let isPrivate = first == 10 || (first == 172 && (16...31).contains(second)) || (first == 192 && second == 168)
        guard isPrivate, mask != 0 else { return [] }
        let base = ip & 0xFFFF_FF00
        // A network smaller than /24 (say /26) is only its own addresses.
        let net = ip & mask
        let bcast = net | ~mask
        return (1...254).map { base | UInt32($0) }.filter { $0 != ip && $0 >= net && $0 <= bcast && $0 != net && $0 != bcast }
    }

    public static func dotted(_ v: UInt32) -> String { "\(v >> 24).\((v >> 16) & 255).\((v >> 8) & 255).\(v & 255)" }

    /// This Mac's IPv4 addresses and netmasks on active, non-loopback interfaces.
    public static func localInterfaces() -> [(ip: UInt32, mask: UInt32)] {
        var out: [(UInt32, UInt32)] = []
        var head: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&head) == 0, let first = head else { return [] }
        defer { freeifaddrs(head) }
        for p in sequence(first: first, next: { $0.pointee.ifa_next }) {
            let f = p.pointee
            guard let addr = f.ifa_addr, let mask = f.ifa_netmask, addr.pointee.sa_family == sa_family_t(AF_INET),
                  Int32(f.ifa_flags) & IFF_UP != 0, Int32(f.ifa_flags) & IFF_LOOPBACK == 0 else { continue }
            let ip = addr.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { UInt32(bigEndian: $0.pointee.sin_addr.s_addr) }
            let m = mask.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { UInt32(bigEndian: $0.pointee.sin_addr.s_addr) }
            out.append((ip, m))
        }
        return out
    }

    /// What a server's health page says, if it is a TVMaster.
    public static func parse(health data: Data, host: String, port: Int = ServerDiscovery.port) -> FoundServer? {
        guard let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              o["status"] as? String == "ok", o["version"] is String, o["ffmpeg"] != nil || o["on_air"] != nil,
              let url = URL(string: "http://\(host):\(port)") else { return nil }
        return FoundServer(url: url, host: host, name: o["name"] as? String, version: o["version"] as? String)
    }

    /// Scan this Mac's networks. Calls `found` as each server answers; returns them all.
    @discardableResult
    public static func scan(timeout: TimeInterval = 0.8, found: (@Sendable (FoundServer) -> Void)? = nil) async -> [FoundServer] {
        var hosts: [String] = []
        for (ip, mask) in localInterfaces() { hosts += candidates(ip: ip, mask: mask).map(dotted) }
        hosts = Array(Set(hosts)).sorted()
        let cfg = URLSessionConfiguration.ephemeral
        cfg.timeoutIntervalForRequest = timeout
        cfg.timeoutIntervalForResource = timeout + 0.5
        cfg.waitsForConnectivity = false
        let session = URLSession(configuration: cfg)
        defer { session.invalidateAndCancel() }
        var results: [FoundServer] = []
        await withTaskGroup(of: FoundServer?.self) { group in
            var iterator = hosts.makeIterator()
            var running = 0
            func next() {
                guard let host = iterator.next() else { return }
                running += 1
                group.addTask {
                    guard let url = URL(string: "http://\(host):\(port)/api/v1/health"),
                          let (data, response) = try? await session.data(from: url),
                          (response as? HTTPURLResponse)?.statusCode == 200 else { return nil }
                    return parse(health: data, host: host)
                }
            }
            for _ in 0..<64 { next() }
            while let r = await group.next() {
                running -= 1
                if let r { results.append(r); found?(r) }
                next()
            }
        }
        return results.sorted { $0.host.compare($1.host, options: .numeric) == .orderedAscending }
    }
}
