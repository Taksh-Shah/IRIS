# Benchmarking — IRIS Performance Benchmark Suite

**Component:** `iris-bench` (Rust), `tests/bench/` (Python)
**Status:** Design v1.0 — BENCH-001 through BENCH-008 defined
**Last Updated:** 2026-08-11

---

## 1. Overview

The IRIS benchmark suite measures performance of critical operations against defined targets. Benchmarks run in CI on every PR that touches relevant code. A benchmark regression (result worse than target) blocks the merge.

All benchmarks are deterministic and hardware-normalized. Results are reported relative to the reference hardware to enable comparison across machines.

### Reference Hardware

| Platform | Description | Identifier |
|---|---|---|
| Android mid-range | Snapdragon 680, 4 GB RAM | `android-sd680` |
| Raspberry Pi Zero 2W | Cortex-A53 @1 GHz, 512 MB RAM | `rpi-zero2w` |
| Raspberry Pi 4 | Cortex-A72 @1.5 GHz, 8 GB RAM | `rpi4-8gb` |
| x86 CI runner | Intel Xeon E5 @2.4 GHz | `ci-x86` |

Benchmarks define targets for the most constrained relevant platform.

---

## 2. Benchmark Definitions

### BENCH-001: Message Encode/Decode Throughput

**What it measures:** Throughput of protobuf/MessagePack encode and decode for the standard IRIS message format.

**Rationale:** Message serialization happens on every relay and store operation. On a busy relay node, this can be called thousands of times per second.

**Target:** > 10,000 messages/second on `android-sd680`

**Methodology:**

```rust
// Cargo bench (criterion.rs)
fn bench_message_encode(c: &mut Criterion) {
    let msg = Message::test_message_p3_1kb();
    c.bench_function("message_encode_1kb", |b| {
        b.iter(|| {
            let encoded = msg.encode_to_vec();
            black_box(encoded);
        })
    });
}

fn bench_message_decode(c: &mut Criterion) {
    let msg = Message::test_message_p3_1kb();
    let encoded = msg.encode_to_vec();
    c.bench_function("message_decode_1kb", |b| {
        b.iter(|| {
            let decoded = Message::decode(black_box(encoded.as_slice())).unwrap();
            black_box(decoded);
        })
    });
}
```

**Baseline (measured, `ci-x86`):** 85,000 msg/s encode, 78,000 msg/s decode
**Target (`android-sd680`):** > 10,000 msg/s (encode + decode combined)

---

### BENCH-002: Ed25519 Sign/Verify Throughput

**What it measures:** Throughput of Ed25519 digital signature operations (sign and verify) using the `ed25519-dalek` crate.

**Rationale:** Every message is signed by its originator. Edge server relays verify signatures on relay. On constrained hardware, this must not become a bottleneck.

**Target:** > 1,000 sign+verify/second on `rpi-zero2w`

**Methodology:**

```rust
fn bench_ed25519(c: &mut Criterion) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let message = b"test message payload for benchmarking";

    c.bench_function("ed25519_sign", |b| {
        b.iter(|| signing_key.sign(black_box(message)))
    });

    let signature = signing_key.sign(message);
    c.bench_function("ed25519_verify", |b| {
        b.iter(|| verifying_key.verify(black_box(message), &signature).unwrap())
    });
}
```

**Baseline (`ci-x86`):** ~18,000 sign/s, ~7,000 verify/s
**Target (`rpi-zero2w`):** > 1,000 sign+verify pairs/second

---

### BENCH-003: BLAKE3 Hash Throughput

**What it measures:** Throughput of BLAKE3 hashing for message integrity verification and Bloom filter hashing.

**Target:** > 500 MB/s on `android-sd680`

**Methodology:**

```rust
fn bench_blake3(c: &mut Criterion) {
    let data = vec![0u8; 1024 * 1024]; // 1 MB
    c.bench_function("blake3_1mb", |b| {
        b.iter(|| blake3::hash(black_box(&data)))
    });
}
```

**Baseline (`ci-x86`):** ~4,200 MB/s (BLAKE3 is highly optimized for x86 AVX-512)
**Target (`android-sd680`):** > 500 MB/s (ARM NEON acceleration in blake3 crate)

---

### BENCH-004: SQLite Write Throughput for Message Store

**What it measures:** Sustained write throughput to the IRIS SQLite message store under realistic conditions (write + index update + WAL commit).

**Target:** > 1,000 writes/second on `rpi4-8gb`, > 100 writes/second on `rpi-zero2w`

**Methodology:**

```python
import sqlite3
import time

def bench_sqlite_writes(n_writes: int = 10000, batch_size: int = 100) -> float:
    conn = sqlite3.connect(":memory:")
    conn.execute("""CREATE TABLE messages (
        id TEXT PRIMARY KEY,
        priority INTEGER,
        payload BLOB,
        created_at INTEGER,
        ttl_s INTEGER
    )""")
    conn.execute("CREATE INDEX idx_priority ON messages(priority)")

    start = time.perf_counter()
    for batch_start in range(0, n_writes, batch_size):
        with conn:
            for i in range(batch_start, min(batch_start + batch_size, n_writes)):
                conn.execute(
                    "INSERT INTO messages VALUES (?, ?, ?, ?, ?)",
                    (f"msg-{i}", 4, b"x" * 1024, int(time.time()), 21600)
                )
    elapsed = time.perf_counter() - start
    return n_writes / elapsed
```

**Baseline (`ci-x86`):** ~12,000 writes/s (WAL mode, batch 100)
**Target:** > 1,000/s on RPi 4, > 100/s on RPi Zero 2W

---

### BENCH-005: BLE Discovery Time

