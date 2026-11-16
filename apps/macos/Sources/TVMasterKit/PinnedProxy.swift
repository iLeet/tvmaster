import Foundation
import Network

/// AVPlayer cannot be told to trust a pinned self-signed certificate, so for
/// those servers the app plays through this tiny loopback relay: AVPlayer talks
/// plain HTTP to `127.0.0.1`, and the relay fetches the same path from the real
/// server with the app's pinned-certificate session.
///
/// It only forwards `GET` and `HEAD` for `/stream/...`, buffers each response
/// (HLS playlists and two-second segments are small), and listens on loopback
/// only. It adds no credentials: the access token is part of the URL AVPlayer
/// was given, so another program on this Mac gains nothing from it.
public final class PinnedProxy: @unchecked Sendable {
    private let upstream: URL
    private let session: URLSession
    private var listener: NWListener?
    private let queue = DispatchQueue(label: "tvmaster.pinned-proxy")
    public private(set) var port: UInt16 = 0

    public init(upstream: URL, session: URLSession) {
        self.upstream = upstream
        self.session = session
    }

    /// Start listening on an ephemeral loopback port; returns once it is ready.
    @discardableResult
    public func start() async throws -> UInt16 {
        if port != 0 { return port }
        let params = NWParameters.tcp
        params.requiredInterfaceType = .loopback
        let l = try NWListener(using: params, on: .any)
        listener = l
        l.newConnectionHandler = { [weak self] conn in self?.accept(conn) }
        return try await withCheckedThrowingContinuation { (cont: CheckedContinuation<UInt16, Error>) in
            let once = Once()
            l.stateUpdateHandler = { [weak self, weak l] state in
                switch state {
                case .ready:
                    if let p = l?.port?.rawValue, once.first() { self?.port = p; cont.resume(returning: p) }
                case .failed(let e):
                    if once.first() { cont.resume(throwing: e) }
                default: break
                }
            }
            l.start(queue: queue)
        }
    }

    public func stop() {
        listener?.cancel()
        listener = nil
        port = 0
    }

    /// The loopback URL that serves what `url` serves on the real server.
    public func localURL(for url: URL) -> URL {
        var c = URLComponents(url: url, resolvingAgainstBaseURL: false) ?? URLComponents()
        c.scheme = "http"
        c.host = "127.0.0.1"
        c.port = Int(port)
        return c.url ?? url
    }

    // MARK: connections

    private func accept(_ conn: NWConnection) {
        conn.start(queue: queue)
        read(conn, buffer: Data())
    }

    private func read(_ conn: NWConnection, buffer: Data) {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 16 * 1024) { [weak self] data, _, done, error in
            guard let self else { return }
            var buf = buffer
            if let data { buf.append(data) }
            if let end = buf.range(of: Data("\r\n\r\n".utf8)) {
                let head = String(decoding: buf[..<end.lowerBound], as: UTF8.self)
                Task { await self.respond(to: head, on: conn) }
            } else if done || error != nil || buf.count > 64 * 1024 {
                conn.cancel()
            } else {
                self.read(conn, buffer: buf)
            }
        }
    }

    /// Parse the request line and the headers worth forwarding.
    static func parse(head: String) -> (method: String, target: String, headers: [String: String])? {
        let lines = head.components(separatedBy: "\r\n")
        let parts = (lines.first ?? "").split(separator: " ")
        guard parts.count >= 2 else { return nil }
        var headers: [String: String] = [:]
        for l in lines.dropFirst() {
            guard let i = l.firstIndex(of: ":") else { continue }
            headers[l[..<i].lowercased()] = l[l.index(after: i)...].trimmingCharacters(in: .whitespaces)
        }
        return (String(parts[0]), String(parts[1]), headers)
    }

    private func respond(to head: String, on conn: NWConnection) async {
        guard let (method, target, headers) = Self.parse(head: head) else { return send(502, "Bad request", on: conn) }
        guard method == "GET" || method == "HEAD", target.hasPrefix("/stream/") else {
            return send(403, "Only stream requests are relayed", on: conn)
        }
        guard let url = URL(string: target, relativeTo: upstream)?.absoluteURL else { return send(400, "Bad path", on: conn) }
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.timeoutInterval = 40
        for h in ["range", "if-none-match", "if-modified-since"] { if let v = headers[h] { req.setValue(v, forHTTPHeaderField: h) } }
        do {
            let (body, response) = try await session.data(for: req)
            guard let http = response as? HTTPURLResponse else { return send(502, "No response", on: conn) }
            var out = "HTTP/1.1 \(http.statusCode) \(HTTPURLResponse.localizedString(forStatusCode: http.statusCode))\r\n"
            for h in ["Content-Type", "Cache-Control", "Content-Range", "Accept-Ranges", "ETag", "Last-Modified"] {
                if let v = http.value(forHTTPHeaderField: h) { out += "\(h): \(v)\r\n" }
            }
            out += "Content-Length: \(body.count)\r\nConnection: close\r\n\r\n"
            var payload = Data(out.utf8)
            if method != "HEAD" { payload.append(body) }
            conn.send(content: payload, completion: .contentProcessed { _ in conn.cancel() })
        } catch {
            send(502, "Upstream error: \(error.localizedDescription)", on: conn)
        }
    }

    private func send(_ status: Int, _ message: String, on conn: NWConnection) {
        let body = Data(message.utf8)
        let head = "HTTP/1.1 \(status) \(HTTPURLResponse.localizedString(forStatusCode: status))\r\nContent-Type: text/plain\r\nContent-Length: \(body.count)\r\nConnection: close\r\n\r\n"
        conn.send(content: Data(head.utf8) + body, completion: .contentProcessed { _ in conn.cancel() })
    }
}

/// Lets a callback run its body once.
private final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false
    func first() -> Bool { lock.lock(); defer { lock.unlock() }; if done { return false }; done = true; return true }
}
