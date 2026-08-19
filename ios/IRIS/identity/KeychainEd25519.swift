// IOS-001 AC-12 — Keychain sewing seam + identity key providers.
//
// D-5 identity: CryptoKit `Curve25519.Signing.PrivateKey` (Ed25519, RFC 8032),
// persisted as a `kSecClassGenericPassword` item, tag `com.iris.identity.v1`,
// `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, NO `kSecAccessControl`
// biometric flags (FaceID/TouchID-gated items fail in background with
// `errSecInteractionNotAllowed` — RES-0025 RQ-2). Load path gated on
// `isProtectedDataAvailable` (a process can be pre-warmed before first unlock).
//
// The `KeychainAccessible` protocol is the Security-framework seam so the
// macOS-host unit tests round-trip a REAL Keychain item (Keychain works on the
// macOS host) while the protected-data gate stays injection-testable.
import Foundation
import CryptoKit
import Security

/// Security-framework seam (generic-password CRUD). Real impl = `SecurityKeychain`.
public protocol KeychainAccessible: AnyObject {
    func set(_ data: Data, for account: String, service: String) throws
    func get(_ account: String, service: String) throws -> Data?
    func delete(_ account: String, service: String) throws
    /// kSecAttrAccessible value for the item. Defaults to
    /// `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` (D-5 / AC-12).
    var accessibility: CFString { get }
}

public final class SecurityKeychain: KeychainAccessible {
    public init() {}

    public var accessibility: CFString {
        kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    }

    public func set(_ data: Data, for account: String, service: String) throws {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: account,
            kSecAttrService as String: service,
            kSecAttrAccessible as String: accessibility,
            kSecValueData as String: data,
        ]
        // Upsert: delete-then-add is atomic enough for a single-key provider.
        delete(account, service: service)
        let status = SecItemAdd(base as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.unavailable(Int(status))
        }
    }

    public func get(_ account: String, service: String) throws -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: account,
            kSecAttrService as String: service,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else {
            throw KeychainError.unavailable(Int(status))
        }
        return item as? Data
    }

    public func delete(_ account: String, service: String) throws {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: account,
            kSecAttrService as String: service,
        ]
        let status = SecItemDelete(query as CFDictionary)
        if status == errSecItemNotFound { return }
        guard status == errSecSuccess else {
            throw KeychainError.unavailable(Int(status))
        }
    }
}

public enum KeychainError: Error, Equatable {
    /// Security framework returned a non-success status.
    case unavailable(Int)
    /// `load()` gate: app started before the protected data is available.
    case protectedDataUnavailable
    case invalidKeyBytes
}

// MARK: - Ed25519 identity (identity.v1)

/// D-5 identity provider: `com.iris.identity.v1` Ed25519 signing key.
public final class KeychainEd25519 {
    public static let service = "com.iris.identity.v1"

    private let keychain: KeychainAccessible
    /// Protected-data gate (iOS pre-warm). Persisted through injection so the
    /// macOS-host unit tests can exercise both branches.
    private let protectedDataAvailable: () -> Bool

    public init(
        keychain: KeychainAccessible = SecurityKeychain(),
        protectedDataAvailable: (() -> Bool)? = nil
    ) {
        self.keychain = keychain
        self.protectedDataAvailable = protectedDataAvailable ?? {
            #if os(iOS)
            return UIApplication.shared.isProtectedDataAvailable
            #else
            return true
            #endif
        }
    }

    /// Load the existing identity key, or create + persist a fresh one on first
    /// launch (`identity.v1` provisioning).
    public func loadOrCreate() throws -> Curve25519.Signing.PrivateKey {
        guard protectedDataAvailable() else {
            throw KeychainError.protectedDataUnavailable
        }
        if let existing = try keychain.get(Self.service, service: Self.service) {
            return try decode(existing)
        }
        let key = Curve25519.Signing.PrivateKey()
        try persist(key)
        return key
    }

    /// Load-only (throws protectedDataUnavailable if gated; nil if absent).
    public func load() throws -> Curve25519.Signing.PrivateKey? {
        guard protectedDataAvailable() else {
            throw KeychainError.protectedDataUnavailable
        }
        guard let data = try keychain.get(Self.service, service: Self.service) else {
            return nil
        }
        return try decode(data)
    }

    public func persist(_ key: Curve25519.Signing.PrivateKey) throws {
        try keychain.set(key.rawRepresentation, for: Self.service, service: Self.service)
    }

    /// RFC 8032 byte-compat decode: the rawRepresentation is the 32-byte seed;
    /// CryptoKit derives the public key exactly as `ed25519-dalek` does (D-5).
    private func decode(_ data: Data) throws -> Curve25519.Signing.PrivateKey {
        guard data.count == 32 else { throw KeychainError.invalidKeyBytes }
        return try Curve25519.Signing.PrivateKey(rawRepresentation: data)
    }
}

// MARK: - X25519 static-ad key

/// D-5 static advertisement X25519 key (X25519StaticAd-ios analogue; same
/// persistence posture as the identity key).
public final class KeychainX25519 {
    public static let service = "com.iris.staticad.v1"

    private let keychain: KeychainAccessible
    private let protectedDataAvailable: () -> Bool

    public init(
        keychain: KeychainAccessible = SecurityKeychain(),
        protectedDataAvailable: (() -> Bool)? = nil
    ) {
        self.keychain = keychain
        self.protectedDataAvailable = protectedDataAvailable ?? {
            #if os(iOS)
            return UIApplication.shared.isProtectedDataAvailable
            #else
            return true
            #endif
        }
    }

    public func loadOrCreate() throws -> Curve25519.KeyAgreement.PrivateKey {
        guard protectedDataAvailable() else {
            throw KeychainError.protectedDataUnavailable
        }
        if let existing = try keychain.get(Self.service, service: Self.service),
           let key = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: existing) {
            return key
        }
        let key = Curve25519.KeyAgreement.PrivateKey()
        try keychain.set(key.rawRepresentation, for: Self.service, service: Self.service)
        return key
    }

    public func load() throws -> Curve25519.KeyAgreement.PrivateKey? {
        guard protectedDataAvailable() else {
            throw KeychainError.protectedDataUnavailable
        }
        guard let data = try keychain.get(Self.service, service: Self.service) else {
            return nil
        }
        return try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: data)
    }
}