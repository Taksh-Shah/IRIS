# Logging — Structured Logging Design for IRIS

**Component:** All IRIS components
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

IRIS logging serves two purposes: operational diagnostics (what is the system doing?) and post-incident analysis (what went wrong?). The design must balance completeness against privacy (no message content in logs) and storage (circular buffer on mobile devices).

All IRIS components use structured JSON-lines logging. Each log entry is a self-contained JSON object on a single line, making logs parseable by `jq`, `grep`, and log aggregation tools without a schema.

---

## 2. Log Levels

| Level | Numeric | Use cases | Written to disk? |
|---|---|---|---|
| `ERROR` | 50 | Unrecoverable failures, crypto errors, storage full | Always |
| `WARN` | 40 | Degraded operation, fallback activated, rate limits | Always |
| `INFO` | 30 | Normal operation events (transport connect, message relayed) | Yes (circular) |
| `DEBUG` | 20 | Detailed routing decisions, contact events | Yes (circular, pruned first) |
| `TRACE` | 10 | Per-packet events, cryptographic operations | Memory only (not persisted) |

**Default level in production:** `INFO`. `DEBUG` enabled by user in settings or by coordinator command. `TRACE` only for on-device debugging sessions.

---

## 3. Log Entry Format

Every log entry is a single-line JSON object:

```json
{
  "ts": "2026-08-11T14:32:15.003Z",
  "lvl": "INFO",
  "nid": "sha256:a3f7",
  "comp": "routing",
  "evt": "message_relayed",
  "data": {
    "msg_id_hash": "sha256:b7c2",
    "priority": "P2",
    "relay_to_hash": "sha256:c9d1",
    "transport": "ble",
    "prophet_dp": 0.67,
    "queue_depth": 45
  }
}
```

### 3.1 Standard Fields

| Field | Type | Description |
|---|---|---|
| `ts` | ISO 8601 string | UTC timestamp with millisecond precision |
| `lvl` | string | Log level: ERROR, WARN, INFO, DEBUG, TRACE |
| `nid` | string | Pseudonymized node ID (first 8 chars of SHA256) |
| `comp` | string | Component name (see Section 5) |
| `evt` | string | Event type (snake_case) |
| `data` | object | Event-specific structured data |

### 3.2 Privacy Rules for Log Fields

| Data type | Policy |
|---|---|
| Message content | **Never logged.** Log the message ID hash only. |
| Raw NodeId (public key) | **Never in user-visible logs.** Use pseudonymized `nid`. |
| GPS coordinates | Rounded to 3 decimal places (~100m precision) in logs |
| Contact history | Node ID hashed before logging |
| Battery level | Logged as percentage (not raw ADC value) |
| User settings | Logged as key name only, not value |

The pseudonymization key rotates every 24 hours. Logs from the previous 24 hours can be cross-referenced; older logs have independent pseudonyms.

---

## 4. Component Names

| `comp` value | Description |
|---|---|
| `transport.ble` | BLE 5.x transport |
| `transport.wifi` | Wi-Fi Direct transport |
| `transport.lora` | LoRa transport |
| `transport.satellite` | Satellite transport |
| `transport.cellular` | Cellular transport |
| `routing` | Message routing engine (PRoPHET, Spray-and-Wait) |
| `routing.ml` | ML routing model decisions |
| `queue` | Message priority queue |
| `store` | SQLite message store |
| `crypto` | Cryptographic operations |
| `graph` | Network graph analytics |
| `llm` | LLM service (edge server only) |
| `sync` | Peer synchronization |
| `discovery` | Peer discovery |

---

## 5. Standard Events

### 5.1 Transport Events

```json
{"evt": "transport_connected", "data": {"transport": "ble", "peer_hash": "sha256:...", "rssi_dbm": -72}}
{"evt": "transport_disconnected", "data": {"transport": "ble", "peer_hash": "sha256:...", "duration_s": 45.2}}
{"evt": "transport_error", "data": {"transport": "lora", "error": "duty_cycle_exceeded", "retry_after_s": 36}}
```

### 5.2 Routing Events

