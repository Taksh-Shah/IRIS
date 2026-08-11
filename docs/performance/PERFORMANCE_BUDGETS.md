# Performance Budgets — Hard and Soft Resource Limits

**Component:** All IRIS components
**Status:** Policy v1.0 — Budgets are enforced by CI benchmarks
**Last Updated:** 2026-08-11

---

## 1. Overview

Performance budgets define resource limits that IRIS must not exceed. They exist to ensure:

1. IRIS does not degrade the user's device in normal disaster scenarios
2. IRIS remains operational on the most constrained supported hardware
3. Performance regressions are caught before they reach users

**Hard budgets** are enforced automatically: CI benchmark jobs fail if a hard budget is exceeded, blocking the merge.

**Soft budgets** are targets that the team aims for but that do not auto-block merges. They are reviewed in the quarterly performance review.

---

## 2. Hard Budgets (Must Not Exceed)

### 2.1 Memory (RAM)

| Platform | Component | Budget | Measured (current) |
|---|---|---|---|
| Android (any) | IRIS app process RSS | 100 MB | ~72 MB |
| iOS (any) | IRIS app process RSS | 80 MB | ~68 MB |
| Raspberry Pi Zero 2W | iris-core process | 50 MB | ~31 MB |
| Raspberry Pi 4 (8 GB) | iris-edge process | 500 MB | ~220 MB |
| x86 edge server | iris-edge process | 2 GB | ~180 MB |

**Measurement method:** `proc/self/status` VmRSS on Linux, `os.proc_info` on macOS/iOS, `ActivityManager.getProcessMemoryInfo()` on Android.

**Enforcement:** CI runs a memory profiling job on the Pi Zero 2W reference image. If `VmRSS > 50 MB` during any benchmark scenario, the CI job fails.

```rust
// Memory budget check in test (runs in CI)
#[test]
fn test_memory_budget_rpi_zero() {
    let _ = IRISCore::start_minimal();
    let rss_mb = process_memory_mb();
    assert!(
        rss_mb < 50.0,
        "Memory budget exceeded: {:.1} MB > 50 MB",
        rss_mb
    );
}
```

### 2.2 CPU at Idle

| Platform | Mode | CPU budget |
|---|---|---|
| Android | Background (screen off) | ≤ 10% averaged over 5 minutes |
| iOS | Background | ≤ 8% (iOS terminates at ~15%) |
| Raspberry Pi Zero 2W | Idle relay | ≤ 25% (50% of one core) |
| Raspberry Pi 4 | Idle relay | ≤ 10% per core |

**Measurement:** `top` measurement in CI over 5-minute idle period after startup.

**Note:** "Idle" means no active contacts, no pending messages. CPU usage will spike during contacts — these spikes are acceptable as long as the idle floor is within budget.

### 2.3 Storage

| Platform | Limit | Notes |
|---|---|---|
| Android message store | 100 MB hard cap | Enforced by SQLite PRAGMA max_page_count |
| iOS message store | 80 MB hard cap | Enforced by SQLite PRAGMA |
| Android log buffer | 50 MB | Circular, self-enforcing |
| iOS log buffer | 50 MB | Circular, self-enforcing |
| Pi Zero 2W message store | 200 MB | Enforced in config |
| Pi Zero 2W log | Unlimited (SD card) | Rotation after 7 days |

### 2.4 Startup Time

| Platform | Target |
|---|---|
| Android (cold start to routing ready) | < 3 seconds |
| iOS (cold start to routing ready) | < 3 seconds |
| Raspberry Pi Zero 2W | < 30 seconds |
| Raspberry Pi 4 | < 10 seconds |

"Routing ready" means the self-test has passed, the message store is open, and the first BLE advertisement has been broadcast.

### 2.5 Latency Hard Limits

These latency values are hard limits — if any single benchmark result exceeds these, the CI fails:

| Benchmark | Hard limit |
|---|---|
| Ed25519 sign (single) | < 5 ms on Pi Zero 2W |
| PRoPHET routing decision | < 10 ms on Pi Zero 2W |
| ML inference (full pipeline) | < 50 ms on Pi Zero 2W |
| Message encode (1 KB) | < 1 ms on Android |
| Bloom filter insert | < 10 μs |
| SQLite write (single) | < 10 ms on Pi Zero 2W |

---

## 3. Soft Budgets (Targets)

Soft budgets are tracked in the quarterly performance review. Violations do not block merges but are flagged for the engineering team.

### 3.1 Battery Drain

