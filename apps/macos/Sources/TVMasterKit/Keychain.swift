import Foundation
import Security

/// Stores the server's access token in the macOS Keychain, per server address.
public struct TokenStore: Sendable {
    private let service: String

    public init(service: String = "app.tvmaster.macos") {
        self.service = service
    }

    private func query(_ server: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: server,
        ]
    }

    public func token(for server: String) -> String? {
        var q = query(server)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    @discardableResult
    public func set(_ token: String?, for server: String) -> Bool {
        SecItemDelete(query(server) as CFDictionary)
        guard let token, !token.isEmpty else { return true }
        var q = query(server)
        q[kSecValueData as String] = Data(token.utf8)
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        return SecItemAdd(q as CFDictionary, nil) == errSecSuccess
    }
}
