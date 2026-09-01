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
@Suppress("unused")
typealias FfiCryptoSigner = uniffi.iriscode.FfiCryptoSigner

// --- Engine host ---
@Suppress("unused")
typealias IrisEngineInterface = uniffi.iriscode.IrisEngineInterface
@Suppress("unused")
typealias IrisEngine = uniffi.iriscode.IrisEngine

// --- Exceptions ---
//
// `IrisFfiException` is a *sealed* class whose variants are nested classifiers.
// Kotlin does not resolve nested classifiers through a type alias qualifier
// (`IrisFfiException.GattFailure` fails to resolve when `IrisFfiException` is an
// alias), so each variant is re-exported as its own top-level alias. Adapters
// throw the variant directly (`throw GattFailure("...")`) and catch the base
// type; both surfaces stay inside the `iriscode.*` FQCN contract (AC-11).
@Suppress("unused")
typealias IrisFfiException = _IrisFfiException

@Suppress("unused")
typealias NotSupported = _IrisFfiException.NotSupported
@Suppress("unused")
typealias PermissionDenied = _IrisFfiException.PermissionDenied
@Suppress("unused")
typealias AdapterOff = _IrisFfiException.AdapterOff
@Suppress("unused")
typealias DeviceNotFound = _IrisFfiException.DeviceNotFound
@Suppress("unused")
typealias GattFailure = _IrisFfiException.GattFailure
@Suppress("unused")
typealias FfiTimeout = _IrisFfiException.Timeout
@Suppress("unused")
typealias IoException = _IrisFfiException.IoException
@Suppress("unused")
typealias InvalidArgument = _IrisFfiException.InvalidArgument
@Suppress("unused")
typealias TransportFailure = _IrisFfiException.Transport

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
typealias FfiAcceptedConnection = uniffi.iriscode.FfiAcceptedConnection
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