# IRIS Observability Design

**Document Type:** Operations Reference  
**Status:** DRAFT  
**Audience:** Platform operators, field engineers, DevOps

---

## 1. Observability Philosophy

IRIS is a decentralized, infrastructure-light system. Observability must work under the same constraints as the product itself: no persistent central server, intermittent connectivity, privacy-first. This means:

- **Pull-based where possible**: Operators query nodes rather than nodes pushing to central aggregator
- **Privacy-preserving**: No message content, no contact lists, no precise location in any telemetry
- **Degraded-mode friendly**: Observability must function even when the mesh has limited connectivity
- **On-device first**: Each node maintains its own metrics ring buffer; export is opportunistic

---

## 2. Metrics Taxonomy

### 2.1 Network Health Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.mesh.neighbors.active` | Gauge | count | Active neighbors seen in last 60s |
| `iris.mesh.neighbors.seen_1h` | Gauge | count | Unique neighbors seen in last 1 hour |
| `iris.mesh.hops.max_observed` | Gauge | hops | Maximum hop count seen in routing table |
| `iris.mesh.partition.detected` | Gauge | bool | 1 if node believes itself isolated |
| `iris.mesh.topology.change_rate` | Counter | events/min | Rate of neighbor add/remove events |
| `iris.mesh.reachability.score` | Gauge | 0.0–1.0 | Network reachability health score |

### 2.2 Message Delivery Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.messages.sent` | Counter | count | Messages originated by this node |
| `iris.messages.received` | Counter | count | Messages received (for self) |
| `iris.messages.relayed` | Counter | count | Messages forwarded on behalf of others |
| `iris.messages.dropped.ttl` | Counter | count | Messages dropped due to TTL expiry |
| `iris.messages.dropped.queue_full` | Counter | count | Messages dropped due to queue full |
| `iris.messages.dropped.duplicate` | Counter | count | Duplicate messages suppressed |
| `iris.messages.delivery_ratio` | Gauge | 0.0–1.0 | Ratio of sent:acked for own messages |
| `iris.messages.latency_p50` | Gauge | ms | 50th percentile delivery latency |
| `iris.messages.latency_p95` | Gauge | ms | 95th percentile delivery latency |
| `iris.messages.latency_p99` | Gauge | ms | 99th percentile delivery latency |
| `iris.messages.queue.depth` | Gauge | count | Current outbound queue depth |
| `iris.messages.store_carry.stored` | Gauge | count | Messages in store-carry-forward buffer |

### 2.3 Transport Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.transport.ble.state` | Gauge | enum | BLE state: OFF/SCANNING/ADVERTISING/CONNECTED |
| `iris.transport.ble.connections.active` | Gauge | count | Active BLE connections |
| `iris.transport.ble.bytes_tx` | Counter | bytes | Total bytes sent via BLE |
| `iris.transport.ble.bytes_rx` | Counter | bytes | Total bytes received via BLE |
| `iris.transport.ble.errors` | Counter | count | BLE-level errors |
| `iris.transport.wifi.state` | Gauge | enum | Wi-Fi transport state |
| `iris.transport.wifi.bytes_tx` | Counter | bytes | Total bytes sent via Wi-Fi |
| `iris.transport.wifi.bytes_rx` | Counter | bytes | Total bytes received via Wi-Fi |
| `iris.transport.lora.state` | Gauge | enum | LoRa gateway connection state |
| `iris.transport.lora.rssi_avg` | Gauge | dBm | Average LoRa RSSI |
| `iris.transport.lora.snr_avg` | Gauge | dB | Average LoRa SNR |
| `iris.transport.satellite.state` | Gauge | enum | Satellite link state |
| `iris.transport.internet.available` | Gauge | bool | Whether internet bridge is reachable |

