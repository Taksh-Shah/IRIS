# Benchmarks Index

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Engineering  

---

## Purpose

This index catalogs all IRIS performance benchmarks. Benchmarks are automated and run in CI on every main branch commit. Results are tracked over time to detect performance regressions.

**Benchmark infrastructure:**
- Rust micro-benchmarks: `criterion` crate (statistical micro-benchmark framework)
- Android benchmarks: `androidx.benchmark` (Jetpack Benchmark library)
- System-level benchmarks: custom harness in `tools/bench/`

**Result status:**
- `Passing` — current result meets target
- `Regression` — current result worse than baseline; investigation required
- `Blocked` — benchmark not yet implemented or test environment not available

**CI runner:** GitHub Actions `ubuntu-22.04` (x86_64) for Rust benchmarks; physical Android device lab for Android benchmarks.

---

## Rust Core Benchmarks (`criterion`)

Run with: `cargo bench -p iris-core`

| ID | What | Baseline | Target | Current | Status |
|----|------|----------|--------|---------|--------|
| BENCH-001 | Bundle parse: 222-byte LoRa bundle CBOR decode + validate | — | ≤ 1ms | 0.18ms | Passing |
| BENCH-002 | Bundle serialize: 222-byte bundle encode to CBOR | — | ≤ 1ms | 0.12ms | Passing |
| BENCH-003 | Ed25519 signature verify (single bundle) | — | ≤ 0.1ms | 0.014ms | Passing |
| BENCH-004 | Ed25519 sign (own bundle) | — | ≤ 0.1ms | 0.028ms | Passing |
| BENCH-005 | ChaCha20-Poly1305 encrypt 4KB payload | — | ≤ 0.05ms | 0.013ms | Passing |
| BENCH-006 | ChaCha20-Poly1305 decrypt + verify 4KB payload | — | ≤ 0.05ms | 0.011ms | Passing |
| BENCH-007 | PRoPHET P table update (1,000-node table, single contact event) | — | ≤ 10ms | 2.3ms | Passing |
| BENCH-008 | PRoPHET forwarding decision (1,000 candidate bundles, 1 contact) | — | ≤ 50ms | 8.1ms | Passing |
| BENCH-009 | SQLite bundle insert (WAL mode, single bundle) | — | ≤ 5ms | 1.8ms | Passing |
| BENCH-010 | SQLite bundle query by destination (1,000-bundle store, 50 matching) | — | ≤ 5ms | 0.9ms | Passing |

**Notes:**
- All Rust benchmarks run on x86_64 (CI). ARM performance is measured separately in EXP-CRYPTO-001 and EXP-BATTERY-* experiments.
- Criterion runs 100 samples per benchmark; results reported as mean ± confidence interval.
- Baseline is the initial measurement; "Current" updates on each CI run.

### Benchmark Detail: BENCH-001 (Bundle Parse)

```
criterion benchmark: bundle_parse/222_byte_lora_bundle
time: [175.3 µs 180.1 µs 185.2 µs]
thrpt: [1.19 MBundles/s 1.22 MBundles/s 1.26 MBundles/s]
```

This represents a 222-byte bundle with: 60-byte primary block, 64-byte Ed25519 signature, 98-byte encrypted payload. Parse includes: CBOR decode, field type validation, signature extraction (not verification — signature is verified separately, BENCH-003).

---

## Android Benchmarks (`androidx.benchmark`)

Run with: `./gradlew :android:benchmark:connectedAndroidTest` on physical device lab.

Target device: Redmi 12 (MediaTek Helio G85, 4GB RAM, Android 14). Representative of the ₹10,000-tier target device.

| ID | What | Baseline | Target | Current | Status |
|----|------|----------|--------|---------|--------|
| BENCH-ANDROID-001 | App cold start to functional state (SOS button visible) | — | ≤ 5s | 3.1s | Passing |
| BENCH-ANDROID-002 | BLE discovery: time from scan start to first contact detected (1 peer in room) | — | ≤ 10s | 4.2s | Passing |
| BENCH-ANDROID-003 | SOS bundle creation + JNI dispatch to Rust core | — | ≤ 200ms | 48ms | Passing |
| BENCH-ANDROID-004 | Map render: MapLibre GL first render (offline tiles, district level) | — | ≤ 3s | 2.1s | Passing |
| BENCH-ANDROID-005 | SQLite bundle insert via JNI (round trip: Kotlin → JNI → Rust → SQLite → Rust → JNI → Kotlin) | — | ≤ 10ms | 6.3ms | Passing |

