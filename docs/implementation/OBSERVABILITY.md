# IRIS Observability Implementation

## Structured Logging

IRIS uses the `tracing` crate for structured logging throughout the Rust core. `tracing` provides both synchronous and async-aware spans with key-value fields.

### Log Levels

| Level | Usage | Example |
|-------|-------|---------|
| `ERROR` | Crash-worthy events, data corruption risk | Failed signature verification, storage write failure |
| `WARN` | Degraded operation, recoverable issues | Transport unavailable, message near expiry |
| `INFO` | Major lifecycle events | Neighbor connected, message delivered, SOS sent |
| `DEBUG` | Detailed operational flow | Routing decision, hop relay, cache hit/miss |
| `TRACE` | Per-packet level detail | CBOR bytes parsed, BLE characteristic write |

In production builds: `INFO` and above. In development: `DEBUG`. Never enable `TRACE` in production (performance + privacy).

### Privacy Rules for Logging

The following data must **never** appear in logs:

- Message payload content (encrypted or decrypted)
- Private keys or key derivation material
- Complete contact lists or address books
- Full NodeId of users (use 8-byte prefix for correlation only)
- Any personally identifiable content

```rust
// BAD: logs full content
tracing::info!("Received message: {:?}", message.payload);

// GOOD: logs only metadata
tracing::info!(
    message_id = %message.id.short(),  // first 8 bytes only
    sender = %message.sender.short(),
    priority = message.priority as u8,
    payload_size = message.payload.len(),
    "Message received"
);
```

### Tracing Setup

```rust
use tracing_subscriber::{fmt, EnvFilter, prelude::*};

pub fn init_tracing(json_output: bool) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("iris_core=info,iris_transport=info"));

    if json_output {
        // Production: JSON for log aggregation
        tracing_subscriber::registry()
            .with(fmt::layer().json().with_filter(env_filter))
            .init();
    } else {
        // Development: human-readable
        tracing_subscriber::registry()
            .with(fmt::layer().pretty().with_filter(env_filter))
            .init();
    }
}
```

### Span-based Tracing

Every message gets a span covering its full lifecycle through the system:

```rust
pub async fn process_incoming(&self, envelope: Envelope) -> Result<(), IrisError> {
    let span = tracing::info_span!(
        "message_incoming",
        message_id = %envelope.id.short(),
        sender = %envelope.sender.short(),
        priority = envelope.priority as u8,
        hop_count = envelope.hop_count,
    );

    async move {
        tracing::info!("Processing incoming message");

        // Deduplication
        if self.dedup.is_seen(&envelope.id).await? {
            tracing::debug!("Duplicate message, dropping");
            return Ok(());
        }

        // Storage
        self.storage.persist_message(&envelope).await
            .map_err(|e| { tracing::error!(error = %e, "Storage write failed"); e })?;
        tracing::debug!("Message persisted");

        // Relay decision
        if let Some(next_hops) = self.routing.compute_relay(&envelope).await? {
            tracing::info!(hop_count = next_hops.len(), "Relaying message");
            self.transport.forward(&envelope, &next_hops).await?;
        }

        tracing::info!("Message processing complete");
        Ok(())
    }.instrument(span).await
}
```

## Metrics

IRIS uses the `metrics` crate with a pluggable backend. In production, metrics are exported as Prometheus format or stored locally in SQLite.

### Metric Definitions

```rust
use metrics::{gauge, counter, histogram, describe_gauge, describe_counter, describe_histogram, Unit};

pub fn register_metrics() {
    describe_gauge!(
        "iris_delivery_ratio",
        Unit::Dimensionless,
        "Ratio of delivered messages to total sent (rolling 1 hour)"
    );
    describe_gauge!(
        "iris_messages_in_queue",
        Unit::Count,
        "Number of messages currently in the send queue by priority"
    );
    describe_gauge!(
        "iris_transport_active",
        Unit::Count,
        "Whether each transport is currently active (1=active, 0=inactive)"
    );
    describe_counter!(
        "iris_routing_decisions_total",
        Unit::Count,
        "Total number of routing decisions made"
    );
    describe_counter!(
        "iris_ack_timeout_total",
        Unit::Count,
        "Number of messages that timed out waiting for acknowledgement"
    );
    describe_gauge!(
        "iris_storage_usage_bytes",
        Unit::Bytes,
        "Current size of the message store in bytes"
    );
    describe_gauge!(
        "iris_neighbor_count",
        Unit::Count,
        "Number of currently active neighbors"
    );
    describe_histogram!(
        "iris_message_delivery_latency_ms",
        Unit::Milliseconds,
        "End-to-end message delivery latency when ack is received"
    );
    describe_gauge!(
        "iris_routing_table_size",
        Unit::Count,
        "Number of nodes in the routing probability table"
    );
}

// Recording metrics
pub fn record_message_delivered(priority: u8, latency_ms: f64) {
    counter!("iris_messages_delivered_total", "priority" => priority.to_string()).increment(1);
    histogram!("iris_message_delivery_latency_ms", "priority" => priority.to_string())
        .record(latency_ms);
}

pub fn record_queue_depth(priority: u8, depth: f64) {
    gauge!("iris_messages_in_queue", "priority" => priority.to_string()).set(depth);
}

pub fn record_transport_status(transport: &str, active: bool) {
    gauge!("iris_transport_active", "transport" => transport.to_string())
        .set(if active { 1.0 } else { 0.0 });
}
```

