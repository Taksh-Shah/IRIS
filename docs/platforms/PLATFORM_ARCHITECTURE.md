# IRIS Platform Architecture

## Overview

IRIS is built on a strict two-layer architecture: a Rust core library (`iris-core`) containing all business logic, and thin platform adapters on Android, iOS, and desktop. The platform layers adapt OS APIs to the core but contain no routing, protocol, or cryptographic logic.

## Layer Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Platform Layer                           │
│   Android (Kotlin)    iOS (Swift)    Desktop (TypeScript/Tauri) │
│   Jetpack Compose     SwiftUI        React + Webview2           │
│   BLE Android API     CoreBluetooth  WinRT / BlueZ / CoreBT     │
│   Wi-Fi Aware         MCF            Wi-Fi Direct               │
│   Android Keystore    Secure Enclave DPAPI / Keychain           │
└─────────────────────┬──────────────────────────┬────────────────┘
                      │ JNI (UniFFI)             │ Swift FFI (UniFFI)
                      │ Tauri IPC                │
┌─────────────────────▼──────────────────────────▼────────────────┐
│                        iris-core (Rust)                         │
│                                                                 │
│  message_engine   routing_engine   transport_manager            │
│  discovery        crypto           storage                      │
│  emergency        identity         simulation                   │
│                                                                 │
│  Tokio async runtime                                            │
│  serde_cbor / ed25519-dalek / x25519-dalek / chacha20poly1305   │
│  rusqlite / bloom / uuid / tracing / metrics                    │
└─────────────────────────────────────────────────────────────────┘
```

## Rust Core (`iris-core`)

The core is a Cargo workspace containing multiple crates:

### `iris-core`
Central crate. Owns the message lifecycle, priority queue, and orchestration of all subsystems. All subsystem modules are internal to this crate or re-exported from subcrates.

### `iris-transport`
Defines the `TransportAdapter` trait — the interface all platform-specific transports must implement:

```rust
#[async_trait]
pub trait TransportAdapter: Send + Sync {
    fn transport_type(&self) -> TransportType;
    fn capabilities(&self) -> TransportCapabilities;
    async fn send(&self, peer_id: &NodeId, data: &[u8]) -> Result<(), TransportError>;
    async fn broadcast(&self, data: &[u8]) -> Result<usize, TransportError>;
    fn is_available(&self) -> bool;
    fn estimated_range_meters(&self) -> u32;
    fn estimated_bandwidth_bps(&self) -> u32;
}
```

Platform code (Kotlin BLE, Swift CoreBluetooth, Rust BlueZ) implements this trait and registers with `TransportManager` at startup.

### `iris-storage`
SQLite storage via `rusqlite`. Encrypted with SQLCipher. Owns all schema, migration, and query logic. No SQL appears outside this crate.

### `iris-crypto`
Thin wrapper around cryptographic primitives. Exposes higher-level operations (`sign_message`, `verify_message`, `encrypt_payload`, `decrypt_payload`, `derive_shared_secret`) without exposing raw primitive APIs at the platform boundary.

### `iris-sim`
Simulation engine for offline testing. Provides a virtual node graph, simulated transports, and time control. Used in `iris-sim` CLI and in integration tests.

### `iris-proto`
CBOR schema definitions. All message types, envelope structure, and serialization logic. The canonical source for wire format.

## Platform Communication Mechanisms

### Android: JNI via UniFFI

UniFFI reads a UDL (WASM/IDL-like) interface definition and generates:
- Rust `extern "C"` scaffolding
- Kotlin `IrisCore.kt` with type-safe Kotlin APIs
- JNI glue (Java Native Interface bindings)

The Kotlin layer calls generated Kotlin functions. No handwritten JNI.

```
iris.udl (interface definition)
    ↓ uniffi-bindgen generate
IrisCore.kt (Kotlin, auto-generated)
    ↓ calls via JNI
libiriscode.so (compiled Rust, arm64-v8a + armeabi-v7a + x86_64)
```

Android BLE transport is implemented in Kotlin as a class that implements a Kotlin interface mirroring `TransportAdapter`. The JNI layer passes events to the Rust `TransportManager` via callback registration.

### iOS: Swift FFI via UniFFI

Same UDL file generates Swift bindings:

```
iris.udl
    ↓ uniffi-bindgen generate --language swift
IrisCore.swift (Swift, auto-generated)
IrisCoreFFI.h (C header, auto-generated)
    ↓ linked against
