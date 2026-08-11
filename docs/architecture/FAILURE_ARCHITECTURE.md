# Failure Architecture

## Design Principle
The system degrades gracefully. No single failure causes complete communication loss. Every layer has defined failure behavior and recovery strategy. The design philosophy: assume failure, design for partial function, preserve emergency communication above all else.

Failure handling is not an afterthought — it is the primary design mode. IRIS is built for the moment when everything else fails.

## Failure Categories

### Transport Failure
**Definition:** A physical transport becomes unavailable: BLE adapter disabled, Wi-Fi Direct group formation fails, LoRa module disconnected, network cable unplugged.

**Detection:**
- Transport adapter reports explicit error (OS callback, ENODEV, etc.)
- No receive activity for > `transport_silence_timeout` (30s BLE, 5min LoRa)
- Send attempts return consecutive failures (>3 within 60s)

**Immediate Behavior:**
- Transport Manager marks transport `UNAVAILABLE`
- Routing Engine removes affected links from active graph
- Messages queued for that transport are moved to alternate transport queue if alternate exists
- If no alternate: messages remain queued, await transport recovery

**Recovery:**
- Periodic probe: attempt to re-open transport every 30s (BLE), 5min (LoRa)
- OS notification: listen for OS transport state change events (preferred over polling)
- On recovery: re-advertise, re-scan, restore links to previously seen neighbors

**Worst Case:**
All transports fail simultaneously. All messages stored locally. System waits in OFFLINE state. When any transport recovers, stored messages are immediately queued for forwarding.

### Gateway Failure
**Definition:** An Internet gateway, LoRa gateway, or satellite terminal loses its external connection or becomes unreachable from local node.

**Detection:**
- Gateway heartbeat timeout (no response in 60s)
- Gateway's CapabilityBundle updated: gateway_types no longer includes affected type
- Direct probe: send test message to known external endpoint, no response

**Immediate Behavior:**
- Gateway selection removes failed gateway from routing
- Messages previously routed via failed gateway re-routed via alternate gateway
- If no alternate gateway: enter mesh-only mode, store messages with gateway as pending recipient

**Recovery:**
- Automatic: gateway re-appears in discovery, re-adds to routing
- Message flush: all stored messages destined for external recipients queued through restored gateway

**P0 Impact:**
If satellite gateway is available and Internet fails, P0 messages automatically re-route to satellite. Operator must ensure satellite costs are covered for this failover to function.

### Node Disappearance
**Definition:** A previously active node (neighbor) disappears without graceful shutdown: battery death, crash, user leaves area.

**Detection:**
- Neighbor table expiry: node not seen for `neighbor_timeout` (30s BLE, 5min LoRa)
- Transport adapter disconnection event (where available: BLE GATT disconnect callback)

**Immediate Behavior:**
- Node removed from active neighbor table (but retained in graph with `is_active = false`)
- Routing table: delivery probability for disappeared node decays over time
- In-flight messages to disappeared node: re-queued for alternate path or stored for re-contact

**In-Flight Message Handling:**
Messages that were mid-transfer when node disappeared:
- Fragmented messages: incomplete fragments discarded
- Store-carry-forward: stored messages retained until TTL
- If CONFIRMED_DELIVERY: ACK timeout triggers alternate path attempt

**Recovery:**
Node re-appears → rediscovery → sync → stored messages forwarded.

### Network Partition
**Definition:** Physical separation of the network into two or more disconnected components with no path between them.

**Detection:**
- PartitionDetector: BFS from local node finds previously-reachable nodes now unreachable
- All gateways unreachable, all cross-partition links expired
- Symptoms: delivery probability drops across the board, ACK timeouts increase

**Immediate Behavior:**
- Each partition operates independently as a complete mesh
- Store-carry-forward activated within each partition
- Epidemic routing aggressiveness increased (higher spray count)
- Storage limits relaxed to hold more messages for cross-partition delivery
- Alert sent to application layer: user sees "Network partitioned" indicator

**Vehicle Relay (DTN Bridge):**
If a vehicle relay is operating:
- Vehicle collects messages from partition A
- Travels to partition B
- Delivers messages
- This is the primary recovery mechanism for physical partitions

