// IRIS UniFFI Kotlin facade — package `iriscode` (AC-11 conformance).
//
// The uniffi-bindgen 0.31.2 output lives in `uniffi.iriscode` (committed,
// immutable generated file). Android app + adapter source sets consume FFI
// types through this thin re-export layer so the FQCN surface (`iriscode.*`)
// is stable across regen and every adapter op maps 1:1 (no orphan ops).
//
// Regen procedure (toolchain host): uniffi-bindgen generate --library
// target/debug/iriscode.dll --language kotlin, then re-generate this facade
// from the public surface of uniffi.iriscode.

package iriscode

import uniffi.iriscode.IrisFfiException as _IrisFfiException

// --- Interfaces (foreign traits) ---
@Suppress("unused")
typealias FfiBleAdapter = uniffi.iriscode.FfiBleAdapter
@Suppress("unused")
typealias FfiWifiAwareAdapter = uniffi.iriscode.FfiWifiAwareAdapter
@Suppress("unused")
typealias FfiWifiDirectAdapter = uniffi.iriscode.FfiWifiDirectAdapter
@Suppress("unused")
typealias FfiInboxListener = uniffi.iriscode.FfiInboxListener

// --- Engine host ---
@Suppress("unused")
typealias IrisEngineInterface = uniffi.iriscode.IrisEngineInterface
@Suppress("unused")
typealias IrisEngine = uniffi.iriscode.IrisEngine

// --- Exceptions ---
@Suppress("unused")
typealias IrisFfiException = _IrisFfiException

// --- Data classes (records) ---
@Suppress("unused")
typealias FfiScanFilter = uniffi.iriscode.FfiScanFilter
@Suppress("unused")
typealias FfiScanResult = uniffi.iriscode.FfiScanResult
@Suppress("unused")
typealias FfiAdvertisementData = uniffi.iriscode.FfiAdvertisementData
@Suppress("unused")
typealias FfiGattWriteEvent = uniffi.iriscode.FfiGattWriteEvent
@Suppress("unused")
typealias FfiPublishConfig = uniffi.iriscode.FfiPublishConfig
@Suppress("unused")
typealias FfiPeerDiscovery = uniffi.iriscode.FfiPeerDiscovery
@Suppress("unused")
typealias FfiIncomingNdpData = uniffi.iriscode.FfiIncomingNdpData
@Suppress("unused")
typealias FfiDirectPeerDiscovery = uniffi.iriscode.FfiDirectPeerDiscovery
@Suppress("unused")
typealias FfiGroupConfig = uniffi.iriscode.FfiGroupConfig
@Suppress("unused")
typealias FfiGroupInfo = uniffi.iriscode.FfiGroupInfo
@Suppress("unused")
typealias FfiIncomingWifiDirectData = uniffi.iriscode.FfiIncomingWifiDirectData
@Suppress("unused")
typealias FfiIncomingMessage = uniffi.iriscode.FfiIncomingMessage

// --- Enums ---
@Suppress("unused")
typealias FfiOperatingBand = uniffi.iriscode.FfiOperatingBand