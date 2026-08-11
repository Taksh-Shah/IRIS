# IRIS Rust Core Implementation

## Overview

`iris-core` is the heart of IRIS. All message protocol logic, routing algorithms, cryptographic operations, and storage operations live here. Platform layers (Android/iOS/Desktop) are thin adapters that call into this crate via FFI.

## Async Runtime: Tokio

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

IRIS is built on Tokio's multi-threaded work-stealing scheduler. The core uses:
- `tokio::sync::mpsc` — for inter-subsystem message passing
- `tokio::sync::broadcast` — for network events (neighbor discovered, message received)
- `tokio::sync::Mutex` / `RwLock` — for shared state
- `tokio::time::timeout` / `interval` — for TTL enforcement and periodic tasks
- `tokio::task::spawn` / `spawn_blocking` — for CPU-bound operations

The `IrisCore` struct is the main entry point, holding a Tokio runtime (when used from FFI) or running inside the caller's runtime (when used from Rust integration tests).

## Complete Dependency Set

```toml
[dependencies]
# Async
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# Serialization (CBOR wire format)
serde = { version = "1", features = ["derive"] }
ciborium = "0.2"                        # CBOR via serde
serde_bytes = "0.11"                    # Efficient byte array serialization

# Cryptography
ed25519-dalek = { version = "2", features = ["serde", "rand_core"] }
x25519-dalek = { version = "2", features = ["serde", "static_secrets"] }
chacha20poly1305 = "0.10"
hkdf = "0.12"
sha2 = "0.10"
blake3 = "1"
rand = "0.8"
rand_core = "0.6"
zeroize = { version = "1", features = ["derive"] }   # Secure key erasure

# Message IDs
uuid = { version = "1", features = ["v7"] }

# Deduplication
bloomfilter = "1"

# Storage
rusqlite = { version = "0.31", features = ["bundled"] }
rusqlite_migration = "1"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
metrics = "0.22"
metrics-exporter-prometheus = "0.13"

# Error handling
thiserror = "1"
anyhow = "1"

# Utilities
bytes = "1"
hex = "1"
once_cell = "1"
parking_lot = "0.12"                     # Faster Mutex than std
smallvec = "1"                           # Stack-allocated small vectors

[dev-dependencies]
proptest = "1"
tokio-test = "0.4"
tempfile = "3"
hex-literal = "0.4"
criterion = { version = "0.5", features = ["async_tokio"] }
```

## Module Structure

```
iris-core/src/
├── lib.rs                    # Public API, IrisCore struct, FFI entry points
├── error.rs                  # IrisError enum (thiserror)
├── config.rs                 # IrisCoreConfig (TOML-deserializable)
│
├── message_engine/
│   ├── mod.rs                # MessageEngine struct
│   ├── lifecycle.rs          # Message state machine (QUEUED → PENDING → DELIVERED)
│   ├── queue.rs              # Priority queue (P0-P7, BinaryHeap)
│   └── expiry.rs             # TTL enforcement, eviction
│
├── routing_engine/
│   ├── mod.rs                # RoutingEngine struct
│   ├── prophet.rs            # PRoPHET routing algorithm
│   ├── spray.rs              # Spray-and-Wait
│   ├── epidemic.rs           # Epidemic (flood) routing
│   ├── direct.rs             # Direct delivery only
│   └── contact_history.rs    # Contact event log for PRoPHET
│
├── transport_manager/
│   ├── mod.rs                # TransportManager
│   ├── selection.rs          # Transport selection logic
│   └── registry.rs           # Registered transport adapters
│
├── discovery/
│   ├── mod.rs                # DiscoveryManager
│   ├── handshake.rs          # Peer capability exchange
│   └── neighbor_table.rs     # Active neighbor tracking
│
├── crypto/
│   ├── mod.rs                # CryptoEngine
│   ├── identity.rs           # Node identity (Ed25519 keypair)
│   ├── session.rs            # Per-session keys (X25519 + HKDF)
│   ├── encryption.rs         # ChaCha20-Poly1305 encrypt/decrypt
│   └── signing.rs            # Ed25519 sign/verify
│
├── storage/
│   ├── mod.rs                # StorageEngine (delegates to iris-storage)
│   └── cache.rs              # In-memory cache over SQLite
│
├── deduplication/
│   └── mod.rs                # BloomFilter + seen_messages DB fallback
│
├── emergency/
│   ├── mod.rs                # EmergencyEngine
│   ├── sos.rs                # SOS message handling
│   ├── broadcast.rs          # Emergency broadcast from authorities
│   └── priority_routing.rs   # P0 routing override
│
└── identity/
    ├── mod.rs                # IdentityManager
    ├── address.rs            # NodeId derivation from public key
    └── keystore.rs           # Platform keystore abstraction
```

## IrisCore Public API

```rust
pub struct IrisCore {
    config: IrisCoreConfig,
    message_engine: Arc<MessageEngine>,
    routing_engine: Arc<RoutingEngine>,
    transport_manager: Arc<TransportManager>,
    discovery: Arc<DiscoveryManager>,
    crypto: Arc<CryptoEngine>,
    storage: Arc<StorageEngine>,
    emergency: Arc<EmergencyEngine>,
    identity: Arc<IdentityManager>,
    event_tx: broadcast::Sender<IrisEvent>,
}

impl IrisCore {
    pub async fn new(config: IrisCoreConfig) -> Result<Self, IrisError>;
    pub async fn start(&self) -> Result<(), IrisError>;
    pub async fn stop(&self) -> Result<(), IrisError>;

    // Messaging
    pub async fn send_message(
        &self,
        recipient: NodeId,
        payload: Vec<u8>,
        priority: Priority,
    ) -> Result<MessageId, IrisError>;

    pub async fn get_messages(&self, limit: u32) -> Result<Vec<MessageSummary>, IrisError>;

    // Emergency
    pub async fn send_sos(&self) -> Result<MessageId, IrisError>;
    pub async fn cancel_sos(&self, sos_id: MessageId) -> Result<(), IrisError>;

    // Network
    pub fn get_neighbors(&self) -> Vec<NeighborInfo>;
    pub fn get_network_status(&self) -> NetworkStatus;

    // Events (subscribe to receive incoming messages, delivery acks, etc.)
    pub fn subscribe_events(&self) -> broadcast::Receiver<IrisEvent>;

    // Transport registration (called by platform layer)
    pub async fn register_transport(
        &self,
        adapter: Box<dyn TransportAdapter>,
    ) -> Result<(), IrisError>;
}
```

