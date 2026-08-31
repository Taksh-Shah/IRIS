//
//  KeychainEd25519.swift
//  IRIS
//
//  AC-12 of IOS_DESIGN.md v1.0. Identity key for the iOS shell.
//
//  DEC-IOS-0001 (RES-0025 RQ-2): Secure Enclave supports ECDSA P-256 ONLY and
//  cannot host Ed25519; CRYPTO-001's envelope core is Ed25519. Identity is a
//  CryptoKit Ed25519 app-layer key persisted as a Keychain generic password:
//   - kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly (background-services
//     class; device-bound, excluded from backups),
//   - NO biometric flag (kSecAccessControl biometric prompts fail with
//     errSecInteractionNotAllowed while the app is backgrounded),
//   - Secure-Enclave-backed keys DEFERRED (known_limitation).
//
//  RFC 8032 known-answer-vector byte-compat test in Tests/KeychainEd25519Tests.

import Foundation
import CryptoKit

/// Keychain-safe Codable wrapper for the Ed25519 signing/verifying keys.
struct Ed25519KeyPair: Codable {
    let signingKeyRaw: Data
    let verifyingKeyRaw: Data
}

enum KeychainEd25519 {

    static let service = "com.iris.identity.v1"
    static let account = "ed25519-signing-pair"

    // Bug #15: serialize check-generate-store to prevent concurrent callers
    // (main + extension) both seeing "no key" and racing to store.
    private static let identityLock = NSLock()

    /// Provision (or load) the node identity. Create-on-first-run, then load.
    static func identity() throws -> Ed25519KeyPair {
        identityLock.lock(); defer { identityLock.unlock() }
        if let existing = try load() { return existing }
        let new = Self.generate()
        do {
            try store(new)
        } catch let err as NSError where err.code == Int(errSecDuplicateItem) {
            // Concurrent store from another process (widget extension) — load the winner
            if let winner = try load() { return winner }
        }
        return new
    }

    static func generate() -> Ed25519KeyPair {
        let signing = Curve25519.Signing.PrivateKey()
        return Ed25519KeyPair(
            signingKeyRaw: signing.rawRepresentation,
            verifyingKeyRaw: signing.publicKey.rawRepresentation
        )
    }

    static func load() throws -> Ed25519KeyPair? {
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecReturnData: kCFBooleanTrue as Any,
            kSecMatchLimit: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        switch status {
        case errSecSuccess:
            guard let data = item as? Data else { return nil }
            return try JSONDecoder().decode(Ed25519KeyPair.self, from: data)
        case errSecItemNotFound:
            return nil
        default:
            throw NSError(domain: NSOSStatusErrorDomain, code: Int(status), userInfo: nil)
        }
    }

    static func store(_ pair: Ed25519KeyPair) throws {
        let data = try JSONEncoder().encode(pair)
        let attributes: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecValueData: data,
            // DEC-IOS-0001: background-services class; device-bound.
            kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemAdd(attributes as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw NSError(domain: NSOSStatusErrorDomain, code: Int(status), userInfo: nil)
        }
    }

    /// Sign a payload with the node identity (self-signed identity attests).
    static func sign(_ payload: Data, keyPair: Ed25519KeyPair) throws -> Data {
        let key = try Curve25519.Signing.PrivateKey(rawRepresentation: keyPair.signingKeyRaw)
        return try key.signature(for: payload)
    }

    static func verify(_ payload: Data, signature: Data, keyPair: Ed25519KeyPair) throws -> Bool {
        let publicKey = try Curve25519.Signing.PublicKey(rawRepresentation: keyPair.verifyingKeyRaw)
        return publicKey.isValidSignature(signature, for: payload)
    }

    /// RFC 8032 §7.1 test vector (Ed25519 deterministic): seed = all 0x00,
    /// public key = 3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29.
    static func rfc8032TestVector() -> Ed25519KeyPair {
        let seed = Data(repeating: 0, count: 32)
        let signing = try! Curve25519.Signing.PrivateKey(rawRepresentation: seed)
        return Ed25519KeyPair(
            signingKeyRaw: signing.rawRepresentation,
            verifyingKeyRaw: signing.publicKey.rawRepresentation
        )
    }
}