### Prometheus Export

When debug endpoint is enabled (configurable), IRIS exposes Prometheus metrics on `127.0.0.1:9090/metrics`:

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

pub fn start_metrics_server(port: u16) -> anyhow::Result<()> {
    PrometheusBuilder::new()
        .with_http_listener(([127, 0, 0, 1], port))
        .install()?;
    tracing::info!(port, "Prometheus metrics endpoint started");
    Ok(())
}
```

On mobile, the Prometheus endpoint is disabled by default (no server on mobile). Metrics are stored in the SQLite `metrics` table for offline collection and later upload when Internet is available.

### Offline Metrics Storage

```rust
pub async fn flush_metrics_to_db(&self) -> Result<(), StorageError> {
    let snapshot = self.metrics_registry.snapshot();
    let now = unix_now_ms();
    let conn = self.conn.lock().await;

    for (key, value) in snapshot.gauges() {
        conn.execute(
            "INSERT INTO metrics (metric_name, value, labels, recorded_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key.name(), value, key.labels_json(), now]
        )?;
    }
    // Prune metrics older than 7 days
    conn.execute(
        "DELETE FROM metrics WHERE recorded_at < ?1",
        [now - 7 * 24 * 60 * 60 * 1000]
    )?;
    Ok(())
}
```

## Diagnostic CLI (`iris-ctl`)

The `iris-ctl` binary connects to a running `iris-relay` daemon via gRPC/Unix socket and displays operational information:

```bash
# Network health summary
$ iris-ctl status
IRIS Relay Status
-----------------
Node ID:        a3f2c8d1... (truncated)
Uptime:         4h 23m
Transport:      BLE (active), TCP (active), LoRa (inactive)
Neighbors:      3 active
Queue:          12 messages pending (P0: 0, P1: 2, P2: 1, P3-7: 9)
Storage:        142 MB / 500 MB (28%)
Delivery ratio: 94.2% (last 1 hour)

# Neighbor list
$ iris-ctl neighbors
NEIGHBORS (3)
-------------
NodeId          Transport   RSSI    Last Seen   Messages
b4e1a9...       BLE         -67dBm  2s ago      47 sent, 12 received
f2c3d8...       BLE         -81dBm  15s ago     8 sent, 3 received
192.168.1.42    TCP         N/A     1s ago      127 sent, 89 received

# Message queue
$ iris-ctl queue
QUEUE (12 messages)
-------------------
P0   0 messages
P1   2 messages  [expires in 55m, 58m]
P2   1 message   [expires in 2h]
P3   4 messages  [oldest: 3h ago]
P4-7 5 messages

# Statistics
$ iris-ctl stats
DELIVERY STATISTICS (last 24h)
--------------------------------
Total sent:       234
Delivered:        211 (90.2%)
ACK timeout:      18
Expired:          5
Direct delivery:  178
1-hop relay:      24
2-hop relay:      9
3+ hop relay:     0
Avg latency:      2.3s
P99 latency:      18.4s
```

## Health Checks

IRIS relay exports a health check endpoint:

```
GET http://127.0.0.1:7788/health
```

Returns:
```json
{
  "status": "healthy",
  "uptime_seconds": 15723,
  "transport_ble": "active",
  "transport_tcp": "active",
  "storage_usage_pct": 28,
  "queue_depth": 12,
  "neighbor_count": 3
}
```

Used by monitoring systems and the zero-downtime upgrade mechanism.
