import Foundation
import Security
import CryptoKit

/// SHA-256 of a certificate, as `AA:BB:...`: the same string the server prints
/// at start-up, shows in its web admin, and returns from `/api/v1/system`.
public enum Fingerprint {
    public static func of(der: Data) -> String {
        SHA256.hash(data: der).map { String(format: "%02X", $0) }.joined(separator: ":")
    }
}

/// Certificates the user has chosen to trust, by server (`host:port`).
///
/// A self-signed server certificate cannot be checked against any authority, so
/// the first time the app meets one it shows the fingerprint and asks. After
/// that the app accepts exactly that certificate and nothing else for that
/// server: if it ever changes, the user is asked again.
public final class PinStore: @unchecked Sendable {
    private let defaults: UserDefaults
    private let storageKey: String
    private let lock = NSLock()

    public init(defaults: UserDefaults = .standard, storageKey: String = "tvmaster.pinnedCertificates") {
        self.defaults = defaults
        self.storageKey = storageKey
    }

    public static func key(host: String, port: Int) -> String { "\(host.lowercased()):\(port)" }

    public static func key(for url: URL) -> String {
        key(host: url.host ?? "", port: url.port ?? (url.scheme == "http" ? 80 : 443))
    }

    public var all: [String: String] {
        lock.lock(); defer { lock.unlock() }
        return defaults.dictionary(forKey: storageKey) as? [String: String] ?? [:]
    }

    public func fingerprint(host: String, port: Int) -> String? { all[Self.key(host: host, port: port)] }

    public func set(_ fingerprint: String?, host: String, port: Int) {
        lock.lock(); defer { lock.unlock() }
        var d = defaults.dictionary(forKey: storageKey) as? [String: String] ?? [:]
        d[Self.key(host: host, port: port)] = fingerprint
        defaults.set(d, forKey: storageKey)
    }
}

/// Decides whether to accept a server's certificate: the system's trust first
/// (Let's Encrypt, a corporate CA, Tailscale), then a pinned fingerprint.
public final class TrustDelegate: NSObject, URLSessionDelegate, @unchecked Sendable {
    public struct Rejected: Sendable, Equatable {
        public var host: String
        public var port: Int
        public var fingerprint: String
    }

    public let pins: PinStore
    private let lock = NSLock()
    private var rejected: Rejected?
    private var pinnedHosts: Set<String> = []

    public init(pins: PinStore) { self.pins = pins }

    /// The last certificate that was refused, so the UI can offer to trust it.
    public var lastRejected: Rejected? { lock.lock(); defer { lock.unlock() }; return rejected }

    /// Did a connection to this server only succeed because of a pinned certificate?
    public func usesPin(host: String, port: Int) -> Bool {
        lock.lock(); defer { lock.unlock() }
        return pinnedHosts.contains(PinStore.key(host: host, port: port))
    }

    public func forgetRejection() { lock.lock(); rejected = nil; lock.unlock() }

    public func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge) async -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        let space = challenge.protectionSpace
        guard space.authenticationMethod == NSURLAuthenticationMethodServerTrust, let trust = space.serverTrust else {
            return (.performDefaultHandling, nil)
        }
        return evaluate(trust: trust, host: space.host, port: space.port)
    }

    /// Separated from the callback so it can be tested with a hand-made trust object.
    func evaluate(trust: SecTrust, host: String, port: Int) -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        var error: CFError?
        if SecTrustEvaluateWithError(trust, &error) { return (.performDefaultHandling, nil) }
        guard let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate], let leaf = chain.first else {
            return (.cancelAuthenticationChallenge, nil)
        }
        let fp = Fingerprint.of(der: SecCertificateCopyData(leaf) as Data)
        lock.lock(); defer { lock.unlock() }
        if pins.fingerprint(host: host, port: port) == fp {
            pinnedHosts.insert(PinStore.key(host: host, port: port))
            return (.useCredential, URLCredential(trust: trust))
        }
        rejected = Rejected(host: host, port: port, fingerprint: fp)
        return (.cancelAuthenticationChallenge, nil)
    }
}