**Recovery:**
When partitions reconnect (bridge node appears, vehicle relay, physical infrastructure restored):
1. Discovery: nodes from other partition appear in scan
2. NetworkHealingSync triggered immediately
3. Bloom filter exchange → mass sync of stored messages
4. Routing tables merged
5. Application layer notified: "Network healed"

### Storage Full
**Definition:** Local storage quota for message store is exhausted.

**Detection:**
- SQLite returns SQLITE_FULL on message insert
- Pre-emptive: storage monitor checks every 5 minutes, alerts at 80% and 90% capacity

**Eviction Policy (Priority Order):**
```
1. Evict P7 (Video/Bulk) first — largest, lowest priority
   Within P7: evict soonest-expiring first (TTL-based)
2. Evict P6 (Voice) — if P7 exhausted
3. Evict P5 (Image) — if P6 exhausted
4. Evict P4 (Normal text) — only oldest messages near TTL expiry
5. NEVER evict P0 (SOS) — P0 retained until TTL expires naturally
6. NEVER evict P1 (Medical) unless storage critically full and TTL < 10% remaining
7. NEVER evict P2 (Location) without logging
```

**User Notification:**
- Alert at 80%: "Storage almost full. Oldest media messages may be removed."
- Alert at 95%: "Storage full. Some messages are being removed to make space for emergencies."

**P0 Guarantee:**
P0 SOS messages are never evicted due to storage pressure. A P0 message occupies at most 255 bytes. A 500MB quota can hold ~2 million P0 messages. This guarantee is absolute.

### Battery Exhaustion
**Definition:** Device battery drops to critical level, risking full shutdown.

**Detection:**
- OS battery level event (Android: BatteryManager, iOS: UIDevice.batteryLevel)
- Threshold: CRITICAL mode at < 5% battery

**Behavior by Battery Level:**

```
Battery Level | Mode           | Transports Active          | Relay Scope
> 50%         | FULL           | All                        | All priorities
20-50%        | BALANCED       | BLE preferred, Wi-Fi OD    | P0-P4
10-20%        | LOW_BATTERY    | BLE only, minimal scan     | P0-P2
5-10%         | CRITICAL       | BLE 30s scan interval      | P0 only
< 5%          | BATTERY_CRITICAL| No active scan            | P0 relay only (passive)
P0 ACTIVE     | EMERGENCY_OVR  | All transports             | Maximum
```

**Rationale for Aggressive Power Saving:**
A dead phone is worse than a throttled phone. Preserving 5% battery for emergency origination (if user needs to send SOS) is more valuable than exhausting battery on background relay.

**Emergency Override:**
When a P0 SOS is actively in relay: temporarily override battery policy. A single P0 relay costs ~0.1% battery on BLE. This is acceptable even at 2% battery.

### Clock Skew
**Definition:** Local clock differs from network consensus time by more than acceptable threshold (30 minutes).

**Detection:**
- On every node contact: compare timestamps
- Log: `clock_skew = abs(local_time - peer_time)` for each contact
- Alert: if clock_skew > 30 minutes

**Behavior:**
- TTL enforcement: use conservative interpretation (expire sooner of local vs peer interpretation)
- Do NOT adjust system clock (security: malicious nodes could inject clock attacks)
- Log skew for diagnostics, report to operator when Internet available

**Mitigation:**
- GPS provides accurate time when available (trust GPS time)
- Internet NTP when gateway available
- Peer time exchange on contact (for relative consistency, not absolute accuracy)

### Malicious Relay
**Definition:** A relay node actively interferes: drops messages (blackhole), modifies messages (tampering), or replays old messages.

**Blackhole Attack:**
Node receives messages but silently drops them instead of relaying.
- Detection: ACK timeout on messages routed through suspected node
- Mitigation: multi-path routing for P0-P2 (send via multiple relay paths simultaneously)
- Response: routing engine penalizes delivery probability for non-delivering nodes
- Limitation: hard to prove vs. simple connectivity failure without end-to-end confirmation