```json
{"evt": "message_accepted_relay", "data": {"msg_id_hash": "sha256:...", "priority": "P2", "relay_to_hash": "sha256:...", "reason": "prophet_dp_high"}}
{"evt": "message_rejected_relay", "data": {"msg_id_hash": "sha256:...", "priority": "P4", "reason": "queue_full"}}
{"evt": "message_delivered", "data": {"msg_id_hash": "sha256:...", "priority": "P0", "hop_count": 2, "latency_s": 87.3}}
{"evt": "message_expired", "data": {"msg_id_hash": "sha256:...", "priority": "P3", "ttl_hours": 12}}
```

### 5.3 System Events

```json
{"evt": "startup", "data": {"version": "1.2.0", "platform": "android", "node_type": "mobile"}}
{"evt": "ml_routing_disabled", "data": {"reason": "battery_low", "battery_pct": 14}}
{"evt": "bloom_filter_rotated", "data": {"element_count": 1842, "fpr_estimate": 0.0009}}
{"evt": "partition_risk_detected", "data": {"bridge_node_hash": "sha256:...", "contact_rate_drop_pct": 78}}
```

---

## 6. Storage

### 6.1 Mobile Devices (Android/iOS)

| Parameter | Value |
|---|---|
| Storage medium | Internal storage, encrypted at rest |
| Maximum log size | 50 MB (circular buffer) |
| Circular buffer eviction | Oldest entries by timestamp |
| Level eviction priority | TRACE → DEBUG → INFO (higher levels kept longer) |
| Storage path (Android) | `{filesDir}/iris/logs/iris.jsonl` |
| Storage path (iOS) | `{applicationSupportDirectory}/iris/logs/iris.jsonl` |

The circular buffer is implemented as a fixed-size append-only file with a rolling write pointer. When the file reaches 50 MB, the oldest 10 MB is discarded (not the entire file).

### 6.2 Edge Servers

| Parameter | Value |
|---|---|
| Storage medium | Local disk |
| Maximum log size | Unlimited (bounded by disk) |
| Log rotation | Daily rotation, gzip compression |
| Retention | 90 days compressed, then purged |
| Storage path | `/var/log/iris/iris-YYYY-MM-DD.jsonl` |

### 6.3 Log Indexing

Edge server logs are indexed by `ts`, `lvl`, `comp`, `evt` using a lightweight inverted index for fast query support. Field `msg_id_hash` is indexed for message lifecycle tracing.

---

## 7. Log Export

### 7.1 Export to Gateway

When a mobile device connects to an edge server or gateway, it may (with user consent) export its local log buffer. Export is compressed with zstd:

```
POST /api/v1/logs/upload
Content-Type: application/x-ndjson+zstd
X-Node-Id-Hash: sha256:a3f7
X-Log-Period: 2026-08-10T00:00:00Z/2026-08-11T00:00:00Z
```

Log export is **opt-in** for mobile users. Enabled in Settings → Privacy → Share diagnostic logs.

### 7.2 Coordinator Log Query

Coordinators can query edge server logs via REST:

```
GET /api/v1/logs?comp=routing&evt=message_delivered&priority=P0&start=2026-08-11T12:00:00Z&end=2026-08-11T18:00:00Z
```

Response: newline-delimited JSON entries matching the filter.

---

## 8. Security Constraints

- Logs are encrypted at rest using the same key as the message store (AES-256-GCM)
- Log files cannot be read without the device's secure enclave key
- Log export requires the user's explicit authentication (biometric/PIN)
- Coordinator log access is limited to their managed nodes' logs
- Audit logs (ML routing decisions) are tamper-evident (chained hashes) — see `docs/intelligence/AI_SAFETY.md`

---

## 9. Implementation (Rust)

```rust
use tracing::{info, warn, error, debug};
use tracing_subscriber::fmt::format::JsonFields;

// Log a routing decision
info!(
    component = "routing",
    event = "message_accepted_relay",
    msg_id_hash = %msg_id.short_hash(),
    priority = %msg.priority,
    relay_to_hash = %peer_id.short_hash(),
    prophet_dp = prophet_dp,
    queue_depth = queue.len(),
    "Message accepted for relay"
);
```

The `short_hash()` method returns `sha256:` followed by the first 8 hex characters of the SHA256 hash of the original value.

---

## 10. References

- Structured logging: https://www.honeycomb.io/blog/structured-logging
- tracing crate (Rust): https://docs.rs/tracing/
- IRIS privacy policy: `docs/security/PRIVACY.md`
- AI audit trail: `docs/intelligence/AI_SAFETY.md`
