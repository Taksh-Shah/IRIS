# Transport Abstraction Layer

## Overview

The Transport Abstraction Layer is the foundational infrastructure of the IRIS Resilient Communication Fabric. It presents a uniform interface to the routing engine and message layer while encapsulating the radically different operating semantics of BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite, and other bearers. Every transport — whether a hardware radio, a software tunnel, or a simulated channel — is registered at startup and managed through a single `TransportManager`.

---

## Transport Capability Matrix

| Property                  | BLE 5.x        | Wi-Fi Direct     | Wi-Fi Aware      | Wi-Fi Infra      | Ethernet         | Cellular         | LoRa             | Satellite        | USB              |
|---------------------------|----------------|------------------|------------------|------------------|------------------|------------------|------------------|------------------|------------------|
| **Typical Range**         | 10–100 m       | 50–200 m         | 10–300 m         | AP-limited       | LAN only         | 1–50 km cell     | 2–15 km          | Global           | 2 m (cable)      |
| **Peak Bandwidth**        | ~250 kbps      | ~250 Mbps        | ~300 Mbps        | ~100–600 Mbps    | 100 Mbps–10 Gbps | 1–100 Mbps       | ~250 bps–5 kbps  | 2.4 kbps–200 Mbps| 480 Mbps (USB 2) |
| **Practical Throughput**  | 50–100 kbps    | 10–80 Mbps       | 20–100 Mbps      | 10–200 Mbps      | 10 Mbps–1 Gbps   | 1–50 Mbps        | 100–500 bps      | 2–50 Mbps        | 10–40 Mbps       |
| **One-Way Latency**       | 5–50 ms        | 50–500 ms setup  | 1–10 ms          | 1–10 ms          | <1 ms            | 30–200 ms        | 0.5–5 s          | 250–700 ms       | <1 ms            |
| **Battery Impact**        | Low            | High             | Medium           | Medium           | Negligible       | High             | Very Low         | Very High        | Low (charges)    |
| **Infrastructure Needed** | None           | None             | None             | AP required      | Switch/cable     | Cell tower       | None (P2P)       | Satellite + dish | Cable only       |
| **Android Background**    | Partial        | No (wakelock)    | Yes (API 29+)    | Yes              | Yes              | Yes              | Via BLE bridge   | Via USB bridge   | No               |
| **iOS Available**         | Yes            | No (private API) | No               | Yes              | No               | Yes              | No               | Via accessory    | Limited          |
| **iOS Background**        | Partial        | N/A              | N/A              | Yes              | N/A              | Yes              | N/A              | Limited          | N/A              |
| **Special Hardware**      | None           | None             | None             | None             | NIC + cable      | SIM              | LoRa module      | Terminal/dish    | Cable            |
| **Cost Per Message**      | ₹0             | ₹0               | ₹0               | ₹0               | ₹0               | ₹0–₹0.01         | ₹0               | ₹0.05–₹5        | ₹0               |
| **Regulatory**            | ISM (free)     | ISM (free)       | ISM (free)       | ISM/licensed     | None             | Licensed         | WPC 865–867 MHz  | DoT license      | None             |
| **Best Priority Tier**    | P0–P5          | P0–P3 (files)   | P0–P4            | P0–P7            | P0–P7            | P0–P3            | P0–P2            | P0–P1 only       | P0–P2 (emergency)|

---

## TransportManager Design

The `TransportManager` is a singleton owned by the IRIS core daemon. It is responsible for:

1. **Transport registration** — accepting transport implementations at startup and runtime.
2. **Capability advertisement** — publishing what each transport can do to the routing engine.
3. **Transport lifecycle** — driving the state machine for each transport.
4. **Transport selection** — choosing the optimal transport(s) for a given send request.
5. **Multi-transport concurrency** — managing simultaneous use of multiple transports.
6. **Cost accounting** — tracking battery, bandwidth, and monetary cost per transport.

```
┌─────────────────────────────────────────────────────────────┐
│                       Routing Engine                        │
│   (consults TransportManager for capabilities, selects      │
│    transport, calls send_message)                           │
└────────────────────┬────────────────────────────────────────┘
                     │  TransportManager API
┌────────────────────▼────────────────────────────────────────┐
│                    TransportManager                         │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ BleTransport │  │WifiDirTrans. │  │LoRaTransport │ ...  │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                             │
│  Transport Registry   State Table    Cost Ledger            │
└─────────────────────────────────────────────────────────────┘
                     │  Platform Adapter API
┌────────────────────▼────────────────────────────────────────┐
│            Platform Adapters (Android/iOS/Desktop)          │
│   (expose OS-level networking APIs to transport impls)      │
└─────────────────────────────────────────────────────────────┘
```