**What it measures:** Time from enabling BLE scanning to discovering the first peer device advertising IRIS services.

**Target:** < 3 seconds (median), < 10 seconds (P95)

**Methodology:** Hardware test only (cannot be simulated). IRIS app running on two Android devices, 2m apart. 100 trials. Measure time from scan start to `onScanResult` callback with IRIS UUID.

**Notes:**
- Android BLE scan batch interval (default 5s) significantly affects this metric
- IRIS uses `SCAN_MODE_LOW_LATENCY` for the first 30s of a discovery session
- iOS CoreBluetooth adds additional delay due to peripheral advertisement scanning policy

**Baseline (measured):** Median 1.2s, P95 4.8s (Android 13, Snapdragon 680)
**Target:** Median < 3s, P95 < 10s

---

### BENCH-006: Wi-Fi Direct Connection Time

**What it measures:** Time from Wi-Fi Direct group formation request to successful P2P connection.

**Target:** < 5 seconds (median)

**Methodology:** Hardware test. Android P2P API: `requestConnectionInfo` timing from `WifiP2pManager.connect()` to `onConnectionInfoAvailable()`.

**Known variable:** Wi-Fi Direct connection time is highly platform-dependent and varies with Android version and chipset. Samsung devices tend to be slower (~8s) than Pixel devices (~2s).

**Baseline (measured):** Median 2.8s (Pixel 7), 7.4s (Samsung A53)
**Target:** Median < 5s on Snapdragon 680 class hardware

---

### BENCH-007: PRoPHET Routing Decision Time

**What it measures:** Time for the routing engine to make a relay/reject decision for a single candidate peer using PRoPHET delivery probabilities.

**Target:** < 1 ms (P99) on `rpi-zero2w`

**Methodology:**

```rust
fn bench_prophet_routing(c: &mut Criterion) {
    let routing_table = RoutingTable::with_n_entries(1000); // realistic size
    let candidate = PeerId::random();
    let message = Message::test_message_p3_1kb();

    c.bench_function("prophet_routing_decision", |b| {
        b.iter(|| {
            routing_table.should_relay(
                black_box(&candidate),
                black_box(&message),
                black_box(0.5), // prophet_dp threshold
            )
        })
    });
}
```

**Baseline (`ci-x86`):** 2.3 μs per decision (1000-entry routing table)
**Target (`rpi-zero2w`):** < 1 ms (P99)

---

### BENCH-008: Bloom Filter Insert/Query

**What it measures:** Throughput of Bloom filter insert and membership query operations.

**Target:** < 1 μs per insert, < 1 μs per query

**Methodology:**

```rust
fn bench_bloom(c: &mut Criterion) {
    let mut filter = BloomFilter::new(65536, 10); // 8 KB, 10 hash functions
    let ids: Vec<MessageId> = (0..10000).map(|_| MessageId::random()).collect();

    c.bench_function("bloom_insert", |b| {
        b.iter(|| filter.insert(black_box(&ids[0])))
    });

    filter.insert(&ids[0]);
    c.bench_function("bloom_contains", |b| {
        b.iter(|| filter.contains(black_box(&ids[0])))
    });
}
```

**Baseline (`ci-x86`):** ~48 ns insert, ~31 ns query
**Target (all platforms):** < 1 μs insert, < 1 μs query

---

## 3. Benchmark Registry

| ID | Name | Target platform | Target | CI gate? |
|---|---|---|---|---|
| BENCH-001 | Message encode/decode | `android-sd680` | > 10,000 msg/s | Yes |
| BENCH-002 | Ed25519 sign/verify | `rpi-zero2w` | > 1,000/s | Yes |
| BENCH-003 | BLAKE3 hash | `android-sd680` | > 500 MB/s | Yes |
| BENCH-004 | SQLite write | `rpi-zero2w` | > 100/s | Yes |
| BENCH-005 | BLE discovery | Hardware only | Median < 3s | No |
| BENCH-006 | Wi-Fi Direct connect | Hardware only | Median < 5s | No |
| BENCH-007 | PRoPHET routing | `rpi-zero2w` | P99 < 1 ms | Yes |
| BENCH-008 | Bloom filter | All | < 1 μs | Yes |
| BENCH-009 | ML inference | `rpi-zero2w` | < 10 ms | Yes |
| BENCH-010 | RL Q-table lookup | `rpi-zero2w` | < 0.1 ms | Yes |

---

## 4. Running Benchmarks

```bash
# Run all Rust benchmarks (Criterion)
cargo bench --workspace

# Run specific benchmark
cargo bench --bench message -- bench_message_encode

# Run Python benchmarks
python -m pytest tests/bench/ -v --benchmark-only

# Generate benchmark report
cargo bench --workspace -- --output-format bencher | tee bench_results.txt

# Compare against baseline
cargo bench --workspace -- --baseline main
```

---

## 5. CI Integration

Benchmark results are compared against the stored baseline on every PR. A regression of > 10% triggers a CI failure and blocks merge. The baseline is updated manually after deliberate performance changes.

```yaml
# .github/workflows/bench.yml (excerpt)
- name: Run benchmarks
  run: cargo bench --workspace -- --output-format bencher | tee output.txt

- name: Check for regression
  uses: benchmark-action/github-action-benchmark@v1
  with:
    tool: 'cargo'
    output-file-path: output.txt
    alert-threshold: '110%'   # 10% regression triggers alert
    fail-on-alert: true
```

---

## 6. References

- Criterion.rs: https://github.com/bheisler/criterion.rs
- ed25519-dalek: https://docs.rs/ed25519-dalek/
- BLAKE3: https://github.com/BLAKE3-team/BLAKE3
- Performance budgets: `docs/performance/PERFORMANCE_BUDGETS.md`