### 2.4 Node Health Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.node.uptime` | Gauge | seconds | Node uptime since last restart |
| `iris.node.battery_level` | Gauge | % | Battery percentage (mobile nodes) |
| `iris.node.battery_drain_rate` | Gauge | %/hour | Battery consumption rate |
| `iris.node.memory.used` | Gauge | bytes | Memory used by IRIS process |
| `iris.node.storage.used` | Gauge | bytes | Storage used for message store |
| `iris.node.storage.quota` | Gauge | bytes | Configured storage quota |
| `iris.node.cpu_time` | Counter | ms | CPU time consumed |
| `iris.node.identity.verified` | Gauge | bool | Whether node identity is verified |
| `iris.node.version` | Info | string | Software version |

### 2.5 Security Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.security.handshakes.attempted` | Counter | count | Noise handshakes attempted |
| `iris.security.handshakes.failed` | Counter | count | Noise handshakes failed |
| `iris.security.handshakes.succeeded` | Counter | count | Noise handshakes succeeded |
| `iris.security.rate_limit.triggered` | Counter | count | Rate limit events |
| `iris.security.replay.blocked` | Counter | count | Replay attacks blocked |
| `iris.security.unknown_sender.dropped` | Counter | count | Messages from unknown identities dropped |

### 2.6 Emergency System Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.emergency.sos.sent` | Counter | count | SOS messages originated |
| `iris.emergency.sos.relayed` | Counter | count | SOS messages relayed |
| `iris.emergency.sos.delivered_ack` | Counter | count | SOS messages with delivery ack |
| `iris.emergency.broadcast.received` | Counter | count | Emergency broadcasts received |
| `iris.emergency.channel.active` | Gauge | bool | Whether emergency channel is active |
| `iris.emergency.authority.verified` | Gauge | bool | Whether emergency authority is reachable |

### 2.7 Routing Metrics

| Metric Name | Type | Unit | Description |
|---|---|---|---|
| `iris.routing.table.size` | Gauge | count | Number of routing table entries |
| `iris.routing.delivery_predictions.count` | Gauge | count | PROPHET delivery prediction entries |
| `iris.routing.spray.copies_active` | Gauge | count | Active spray-and-wait message copies |
| `iris.routing.decisions.per_second` | Gauge | count | Routing decisions made per second |

---

## 3. Network Health Score

The network health score is a composite 0.0–1.0 value computed per node. It is the primary at-a-glance health indicator.

### 3.1 Score Formula

```
health_score = (
    neighbor_score       * 0.30 +
    delivery_ratio_score * 0.30 +
    transport_score      * 0.20 +
    queue_score          * 0.10 +
    emergency_score      * 0.10
)
```

### 3.2 Component Scoring

**neighbor_score**:
- >= 5 neighbors: 1.0
- 3–4 neighbors: 0.75
- 1–2 neighbors: 0.50
- 0 neighbors: 0.0

**delivery_ratio_score**:
- >= 0.90 delivery ratio: 1.0
- 0.70–0.89: 0.75
- 0.50–0.69: 0.50
- < 0.50: 0.25
- No data (new node): 0.80 (assume healthy)

**transport_score**:
- >= 2 transports active: 1.0
- 1 transport active: 0.50
- No transport active: 0.0

**queue_score**:
- Queue < 50% full: 1.0
- Queue 50–80% full: 0.50
- Queue > 80% full: 0.0

**emergency_score**:
- Emergency channel active AND authority reachable: 1.0
- Emergency channel active, authority unreachable: 0.75
- Emergency channel inactive: 0.50

### 3.3 Health Score Thresholds

| Score | Status | Color | Meaning |
|---|---|---|---|
| 0.85–1.0 | HEALTHY | Green | Normal operation |
| 0.65–0.84 | DEGRADED | Yellow | Some impairment, monitor |
| 0.40–0.64 | POOR | Orange | Significant degradation, investigate |
| 0.0–0.39 | CRITICAL | Red | Severe degradation, action required |

---

## 4. Dashboard Design

### 4.1 Operator Dashboard (Web/Desktop)

The operator dashboard is available via the IRIS admin interface for gateway nodes and desktop installs.