---

## Transport State Machine

Every transport instance runs through a well-defined state machine. Transitions are driven by hardware events, OS callbacks, and timeout timers.

```
                  ┌───────────────┐
                  │  UNAVAILABLE  │◄─────────────────────────┐
                  └───────┬───────┘                          │
                          │ hardware/permission available     │
                          ▼                                  │
                  ┌───────────────┐                          │
             ┌───►│   AVAILABLE   │◄──────┐                  │
             │   └───────┬───────┘       │                   │
             │           │ connect()     │ disconnect()       │
             │           ▼               │                   │
             │   ┌───────────────┐       │                   │
             │   │  CONNECTING   │───────┘ (timeout/fail)    │
             │   └───────┬───────┘                           │
             │           │ link established                  │
             │           ▼                                   │
             │   ┌───────────────┐                           │
             │   │   CONNECTED   │──── signal loss ─────────►│
             │   └───────┬───────┘          (DEGRADED first) │
             │           │ partial loss                      │
             │           ▼                                   │
             │   ┌───────────────┐                           │
             └───│   DEGRADED    │──── complete loss ───────►┘
                 └───────────────┘
```

**State definitions:**

- `UNAVAILABLE` — hardware off, permission denied, or hardware not present. Transport cannot be used.
- `AVAILABLE` — hardware ready and permissions granted; transport can accept connections but no active link.
- `CONNECTING` — connection attempt in progress (scan, association, negotiation).
- `CONNECTED` — active link with at least one peer; data can be sent and received.
- `DEGRADED` — link exists but quality below threshold (high packet loss, very low signal, bandwidth constrained).

Transitions emit events to the TransportManager, which propagates them to the routing engine.

---

## Transport Trait (Rust Interface)

```rust
/// Core transport abstraction. Every transport implementation must implement this trait.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Unique identifier for this transport instance (e.g., "ble-android", "wifi-aware-0")
    fn transport_id(&self) -> TransportId;

    /// Human-readable name for logging and UI
    fn display_name(&self) -> &str;

    /// Static capabilities of this transport (hardware limitations, protocol limits)
    fn capabilities(&self) -> TransportCapabilities;

    /// Current dynamic state
    fn state(&self) -> TransportState;

    /// Subscribe to state change events
    fn state_stream(&self) -> Pin<Box<dyn Stream<Item = TransportStateEvent> + Send>>;

    /// Discover/scan for nearby peers. Returns a stream of discovered peers.
    async fn discover_peers(&self, config: DiscoveryConfig) -> Result<PeerStream, TransportError>;

    /// Stop discovery
    async fn stop_discovery(&self) -> Result<(), TransportError>;

    /// Advertise this node's presence so other nodes can discover it
    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError>;

    /// Stop advertising
    async fn stop_advertising(&self) -> Result<(), TransportError>;

    /// Open a connection to a specific peer
    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError>;

    /// Send a message to a peer (may internally manage connection lifecycle)
    async fn send(&self, peer: &PeerId, message: &SerializedMessage) -> Result<SendReceipt, TransportError>;

    /// Subscribe to incoming messages from any peer
    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>;

    /// Current cost model snapshot for this transport
    fn cost_snapshot(&self) -> TransportCost;

    /// Hint to the transport: priority of next send (may adjust power settings)
    fn set_send_priority_hint(&self, priority: MessagePriority);

    /// Graceful shutdown
    async fn shutdown(&self) -> Result<(), TransportError>;
}

/// Static capabilities reported by a transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportCapabilities {
    pub max_message_size: usize,           // bytes per send() call
    pub supports_broadcast: bool,           // can address all nearby peers
    pub supports_unicast: bool,
    pub supports_multicast: bool,
    pub typical_range_meters: RangeHint,   // Min/Max/Typical
    pub typical_throughput_bps: u64,
    pub typical_latency_ms: u32,
    pub requires_infrastructure: bool,      // false = P2P capable
    pub supports_background_android: bool,
    pub supports_background_ios: bool,
    pub requires_special_hardware: bool,
    pub cost_class: TransportCostClass,    // FREE, METERED, EXPENSIVE
    pub regulatory_band: Option<RegulatoryBand>,
}

/// Dynamic cost snapshot (updated during operation)
#[derive(Debug, Clone)]
pub struct TransportCost {
    pub estimated_battery_ma: f32,    // current draw estimate
    pub monetary_cost_per_kb: f64,    // INR per kilobyte (0 for free transports)
    pub bandwidth_available_bps: u64, // current estimated bandwidth
    pub congestion_level: f32,        // 0.0 = no congestion, 1.0 = fully congested
}
```