| Scenario | Soft budget | Notes |
|---|---|---|
| Background relay (Full mode) | ≤ 10% per day (Android) | 4,000 mAh device |
| Active relay (Full mode) | ≤ 25% per day | High traffic scenario |
| Emergency mode | ≤ 5% per day | LoRa only |
| P0 SOS send | ≤ 2% per SOS sequence | Sign + encrypt + flood |

**Current status:** Background relay is at ~16% per day (exceeds 10% soft budget). Battery optimization is an active workstream. See `docs/performance/BATTERY.md`.

### 3.2 Network Overhead

| Metric | Soft budget |
|---|---|
| Bloom filter sync overhead as % of contact time | ≤ 5% |
| Protocol header as % of P0 payload | ≤ 80% |
| Relay count overhead vs. direct delivery count | ≤ 3× |

### 3.3 Routing Quality

| Metric | Soft target |
|---|---|
| P0 delivery ratio (urban, medium density) | ≥ 99% |
| P0 1-hop latency (urban) | ≤ 3 minutes median |
| ML routing improvement vs PRoPHET | ≥ 5% delivery ratio |

---

## 4. Budget Enforcement in CI

### 4.1 CI Benchmark Job

The `bench` CI stage runs on every PR that modifies:
- Any file in `iris-core/src/`
- Any file in `iris-android/app/src/`
- Any file in `iris-sim/`

```yaml
# CI configuration (excerpt)
bench:
  runs-on: self-hosted-rpi-zero2w
  steps:
    - name: Build release binary
      run: cargo build --release --target aarch64-unknown-linux-gnu

    - name: Run benchmark suite
      run: cargo bench --workspace -- --output-format bencher 2>&1 | tee bench.txt

    - name: Check memory budget
      run: |
        ./tools/run_memory_profile.sh
        python tools/check_budget.py --input memory_profile.json --budget 50

    - name: Check latency budgets
      run: python tools/check_latency_budgets.py --input bench.txt

    - name: Upload results
      if: always()
      uses: actions/upload-artifact@v3
      with:
        name: bench-results
        path: |
          bench.txt
          memory_profile.json
```

### 4.2 Budget Check Tool

```python
# tools/check_latency_budgets.py
HARD_LIMITS = {
    "ed25519_sign": 5.0,          # ms
    "prophet_routing_decision": 10.0,  # ms
    "ml_inference_full": 50.0,    # ms
    "message_encode_1kb": 1.0,    # ms
    "bloom_filter_insert": 0.01,  # ms (10 μs)
    "sqlite_write_single": 10.0,  # ms
}

def check_budgets(bench_results: dict) -> list[str]:
    failures = []
    for bench_name, limit_ms in HARD_LIMITS.items():
        if bench_name in bench_results:
            actual_ms = bench_results[bench_name]["mean_ms"]
            if actual_ms > limit_ms:
                failures.append(
                    f"BUDGET EXCEEDED: {bench_name} = {actual_ms:.2f} ms > {limit_ms} ms limit"
                )
    return failures
```

---

## 5. Budget Review Cycle

**Quarterly performance review:**
- Date: First week of each calendar quarter
- Participants: Engineering lead, mobile engineers, embedded engineers
- Agenda:
  1. Review soft budget compliance over previous quarter
  2. Review any hard budget changes proposed
  3. Review benchmark trend charts (detect slow regressions)
  4. Adjust budgets if hardware or requirements have changed
  5. Update this document

**Budget change process:**
- Hard budget changes require a written justification and engineering lead approval
- Soft budget changes require discussion in the quarterly review
- Budget tightening (stricter limit) requires a corresponding workstream to achieve it
- Budget loosening (relaxed limit) requires documented user impact assessment

---

## 6. Current Status and Outstanding Issues

| Issue | Budget affected | Severity | Status |
|---|---|---|---|
| Android background drain ~16%/day | Battery soft budget | Medium | Active workstream |
| iOS BLE background restrictions | Latency soft target | Low | Design investigation |
| Large network routing table growth | Memory hard budget | Low (future) | Monitoring |
| ML inference on Pi Zero 2W | Latency hard limit | Medium | Optimization in progress |

---

## 7. References

- Battery analysis: `docs/performance/BATTERY.md`
- Benchmark suite: `docs/performance/BENCHMARKING.md`
- Scale analysis: `docs/performance/SCALE.md`
- Latency budgets: `docs/performance/LATENCY.md`
- Android memory limits: https://developer.android.com/topic/performance/memory-management
