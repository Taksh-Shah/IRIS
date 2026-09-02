//
//  KeychainCryptoSigner.swift
//  IRIS
//
//  CROSS-004: Swift-side FfiCryptoSigner conformance over KeychainEd25519,
//  wired into IrisEngine.newWithSigner(). Before this existed, IrisEngine's
//  only constructor hardcoded the Rust-side DevCryptoProvider stub — debug
//  builds sent unsigned/unencrypted envelopes, release builds could not even
//  start (crates/iris-ios/src/engine.rs bug #41's guard). This mirrors the
//  Android AN-1 pattern (`FfiCryptoSigner` wrapping `KeystoreEd25519`).
//
//  `sign(data:)` must return exactly 64 raw Ed25519 signature bytes over
//  `data` (the payload `codec::encode_for_signing()` produces on the Rust
//  side) — see crates/iris-ios/src/ffi/crypto_signer.rs's trait doc.
//
//  NOTE: FfiCryptoSigner and IrisEngine.newWithSigner are generated into
//  IrisCore.swift by UniFFI from the Rust `#[uniffi::export(with_foreign)]`
//  trait / `#[uniffi::constructor]` fn added in this same fix — this file
//  will not compile until `ios/Scripts/build-xcframework.sh` is re-run to
//  regenerate IrisCore.swift from the updated Rust source (same class of
//  build-toolchain limitation as CROSS-008 in this environment).
//
import Foundation

final class KeychainCryptoSigner: FfiCryptoSigner {
    private let keyPair: Ed25519KeyPair

    init(keyPair: Ed25519KeyPair) {
        self.keyPair = keyPair
    }

    func sign(data: Data) throws -> Data {
        try KeychainEd25519.sign(data, keyPair: keyPair)
    }
}