**Tamper Attack:**
Node modifies message content.
- Prevention: Ed25519 signature covers entire envelope; modification invalidates signature
- Detection: every relay verifies signature; tampered message detected and dropped
- Guarantee: successfully delivered message is byte-for-byte identical to originated message

**Replay Attack:**
Node re-transmits previously seen messages to consume resources.
- Prevention: deduplication (seen_message_cache) at every relay node
- message_id includes timestamp (UUIDv7); old replays detected as expired (TTL check)
- Limitation: within TTL window, replay not directly detectable; deduplication prevents most harm

**Sybil Attack:**
Malicious node creates many fake identities to dominate routing.
- Mitigation: cryptographic identities are not free (require Ed25519 key generation, negligible but present)
- IRIS does not rely on "majority trust" so Sybil resistance is less critical than in some systems
- Future: proof-of-work or vouching systems could increase Sybil cost

## Cascading Failure Prevention

### Message Loops
**Risk:** Message M is forwarded by A to B, B to C, C back to A, and the cycle repeats.
**Prevention:**
- Hop count limit: `max_hops` (default 20). Decrement on each relay.
- Deduplication: `seen_message_cache` prevents re-processing any seen message_id.
- These two mechanisms together guarantee loop termination.

### Broadcast Storms
**Risk:** Emergency broadcast causes every node to immediately retransmit to every neighbor, consuming all available bandwidth.
**Prevention:**
- Deduplication: seen_message_cache prevents re-broadcasting a message already relayed.
- Rate limiting: max 1 relay per unique message_id (hard guarantee via deduplication).
- Emergency broadcasts are bounded by the network size.

### Queue Overflow (Node-Level)
**Risk:** A single high-traffic node accumulates infinite queue depth, starving memory.
**Prevention:**
- Priority eviction: when queue is full, lowest-priority messages evicted first.
- P0 queue capacity is separate from P4-P7 queue.
- Queue depth alert at 10,000 messages; max at 50,000 (then evict P7).

### Transport Overload
**Risk:** One transport (BLE) is flooded with relay traffic, preventing its use for discovery.
**Prevention:**
- Per-transport rate limits: max send rate per transport
- Discovery traffic reserved on separate BLE advertisement channel
- BLE data and discovery are separated: GATT for data, advertisement for discovery

## Graceful Degradation Ladder

Each rung represents a stable operating mode. The system moves down when conditions worsen, up when they improve. At no rung does communication completely stop.

```
Rung 1: All systems nominal
  Features: Full features, all transports, Internet gateway
  Throughput: High (Wi-Fi, Internet)
  Latency: Low

Rung 2: Internet gateway unavailable
  Features: Mesh-only, store-carry-forward activated
  Throughput: Medium (Wi-Fi Direct, BLE)
  Latency: Higher (multi-hop)

Rung 3: Cellular + Internet unavailable
  Features: Local mesh only, LoRa if available
  Throughput: Low-Medium (BLE, LoRa)
  Latency: Variable (store-carry-forward)

Rung 4: Gateway failure, satellite fallback
  Features: Mesh + satellite for P0-P1
  Throughput: Very low (satellite bandwidth)
  Latency: High (satellite)

Rung 5: Battery critical
  Features: Emergency-only mode
  Throughput: Minimal (BLE low power)
  Latency: High

Rung 6: Single transport remaining
  Features: Minimum viable comms on last transport
  Throughput: Whatever transport allows
  Latency: Variable

Rung 7: Complete isolation
  Features: Store all messages, wait for any contact
  Throughput: Zero (storing only)
  Latency: Until contact opportunity appears
  Note: P0 SOS still stored and ready to send when ANY contact appears
```

## Failure Testing Strategy

Failures must be tested, not just designed for. Test categories:
- **Fault injection**: artificially induce transport failures, storage full, clock skew
- **Chaos testing**: random node disappearance during message relay
- **Partition simulation**: split virtual network into disconnected graphs
- **Byzantine node testing**: node that drops, modifies, or replays messages
- **Battery simulation**: power mode transitions under load

Test scenarios are documented in `tests/failure_scenarios/`. Each failure in this document has a corresponding automated test.
