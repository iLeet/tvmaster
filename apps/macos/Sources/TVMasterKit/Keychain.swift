import Foundation
import Security

/// Remembers the access token of each server you have connected to.
///
/// The tokens live in a private file in the app's own Application Support folder
/// (readable only by you), not in the Keychain: this app is not signed with a
/// developer identity, and macOS asks for your login password every time an
/// unsigned app reads the Keychain. A token saved in the Keychain by an earlier
/// version is picked up once, without any prompt, and moved to the file.
public struct TokenStore: Sendable {
    private let service: String
    private let file: URL
    private let migrateFromKeychain: Bool

    public init(service: String = "app.tvmaster.macos", directory: URL? = nil) {
        self.service = service
        let dir = directory
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("TVMaster", isDirectory: true)
        self.file = dir.appendingPathComponent("tokens-\(service == "app.tvmaster.macos" ? "main" : service).json")
        self.migrateFromKeychain = directory == nil && service == "app.tvmaster.macos"
    }

    // MARK: the file

    private func load() -> [String: String] {
        guard let data = try? Data(contentsOf: file),
              let map = try? JSONDecoder().decode([String: String].self, from: data) else { return [:] }
        return map
    }

    @discardableResult
    private func save(_ map: [String: String]) -> Bool {
        do {
            let dir = file.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true,
                                                    attributes: [.posixPermissions: 0o700])
            let data = try JSONEncoder().encode(map)
            try data.write(to: file, options: [.atomic])
            try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: file.path)
            return true
        } catch {
            return false
        }
    }

    // MARK: the old Keychain entry (read without ever asking)

    private func keychainToken(_ server: String) -> String? {
        let q: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: server,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
            // Fail quietly rather than show the "enter your password" sheet.
            kSecUseAuthenticationUI as String: kSecUseAuthenticationUIFail,
        ]
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // MARK: API

    public func token(for server: String) -> String? {
        if let t = load()[server], !t.isEmpty { return t }
        if migrateFromKeychain, let old = keychainToken(server), !old.isEmpty {
            var map = load()
            map[server] = old
            save(map)
            return old
        }
        return nil
    }

    @discardableResult
    public func set(_ token: String?, for server: String) -> Bool {
        var map = load()
        if let token, !token.isEmpty { map[server] = token } else { map.removeValue(forKey: server) }
        return save(map)
    }
}
