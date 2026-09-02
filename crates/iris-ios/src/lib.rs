//! IRIS iOS integration layer — UniFFI binding crate (IOS-001).
//!
//! Contract: `docs/implementation/IOS_DESIGN.md` v1.0 (IRIS-IOS-001-DESIGN-001,
//! 2026-08-18). This crate is the Rust FFI half of the iOS platform shell; it
//! builds staticlibs for `aarch64-apple-ios` + `aarch64-apple-ios-sim`
//! (IOS_DESIGN §9 AC-1) and forces the same `uniffi = "=0.31.2"` pin as
//! `crates/iris-android` (RES-0025 §2, RES-0022 G-AND-1).
//!
//! FFI surface (IOS_DESIGN §4 / §9):
//! - `ffi::ble_adapter::FfiBleAdapter` — the 11-op `BleAdapter` foreign-trait
//!   projection incl. `gatt_read` (DEC-BLE-002-0015 carry; the Swift
//!   `IosBleAdapter` implements it over CoreBluetooth connect-to-identify).
//! - `ffi::body::IrisBody` — foreign envelope-body surface (async-over-FFI
//!   path, G-IOS spike) implemented by the Swift app shell.
//! - `engine::IrisEngine` — `#[uniffi::export]` object hosting the real
//!   `TransportManager` + `BleTransport::new_ios` (`ble-ios`) + `MessageEngine`
//!   over `crate::bridge`; every async poll path runs through an explicit
//!   `tokio::runtime::Handle` (issue #2576 workaround, DEC-IOS-0003) — the
//!   same proof as the Kotlin RES-0022/G-AND-3 spike (AC-3).
//!
//! The Swift/CoreBluetooth side (`IosBleAdapter`, identity, SessionRecovery,
//! Live Activity, BGTask) is `ios/` + `ios.yml` (AC-3..AC-16) and is macOS-CI
//! gated (env-leg, G-TI-1 pattern — dev host is Windows, no Xcode).

// The `[lib] name = "IrisCore"` fixes the UniFFI namespace -> generated Swift
// module `IrisCore` (IOS_DESIGN §3: `IrisCore.swift` / `IrisCoreFFI.h`).
// rustc flags the CamelCase crate name; allowed deliberately (CI runs
// `-D warnings` and the module name is a binding-contract).
#![allow(non_snake_case)]

uniffi::setup_scaffolding!();

pub mod bridge;
pub mod engine;
pub mod ffi;
pub mod ios_crypto;

pub use engine::{FfiInboxListener, FfiIncomingMessage, IrisEngine};
pub use ffi::ble_adapter::{
    FfiAdvertisementData, FfiBleAdapter, FfiGattWriteEvent, FfiScanFilter, FfiScanResult,
};
pub use ffi::body::{FfiIrisEnvelope, IrisBody};
pub use ffi::crypto_signer::FfiCryptoSigner;
pub use ffi::error::IrisFfiError;