libiriscode.a (XCFramework: arm64 device + arm64 simulator + x86_64 simulator)
```

Swift calls generated `IrisCore.swift` functions. CoreBluetooth transport implemented in Swift, registered with Rust core via callback.

### Desktop: Tauri IPC

Tauri provides a typed command system. TypeScript frontend calls Rust backend functions through Tauri's `invoke()`:

```typescript
// TypeScript
const result = await invoke<DeliveryStatus>('send_message', {
  recipientId: recipientHex,
  payload: messageBytes,
  priority: 4,
});
```

```rust
// Rust (Tauri command handler)
#[tauri::command]
async fn send_message(
    state: State<'_, IrisCoreHandle>,
    recipient_id: String,
    payload: Vec<u8>,
    priority: u8,
) -> Result<DeliveryStatus, String> {
    state.core.send_message(recipient_id, payload, priority).await
        .map_err(|e| e.to_string())
}
```

## Why Rust Core

### Memory Safety at the Security Boundary
Protocol parsers are the most common source of exploitable memory corruption vulnerabilities. CBOR parsing, message validation, and cryptographic boundary checks all occur in Rust, where the compiler proves absence of buffer overflows, use-after-free, and double-free at compile time — not at runtime.

### Performance Without Garbage Collector
Routing decisions must complete in under 1 millisecond. Garbage collector pauses (Go: 1-10ms, JVM: 10-100ms in worst case) are incompatible with this latency requirement. Rust has no GC — memory is managed deterministically via ownership and RAII.

### Tokio Async Runtime
Tokio is a production-grade async runtime used by Cloudflare, Discord, AWS Lambda extensions, and Axum. It provides:
- Multi-threaded work-stealing scheduler
- `async_trait` for async interfaces
- `tokio::sync` primitives (Mutex, RwLock, mpsc, broadcast, oneshot)
- `tokio::time` for timeout and interval handling
- `tokio::net` for TCP/UDP when available

### Cross-Platform Compilation
Rust cross-compiles to all targets via `rustup target add`:
- `aarch64-linux-android` (Android ARM64)
- `armv7-linux-androideabi` (Android ARMv7)
- `x86_64-linux-android` (Android emulator)
- `aarch64-apple-ios` (iPhone/iPad)
- `aarch64-apple-ios-sim` (iOS simulator ARM64)
- `x86_64-apple-ios` (iOS simulator x86)
- `x86_64-unknown-linux-gnu` (Linux desktop)
- `aarch64-unknown-linux-gnu` (Linux ARM64, Raspberry Pi)
- `x86_64-pc-windows-msvc` (Windows)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS Apple Silicon)

### FFI Compatibility
Rust functions marked `extern "C"` export a stable C ABI callable from any language. UniFFI automates the binding generation so platform engineers never write JNI or Objective-C manually.

## Platform-Specific Responsibilities

Each platform layer handles exactly the following and nothing more:

| Responsibility | Android | iOS | Windows | macOS | Linux |
|---------------|---------|-----|---------|-------|-------|
| BLE hardware API | BluetoothManager | CoreBluetooth | WinRT BLE | CoreBluetooth | BlueZ |
| Wi-Fi Direct/Aware | Wi-Fi Aware + P2P | MCF | WiFiDirect | Bonjour | wpa_supplicant |
| OS permissions | Runtime permission | Info.plist + runtime | Manifest | Entitlements | capabilities |
| Background execution | ForegroundService | BGTaskScheduler | System tray | LaunchAgent | systemd |
| Key storage | Android Keystore | Secure Enclave | DPAPI | Keychain | libsecret |
| UI rendering | Jetpack Compose | SwiftUI | React/Webview2 | React/Webview2 | React/Webview2 |
| Notifications | NotificationManager | UNUserNotification | WinToast | NSUserNotification | libnotify |
| App lifecycle | Activity/Service | UIApplication | Tauri app | Tauri app | Tauri app |

## Shared Across All Platforms (via Rust Core)

- All message protocol logic (envelope parsing, validation, TTL, hop count)
- All routing algorithms (PRoPHET, Spray-and-Wait, epidemic, direct)
- All cryptographic operations (key generation, signing, verification, E2EE)
- All storage schema and queries
- All deduplication logic (Bloom filter + seen_messages table)
- All priority queue management (P0-P7)
- All simulation and testing utilities
- All metrics and observability hooks

## Transport Abstraction

Transport implementations are platform-specific but all implement the common `TransportAdapter` Rust trait. The `TransportManager` in `iris-core` maintains a registry of available transports and selects among them based on:

1. Availability (is the transport hardware present and enabled?)
2. Peer reachability (can this peer be reached via this transport?)
3. Bandwidth and range requirements (is the message too large for LoRa?)
4. Battery mode (is the system in LOW_BATTERY mode? Disable Wi-Fi Direct)
5. Priority (is this P0? Use all available transports simultaneously)

The `TransportManager` is transport-agnostic: it never contains BLE or Wi-Fi code. Transport plugins register themselves at startup and emit events via Tokio channels.

## Module Interaction Flow

```
Incoming message (from BLE, raw bytes)
    ↓
TransportAdapter::on_receive() — platform code, calls core via FFI
    ↓
TransportManager::handle_incoming(transport_type, peer_id, data)
    ↓
iris-proto: Envelope::from_cbor(data) — parse and validate
    ↓
MessageEngine::process_incoming(envelope)
    ├── DeduplicationEngine::check_and_mark(message_id) — Bloom filter + DB
    ├── RoutingEngine::should_relay(envelope) — routing decision
    ├── CryptoEngine::verify_signature(envelope) — signature check
    ├── Storage::persist_message(envelope) — ACID write
    └── TransportManager::forward(envelope, next_hops) — relay if needed
```

Every step is async. Every step can fail with a typed error. No panics in the hot path.