## FFI Boundary (UniFFI)

The `iris-ffi` crate exposes `IrisCore` through a UniFFI interface:

```idl
// iris.udl (UniFFI Interface Definition Language)
namespace iris {};

interface IrisCore {
    [Throws=IrisError]
    constructor(IrisCoreConfig config);

    [Async, Throws=IrisError]
    void start();

    [Async, Throws=IrisError]
    string send_message(string recipient_hex, bytes payload, u8 priority);

    sequence<NeighborInfo> get_neighbors();

    NetworkStatus get_network_status();

    [Async, Throws=IrisError]
    string send_sos();
};

dictionary IrisCoreConfig {
    string? node_id_hex;
    string storage_path;
    string routing_algorithm;
    u32 max_storage_bytes;
    boolean emergency_enabled;
};

dictionary NeighborInfo {
    string node_id_hex;
    string transport_type;
    i64 last_seen_ms;
    i16? rssi;
};
```

UniFFI generates:
- `IrisCore.kt` for Android (Kotlin, via JNI)
- `IrisCore.swift` for iOS (Swift, via C ABI)

## Priority Queue Implementation

```rust
use std::collections::BinaryHeap;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
struct QueuedMessage {
    priority: Priority,        // P0 (highest) to P7 (lowest)
    created_at: Instant,       // Earlier = higher priority within same priority level
    message_id: MessageId,
    expires_at: Instant,
}

impl Ord for QueuedMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // Lower priority number = higher urgency = should be dequeued first
        other.priority.cmp(&self.priority)
            .then_with(|| self.created_at.cmp(&other.created_at))
    }
}

impl PartialOrd for QueuedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
```

## PRoPHET Routing Core

```rust
pub struct ProphetRouter {
    delivery_predictabilities: DashMap<NodeId, f64>,
    contact_history: Vec<ContactEvent>,
    config: ProphetConfig,
}

impl ProphetRouter {
    const P_INIT: f64 = 0.75;
    const GAMMA: f64 = 0.98;   // Aging factor (per time unit)
    const BETA: f64 = 0.25;    // Transitivity scaling factor

    /// Update delivery predictability on contact with peer
    pub fn on_contact(&self, peer: NodeId, now: Instant) {
        let p = self.delivery_predictabilities.entry(peer).or_insert(0.0);
        // P(a,b) = P(a,b)_old + (1 - P(a,b)_old) * P_INIT
        *p = *p + (1.0 - *p) * Self::P_INIT;
    }

    /// Age all predictabilities (called periodically)
    pub fn age_predictabilities(&self, elapsed_units: f64) {
        for mut entry in self.delivery_predictabilities.iter_mut() {
            *entry.value_mut() *= Self::GAMMA.powf(elapsed_units);
        }
    }

    /// Transitivity: if we know A knows B well, update our predictability for B
    pub fn update_transitivity(&self, peer: NodeId, peer_table: &HashMap<NodeId, f64>) {
        let p_us_peer = self.delivery_predictabilities.get(&peer)
            .map(|v| *v).unwrap_or(0.0);

        for (dest, p_peer_dest) in peer_table {
            let p_us_dest = self.delivery_predictabilities.entry(*dest).or_insert(0.0);
            // P(a,c) = P(a,c)_old + (1 - P(a,c)_old) * P(a,b) * P(b,c) * BETA
            *p_us_dest = *p_us_dest + (1.0 - *p_us_dest) * p_us_peer * p_peer_dest * Self::BETA;
        }
    }

    /// Should we forward this message to this peer?
    pub fn should_forward(&self, message_dest: &NodeId, peer: &NodeId) -> bool {
        let our_prob = self.delivery_predictabilities.get(message_dest)
            .map(|v| *v).unwrap_or(0.0);
        let peer_prob = // obtained from peer during contact
            todo!();
        peer_prob > our_prob
    }
}
```

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum IrisError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),

    #[error("Protocol error: {message}")]
    Protocol { message: String },

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Message expired (TTL=0)")]
    MessageExpired,

    #[error("Duplicate message: {id}")]
    Duplicate { id: MessageId },

    #[error("Storage quota exceeded")]
    StorageQuotaExceeded,

    #[error("Invalid priority: {value}")]
    InvalidPriority { value: u8 },

    #[error("Node not found: {id}")]
    NodeNotFound { id: NodeId },

    #[error("Rate limit exceeded for sender: {sender}")]
    RateLimited { sender: NodeId },

    #[error("Shutdown")]
    Shutdown,
}
```

## Memory Safety Notes

- All shared state accessed via `Arc<>` + `Mutex`/`RwLock`. No raw pointers in business logic.
- At FFI boundary: `Arc<IrisCore>` is passed as an opaque pointer to platform layer. UniFFI manages the reference count.
- Private key material stored in types implementing `Zeroize` and `Drop` — zeroed on drop.
- No `unsafe` blocks outside the FFI scaffolding generated by UniFFI (which is audited code).
- `#![deny(unsafe_code)]` in all business logic crates.