---

## Transport Registration

Transports register with the `TransportManager` at startup. The manager does not know about concrete transport types at compile time — they are injected by platform adapters.

```rust
pub struct TransportManager {
    registry: Arc<RwLock<HashMap<TransportId, Arc<dyn Transport>>>>,
    state_cache: Arc<RwLock<HashMap<TransportId, TransportState>>>,
    cost_ledger: Arc<Mutex<CostLedger>>,
    routing_notifier: mpsc::Sender<TopologyEvent>,
}

impl TransportManager {
    /// Register a new transport. Can be called at runtime (e.g., when LoRa dongle plugged in).
    pub async fn register(&self, transport: Arc<dyn Transport>) -> Result<(), RegistrationError> {
        let id = transport.transport_id();
        let caps = transport.capabilities();

        // Subscribe to state changes before inserting
        let state_stream = transport.state_stream();
        let notifier = self.routing_notifier.clone();
        let id_clone = id.clone();
        tokio::spawn(async move {
            let mut stream = state_stream;
            while let Some(event) = stream.next().await {
                let _ = notifier.send(TopologyEvent::TransportStateChanged {
                    transport_id: id_clone.clone(),
                    new_state: event.new_state,
                }).await;
            }
        });

        let mut registry = self.registry.write().await;
        registry.insert(id.clone(), transport);
        Ok(())
    }

    /// Deregister a transport (e.g., LoRa dongle unplugged)
    pub async fn deregister(&self, id: &TransportId) -> Result<(), RegistrationError> {
        let mut registry = self.registry.write().await;
        if let Some(transport) = registry.remove(id) {
            transport.shutdown().await.ok();
        }
        Ok(())
    }
}
```

---

## Transport Selection Logic

The routing engine calls `TransportManager::select_transports()` when it needs to send a message. Selection is based on:

1. **Reachability** — does this transport have a path to the target peer?
2. **Priority class** — P0/P1 messages use any available transport; P6/P7 use only cheap ones.
3. **Cost model** — minimize battery drain for non-emergency messages.
4. **State** — prefer CONNECTED over AVAILABLE (avoids connection overhead).
5. **Message size** — avoid LoRa for messages > 250 bytes unless no other option.

```rust
pub struct TransportSelectionRequest {
    pub target_peer: Option<PeerId>,        // None = broadcast
    pub message_size: usize,
    pub priority: MessagePriority,
    pub max_latency_ms: Option<u32>,
    pub prefer_low_cost: bool,
    pub multipath: bool,                    // true = select multiple transports
}

impl TransportManager {
    pub async fn select_transports(
        &self,
        req: &TransportSelectionRequest,
    ) -> Vec<RankedTransport> {
        let registry = self.registry.read().await;

        let mut candidates: Vec<RankedTransport> = registry
            .values()
            .filter(|t| t.state() >= TransportState::Available)
            .filter(|t| t.capabilities().max_message_size >= req.message_size)
            .map(|t| {
                let score = self.score_transport(t, req);
                RankedTransport { transport: t.clone(), score }
            })
            .collect();

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        if req.multipath && req.priority <= MessagePriority::P1 {
            // P0/P1: use all viable transports simultaneously
            candidates.retain(|c| c.score > 0.0);
        } else {
            // Single best transport
            candidates.truncate(1);
        }

        candidates
    }

    fn score_transport(&self, transport: &Arc<dyn Transport>, req: &TransportSelectionRequest) -> f32 {
        let caps = transport.capabilities();
        let cost = transport.cost_snapshot();
        let state = transport.state();

        let mut score = 0.0_f32;

        // State bonus
        score += match state {
            TransportState::Connected  => 40.0,
            TransportState::Available  => 20.0,
            TransportState::Degraded   => 5.0,
            _                          => -1000.0, // eliminates this transport
        };

        // Latency bonus for emergency messages
        if req.priority <= MessagePriority::P1 {
            score += 100.0 / (caps.typical_latency_ms as f32 + 1.0);
        }

        // Cost penalty for metered transports on non-emergency messages
        if req.prefer_low_cost || req.priority >= MessagePriority::P4 {
            score -= cost.monetary_cost_per_kb as f32 * 50.0;
            score -= cost.estimated_battery_ma * 0.5;
        }

        // Bandwidth bonus if message is large
        if req.message_size > 10_000 {
            score += (cost.bandwidth_available_bps as f32).log2() * 5.0;
        }

        // Congestion penalty
        score -= cost.congestion_level * 30.0;

        score
    }
}
```