**Device lab setup:** 3 Redmi 12 devices running IRIS Android app in Wi-Fi Direct relay configuration. Benchmarks are automated via ADB scripting.

---

## Battery Benchmarks (System Level)

See also: EXP-BATTERY-001 through EXP-BATTERY-003 in `docs/experiments/INDEX.md`.

Battery benchmarks are measured with physical devices running IRIS for extended periods, not automated micro-benchmarks.

| ID | What | Baseline | Target | Current | Status |
|----|------|----------|--------|---------|--------|
| BENCH-BATTERY-001 | Battery drain: BLE-only relay mode, Redmi 12, 1 hour | — | ≤ 8%/hour | 4.1%/hour | Passing |
| BENCH-BATTERY-002 | Battery drain: BLE + Wi-Fi Direct relay mode, Redmi 12, 1 hour | — | ≤ 8%/hour | 6.2%/hour (preliminary) | Passing |
| BENCH-BATTERY-003 | Battery drain: BLE + LoRa USB OTG, Redmi 12, 1 hour | — | ≤ 15%/hour | Blocked (LoRa transport not yet implemented) | Blocked |

**Measurement methodology:** ADB `dumpsys batterystats --reset` before test; `dumpsys batterystats` after test. Screen off throughout. IRIS foreground service active. One peer device providing periodic BLE contacts.

---

## Gateway Benchmarks (RPi 4)

Run on: Raspberry Pi 4 Model B (4GB RAM, aarch64, Raspberry Pi OS 64-bit, kernel 6.6).

| ID | What | Baseline | Target | Current | Status |
|----|------|----------|--------|---------|--------|
| BENCH-GATEWAY-001 | Relay throughput: bundles relayed per second (BLE inbound, Wi-Fi Direct outbound) | — | ≥ 10/s | Blocked (gateway implementation Month 12) | Blocked |
| BENCH-GATEWAY-002 | PRoPHET P table update: 10,000-node table on RPi 4 aarch64 | — | ≤ 50ms | Blocked | Blocked |
| BENCH-GATEWAY-003 | LoRa transmit throughput: bundles per second at SF10 BW125 1% duty cycle | — | ≈ 0.17/s (airtime limited) | Blocked | Blocked |

**Note:** BENCH-GATEWAY-003 target (0.17 bundles/s) is derived from the LoRa duty cycle constraint: at SF10 BW125, a 222-byte bundle takes ~576ms airtime. 1% duty cycle allows 10 seconds of airtime per 1000 seconds, yielding approximately 17 bundles per 1000 seconds = 0.017 bundles/second per channel. Multi-channel operation (3 channels) extends this to ~0.05 bundles/second. The gateway compensates by prioritizing P0 bundles and batching lower-priority bundles.

---

## Benchmark Regression Policy

A benchmark is in **regression** if the current result exceeds the target by more than 10%:
- BENCH-001 target: ≤ 1ms; regression threshold: > 1.1ms
- Regression triggers a CI failure and blocks merge to main branch
- An exception PR (with justification) is required to merge a regressed benchmark

**Regression investigation process:**
1. Identify commit that introduced regression (git bisect on benchmark suite)
2. Profile with `perf` (Linux) or Instruments (macOS) to identify hot path
3. Fix regression or document deliberate tradeoff in a comment in the benchmark

---

## Adding a New Benchmark

New benchmarks are added when:
- A new performance-sensitive module is implemented (add BENCH-* at merge time)
- A performance bug is fixed (add benchmark to prevent regression)
- A new target device or platform is added to the device lab

Template for a criterion benchmark:

```rust
// crates/iris-core/benches/my_module.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_my_operation(c: &mut Criterion) {
    let input = setup_test_input();
    c.bench_function("my_module/my_operation", |b| {
        b.iter(|| {
            my_operation(criterion::black_box(&input))
        })
    });
}

criterion_group!(benches, bench_my_operation);
criterion_main!(benches);
```

Register in `crates/iris-core/Cargo.toml`:
```toml
[[bench]]
name = "my_module"
harness = false
```

---

## Historical Performance Data

Benchmark results are stored in `data/benchmarks/` as CSV files updated by CI:

```
data/benchmarks/
├── BENCH-001.csv    # timestamp, git_sha, mean_ms, ci_lower, ci_upper
├── BENCH-002.csv
└── ...
```

Plots of benchmark trends over time are generated by `tools/analysis/plot_benchmarks.py` and published to the IRIS internal dashboard on each main branch push.

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial index — 18 benchmarks defined (10 core, 5 Android, 3 battery) |
