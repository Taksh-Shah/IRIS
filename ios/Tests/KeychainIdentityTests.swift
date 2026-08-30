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

    /// Wipe the real identity slot before/after each test to avoid cross-test pollution.
    override func setUp() {
        super.setUp()
        let deleteQuery: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: KeychainEd25519.service,
            kSecAttrAccount: KeychainEd25519.account,
        ]
        SecItemDelete(deleteQuery as CFDictionary)
    }

    override func tearDown() {
        let deleteQuery: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: KeychainEd25519.service,
            kSecAttrAccount: KeychainEd25519.account,
        ]
        SecItemDelete(deleteQuery as CFDictionary)
        super.tearDown()
    }

    /// AC-12: identity() returns an Ed25519KeyPair with 32-byte signing + verifying keys.
    func testIdentityReturnsPairWithCorrectKeySizes() throws {
        let pair = try KeychainEd25519.identity()
        XCTAssertEqual(pair.signingKeyRaw.count, 32, "Ed25519 signing key must be 32 bytes")
        XCTAssertEqual(pair.verifyingKeyRaw.count, 32, "Ed25519 verifying key must be 32 bytes")
    }

    /// AC-12: provision → load returns the identical key pair (stable PeerId).
    func testRoundTripKeepsSeedAndPublicKey() throws {
        let pair1 = try KeychainEd25519.identity()
        let pair2 = try KeychainEd25519.identity()
        XCTAssertEqual(pair1.signingKeyRaw, pair2.signingKeyRaw, "identity() must return the same key on repeat calls")
        XCTAssertEqual(pair1.verifyingKeyRaw, pair2.verifyingKeyRaw)
    }

    /// AC-12: explicit load() returns nil before first identity(); non-nil after.
    func testLoadReturnsNilBeforeProvision() throws {
        XCTAssertNil(try KeychainEd25519.load(), "load() must return nil before any provisioning")
        _ = try KeychainEd25519.identity()
        XCTAssertNotNil(try KeychainEd25519.load(), "load() must return the pair after identity()")
    }

    /// AC-12: stored pair can sign and self-verify — proves signingKeyRaw is the private key.
    func testStoredPairCanSignAndVerify() throws {
        let pair = try KeychainEd25519.identity()
        let ok = try KeychainEd25519.verify(Data([1, 2, 3]), signature: KeychainEd25519.sign(Data([1, 2, 3]), keyPair: pair), keyPair: pair)
        XCTAssertTrue(ok)
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

        let dataToSign = Data()
        let sig = try key.signature(for: dataToSign)
        XCTAssertTrue(key.publicKey.isValidSignature(sig, for: dataToSign))
    }

    // TODO: KeychainX25519 (X25519 static-DH key) not yet implemented (AC-12 future scope).
    // func testStaticAdKeyRoundTrip() { ... }
}