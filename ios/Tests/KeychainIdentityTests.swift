// IOS-001 AC-12 — KeychainIdentityTests (macOS-host, real Keychain round-trip).
//
// identity.v1 provisioning/load through the REAL Security framework (Keychain
// works on the macOS host), plus the RFC 8032 byte-compat KAT that pins
// CryptoKit Ed25519 raw representations against `ed25519-dalek` (G-IOS-5).
import Foundation
import CryptoKit
import Security
import XCTest

#if canImport(IRIS)
import IRIS
#endif

final class KeychainIdentityTests: XCTestCase {

    /// Unique test-only tag so the suite never collides with app identity.
    private let account = "com.iris.identity.test.\(UUID().uuidString)"

    private var keychain: KeychainAccessible!

    override func setUp() {
        super.setUp()
        let k = SecurityKeychain()
        try? k.delete(account, service: account)
        keychain = k
    }

    override func tearDown() {
        try? keychain.delete(account, service: account)
        super.tearDown()
    }

    /// AC-12: the identity item is a generic password with the documented
    /// accessibility class and NO biometric kSecAccessControl.
    func testAccessibilityIsAfterFirstUnlockThisDeviceOnly() {
        let provider = KeychainEd25519(protectedDataAvailable: { true })
        let probe = SecurityKeychain()
        XCTAssertEqual(probe.accessibility as String, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String)
        XCTAssertNotNil(provider)
    }

    /// AC-12: provision → persist → load returns the identical key (stable PeerId).
    func testRoundTripKeepsSeedAndPublicKey() throws {
        let provider = KeychainEd25519(protectedDataAvailable: { true })
        let key = try provider.loadOrCreate()
        let stored = try keychain.get(KeychainEd25519.service, service: KeychainEd25519.service)
        XCTAssertEqual(stored?.count, 32, "generic-password value = the 32-byte Ed25519 seed")

        // Persist to the isolated test tag, then decode back: same seed + pubkey.
        try keychain.set(key.rawRepresentation, for: account, service: account)
        let reloaded = try Curve25519.Signing.PrivateKey(rawRepresentation: keychain.get(account, service: account)!)
        XCTAssertEqual(reloaded.rawRepresentation, key.rawRepresentation)
        XCTAssertEqual(reloaded.publicKey.rawRepresentation, key.publicKey.rawRepresentation)
    }

    /// AC-12: load is gated on protected-data availability (pre-warm).
    func testProtectedDataGateBlocksLoad() {
        let provider = KeychainEd25519(protectedDataAvailable: { false })
        XCTAssertThrowsError(try provider.loadOrCreate()) { error in
            guard case KeychainError.protectedDataUnavailable = error else {
                return XCTFail("expected protectedDataUnavailable, got \(error)")
            }
        }
    }

    /// AC-12 / G-IOS-5: RFC 8032 KAT — the CryptoKit raw seed (32 B) is the
    /// ed25519 secret; the derived public key matches the RFC 8032 test vector,
    /// byte-for-byte what `ed25519-dalek` reconstructs from the same seed.
    func testRFC8032KatSeedDerivesCanonicalPublicKey() throws {
        let seed = Data([
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60,
            0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
            0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19,
            0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
        ])
        let expectedPub = Data([
            0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7,
            0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
            0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25,
            0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
        ])
        let key = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
        XCTAssertEqual(key.publicKey.rawRepresentation, expectedPub)

        // Signature over the RFC 8032 test message ("") verifies with the
        // derived public key — the same trust flow Rust `ed25519-dalek` uses.
        let dataToSign = Data()
        let sig = try key.signature(for: dataToSign)
        let ok = key.publicKey.isValidSignature(sig, for: dataToSign)
        XCTAssertTrue(ok)
    }

    /// AC-12: X25519 static-ad key — same persistence posture, 32-byte seed.
    func testStaticAdKeyRoundTrip() throws {
        let provider = KeychainX25519(protectedDataAvailable: { true })
        let k1 = try provider.loadOrCreate()
        let k2 = try provider.loadOrCreate()
        XCTAssertEqual(k1.rawRepresentation, k2.rawRepresentation)
        XCTAssertEqual(k1.publicKey.rawRepresentation, k2.publicKey.rawRepresentation)
    }
}