**Panel 1: Network Overview**
- Network health score (large, color-coded)
- Total active nodes in region
- Total messages in-flight
- Active transports across mesh

**Panel 2: Node List**
- Table: Node ID (truncated, anonymized), transport, neighbor count, health score, last seen
- Sortable by health score ascending (worst first)
- Click node to see node detail

**Panel 3: Message Flow**
- Rate chart: messages/minute (sent, received, relayed)
- Delivery ratio time series (15-minute rolling)
- Queue depth time series

**Panel 4: Transport Status**
- Per-transport: BLE, Wi-Fi, LoRa, Satellite, Internet
- Bytes/minute chart per transport
- Error rate per transport

**Panel 5: Emergency System**
- Emergency channel status (active/inactive)
- SOS events in last 24 hours (count, not content)
- Authority reachability status

**Panel 6: Alerts**
- Active alerts list with severity and time
- Alert history (last 7 days)

### 4.2 Field Operator View (Mobile)

Simplified single-screen view for field use:

- Health score (large circle, color-coded)
- Neighbor count
- Active transports (icons)
- Last message time
- Emergency channel: ACTIVE/INACTIVE
- Battery drain rate vs baseline

### 4.3 Gateway Dashboard

For dedicated gateway nodes (Raspberry Pi, etc.):

- Connected nodes count by transport
- Throughput (bytes/second in/out per transport)
- LoRa RF metrics (RSSI, SNR, packet error rate)
- Satellite link quality (if applicable)
- Internet bridge status
- Uptime and resource usage (CPU, RAM, storage, temperature)

---

## 5. Alert Conditions

### 5.1 P0 Alerts (Emergency — Page immediately)

| Alert | Condition | Action |
|---|---|---|
| EMERGENCY_CHANNEL_DOWN | Emergency channel inactive for > 5 minutes | Immediate investigation |
| SOS_DELIVERY_FAILURE | SOS message sent, no ack after 60s with neighbors present | Investigate routing |
| GATEWAY_ALL_DOWN | All gateways unreachable for > 10 minutes | Failover activation |
| SECURITY_HANDSHAKE_STORM | > 100 handshake failures/minute | Possible attack, isolate |

### 5.2 P1 Alerts (High — Respond within 1 hour)

| Alert | Condition |
|---|---|
| DELIVERY_RATIO_LOW | Delivery ratio < 0.50 for 5+ minutes |
| QUEUE_SATURATION | Queue > 90% full for 2+ minutes |
| TRANSPORT_ALL_DOWN | All transports inactive for > 2 minutes |
| NEIGHBOR_ISOLATION | 0 neighbors for > 5 minutes when expected to have connectivity |
| STORAGE_CRITICAL | Storage > 90% of quota |

### 5.3 P2 Alerts (Medium — Respond within 4 hours)

| Alert | Condition |
|---|---|
| DELIVERY_RATIO_DEGRADED | Delivery ratio < 0.70 for 15+ minutes |
| BATTERY_DRAIN_HIGH | Battery drain > 10%/hour in background |
| SINGLE_TRANSPORT | Only 1 transport active for > 30 minutes |
| STORAGE_HIGH | Storage > 75% of quota |
| ROUTING_TABLE_LARGE | Routing table > 10,000 entries |

### 5.4 P3 Alerts (Low — Review next business day)

| Alert | Condition |
|---|---|
| VERSION_MISMATCH | Node running old version |
| UPTIME_SHORT | Node restarted unexpectedly |
| RELAY_LOAD_HIGH | Relay load > expected threshold |

---

## 6. Observability API

The IRIS observability API is a local HTTP API available on gateway nodes and desktop installs. Mobile nodes expose a local socket API.

### 6.1 Endpoints