---

## Multi-Transport Concurrency

IRIS can simultaneously use multiple transports. Key concurrency considerations:

**Concurrent Discovery:** BLE discovery and Wi-Fi Aware discovery run simultaneously. Both report discovered peers to the same peer registry. Deduplication occurs by public key fingerprint, not by transport-specific address.

**Concurrent Send (Multipath):** For P0/P1 messages, the message may be sent on BLE, Wi-Fi Direct, and LoRa simultaneously. The receiver deduplicates by message ID. First arrival wins; subsequent copies are discarded.

**Transport Interference:** Some transports share hardware:
- BLE and Wi-Fi Aware may share the 2.4 GHz radio on low-end devices. Android handles coexistence at OS level, but throughput degrades.
- Wi-Fi Direct and Wi-Fi Aware conflict on some chipsets — only one can be active. TransportManager tracks this via a `RadioConflictGroup` abstraction and ensures only one transport in a conflict group is CONNECTED at a time unless the hardware supports simultaneous operation.

```rust
pub enum RadioConflictGroup {
    Bluetooth24GHz,   // BLE, classic Bluetooth
    WiFi24GHz,        // Wi-Fi, Wi-Fi Direct, Wi-Fi Aware (share RF front-end)
    WiFi5GHz,         // Wi-Fi 5 GHz band
    SubGHz,           // LoRa, Zigbee, 915MHz ISM
    Cellular,         // LTE/5G modem
    None,             // No conflict (Ethernet, USB)
}
```

---

## Platform Adapter Architecture

Platform adapters sit below the Transport implementations. They abstract OS-specific networking APIs.

```
┌─────────────────────────────────────────┐
│          BleTransport (Rust)            │  ← Transport trait implementation
└────────────────────┬────────────────────┘
                     │ calls
┌────────────────────▼────────────────────┐
│         BleAdapter trait (Rust)         │  ← Platform-neutral adapter interface
└────┬──────────────────────────┬─────────┘
     │                          │
┌────▼────────────┐    ┌────────▼────────┐
│ AndroidBleAdapt │    │  IosBleAdapter  │  ← Platform-specific implementations
│ (Kotlin JNI)    │    │  (Swift/ObjC)   │    called via FFI
└─────────────────┘    └─────────────────┘
```

Platform adapters are implemented in the native language (Kotlin for Android, Swift for iOS) and exposed to the Rust core via the `uniffi` or manual FFI layer. Each adapter conforms to a Rust trait that the transport implementation depends on.

```rust
#[uniffi::export]
pub trait BleAdapter: Send + Sync {
    fn start_scan(&self, filter: ScanFilter) -> Result<ScanHandle, BleError>;
    fn stop_scan(&self, handle: ScanHandle);
    fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError>;
    fn stop_advertising(&self, handle: AdvHandle);
    fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError>;
    fn disconnect_gatt(&self, handle: GattHandle);
    fn gatt_write(&self, handle: GattHandle, char_uuid: Uuid, data: Vec<u8>) -> Result<(), BleError>;
    fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError>;
    fn incoming_gatt_writes(&self) -> Box<dyn Iterator<Item = GattWriteEvent>>;
}
```

---

## Transport Priority and Cost Model

### Priority Classes

| Priority | Description              | Transport Policy                          |
|----------|--------------------------|-------------------------------------------|
| P0       | Mass casualty alert      | ALL available transports, multipath       |
| P1       | Emergency SOS            | ALL available transports, multipath       |
| P2       | Critical coordination    | Best 2 transports, avoid expensive only   |
| P3       | Urgent coordination      | Best transport, fallback to second        |
| P4       | Important messages       | Single best cheap transport               |
| P5       | Normal messages          | Cheapest adequate transport               |
| P6       | Background sync          | Free transports only, deferrable          |
| P7       | Bulk/non-urgent data     | Free transports only, highly deferrable   |

### Battery Cost Model

Each transport reports an estimated current draw in milliamps. The TransportManager accumulates this into a `SessionBatteryCost` estimate that can be surfaced to users.

```rust
pub struct BatteryCostModel {
    pub scan_ma: f32,        // current draw during active discovery scan
    pub advertise_ma: f32,   // current draw during advertising
    pub connected_idle_ma: f32,
    pub tx_ma_per_kbps: f32, // additional draw per kbps of TX
    pub rx_ma_per_kbps: f32,
}

// Example values:
const BLE_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 8.0,
    advertise_ma: 5.0,
    connected_idle_ma: 3.0,
    tx_ma_per_kbps: 0.02,
    rx_ma_per_kbps: 0.01,
};

const WIFI_AWARE_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 25.0,
    advertise_ma: 15.0,
    connected_idle_ma: 12.0,
    tx_ma_per_kbps: 0.005,
    rx_ma_per_kbps: 0.003,
};

const LORA_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 1.5,
    advertise_ma: 0.5,
    connected_idle_ma: 1.2,
    tx_ma_per_kbps: 10.0,   // LoRa TX is proportionally expensive
    rx_ma_per_kbps: 1.5,
};
```

### Monetary Cost Model

```rust
pub enum TransportCostClass {
    Free,                    // BLE, Wi-Fi, LoRa (no per-message cost)
    Metered {
        cost_per_kb_inr: f64,
    },
    Expensive {              // Satellite Iridium
        cost_per_message_inr: f64,
        minimum_cost_inr: f64,
    },
}
```

For satellite transports, the TransportManager enforces a policy: monetary-expensive transports are only used for P0–P1 messages, and the user is warned before the first satellite send with an estimated cost.

---

## Capability Advertisement to Routing Engine

The routing engine receives `TransportCapabilityAdvertisement` events whenever transport state changes. This is the mechanism by which the routing engine learns the current network topology.

```rust
pub enum TopologyEvent {
    /// A transport changed state
    TransportStateChanged {
        transport_id: TransportId,
        new_state: TransportState,
    },
    /// A new peer was discovered via some transport
    PeerDiscovered {
        peer: PeerInfo,
        via_transport: TransportId,
        link_quality: LinkQuality,
    },
    /// A peer went out of range / disconnected
    PeerLost {
        peer_id: PeerId,
        via_transport: TransportId,
    },
    /// Link quality changed (RSSI update, throughput measurement)
    LinkQualityUpdated {
        peer_id: PeerId,
        transport_id: TransportId,
        new_quality: LinkQuality,
    },
    /// A gateway became available or unavailable
    GatewayChanged {
        gateway_id: PeerId,
        gateway_type: GatewayType,
        available: bool,
    },
}
```

The routing engine consumes these events to maintain a real-time graph of reachable nodes and available paths.

---

## Transport Shutdown and Recovery

### Graceful Shutdown

On app backgrounding or explicit shutdown, `TransportManager::shutdown()` is called. Each transport's `shutdown()` is called concurrently with a 5-second timeout. Transports that fail to shut down gracefully are forcibly dropped.

### Recovery After Failure

If a transport enters `UNAVAILABLE` after being `CONNECTED` (e.g., BLE hardware error), the TransportManager:
1. Notifies the routing engine immediately.
2. Schedules a recovery attempt (exponential backoff: 1s, 2s, 4s, 8s, up to 60s).
3. On recovery attempt: calls `transport.start_advertising()` and `transport.discover_peers()`.
4. If recovery fails after 5 attempts, emits `TransportPermanentFailure` event.

---

## SimulatedTransport

For testing and development without physical hardware, each transport has a `Simulated` variant:

```rust
pub struct SimulatedTransport {
    pub id: TransportId,
    pub caps: TransportCapabilities,
    pub sim_config: SimConfig,
}

pub struct SimConfig {
    pub packet_loss_rate: f32,     // 0.0–1.0
    pub bandwidth_bps: u64,
    pub latency_ms_distribution: LatencyDistribution,
    pub intermittent_failures: Vec<FailureWindow>,
    pub simulated_peers: Vec<SimulatedPeer>,
}
```

The simulation layer intercepts all send/receive calls and injects configured losses, delays, and disconnections. This allows the routing engine to be tested under realistic disaster conditions without physical infrastructure.