```
GET  /v1/metrics                    - All current metrics (Prometheus text format)
GET  /v1/metrics/json               - All current metrics (JSON)
GET  /v1/health                     - Network health score and status
GET  /v1/health/score               - Single health score float
GET  /v1/nodes                      - List of known nodes (anonymized IDs)
GET  /v1/nodes/{node_id}            - Single node detail
GET  /v1/transports                 - Transport status for this node
GET  /v1/routing/table              - Routing table snapshot (anonymized)
GET  /v1/queue/status               - Message queue status
GET  /v1/emergency/status           - Emergency system status
GET  /v1/alerts                     - Active alerts
GET  /v1/alerts/history             - Alert history (last 7 days)
POST /v1/diagnostics/export         - Trigger diagnostic bundle export
```

### 6.2 Metrics Export Format

Prometheus text format (standard):
```
# HELP iris_mesh_neighbors_active Active mesh neighbors in last 60 seconds
# TYPE iris_mesh_neighbors_active gauge
iris_mesh_neighbors_active{node="anon-a1b2"} 4 1704067200000

# HELP iris_messages_delivery_ratio Message delivery ratio
# TYPE iris_messages_delivery_ratio gauge
iris_messages_delivery_ratio{node="anon-a1b2",priority="normal"} 0.87
iris_messages_delivery_ratio{node="anon-a1b2",priority="emergency"} 0.99
```

JSON format:
```json
{
  "timestamp": "2025-01-01T00:00:00Z",
  "node_id_hash": "sha256:a1b2c3...",
  "metrics": {
    "iris.mesh.neighbors.active": 4,
    "iris.messages.delivery_ratio": 0.87,
    "iris.node.battery_level": 72,
    "iris.mesh.reachability.score": 0.84
  }
}
```

### 6.3 API Authentication

Local API (loopback only): No authentication required.  
Remote API (if enabled for gateway): mTLS with operator certificate.  
Rate limit: 60 requests/minute per client.

---

## 7. Privacy Constraints on Telemetry

**Absolute prohibitions — never in any metric or export:**
- Message content (any part of any message body)
- Full node identifiers (only hashed/truncated forms)
- Contact lists or social graph
- Precise GPS coordinates (city/district level at finest)
- Individual user identifiers
- Message sender/recipient pairs
- Conversation metadata

**Permitted in metrics:**
- Aggregate counts (messages sent, relayed, received)
- Performance statistics (latency percentiles, throughput)
- Error rates and types (no detail that could identify target)
- Network topology statistics (neighbor count, not neighbor identities)
- Transport states (BLE on/off, not what devices are connected)
- Anonymized node IDs (SHA-256 truncated to 8 bytes, rotated daily)

**Node ID anonymization:**
```
anon_id = base64(SHA-256(node_public_key || date_UTC_day)[0:8])
```
This rotates daily, preventing long-term tracking while enabling per-day correlation.

---

## 8. Integration with External Systems

### 8.1 Prometheus Integration

Gateway nodes expose `/v1/metrics` in Prometheus text format. Standard Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: 'iris_gateway'
    static_configs:
      - targets: ['gateway-ip:9090']
    tls_config:
      cert_file: operator.crt
      key_file: operator.key
```

### 8.2 Grafana Dashboards

Pre-built Grafana dashboard definitions are maintained at `engineering/dashboards/`. Deploy via Grafana provisioning API.

### 8.3 Alertmanager Integration

Alerts route to Alertmanager via standard Prometheus alerting rules. Alert definitions in `engineering/alerts/iris_alerts.yaml`.

---

## 9. Observability Under Failure

When the mesh is partitioned or severely degraded, observability degrades gracefully:

- **No internet**: Metrics accumulate in local ring buffer (last 24 hours). Export when connectivity restores.
- **Gateway down**: Mobile nodes self-assess using local metrics only. Health score computed locally.
- **Battery critical**: Metrics collection interval increases (60s → 300s) to reduce overhead.
- **Storage critical**: Oldest metrics dropped first. Emergency metrics retained at all times.

The observability system must never impact emergency message delivery. If the IRIS process is under memory or CPU pressure, metrics collection is the first thing throttled.
