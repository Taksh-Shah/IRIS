# TEST-001 DISCOVER — Test Infrastructure Inventory

- **Node**: TEST-001 — Test Infrastructure (P0 TEST)
- **Stage**: DISCOVER (iter 123) — catalog existing test infra before RESEARCH/DESIGN
- **Date**: 2026-08-18
- **Precedent**: BLE-001/WIFIAWARE-001/WIFIDIRECT-001 UNDERSTAND→RESEARCH→DESIGN (C2 gap → AC-1..n at DESIGN)
- **Workspace baseline**: **613 passed / 0 failed / 1 ignored**, clippy **0**, rustfmt clean

## 1. Existing Rust test infrastructure (evidence, verified live iter 123)

### 1.1 Workspace unit tests — 613/0/1 (`cargo test --workspace --all-features`)

Per-suite breakdown (sum of `test result` lines):

| Suite | Type | Passed | Notes |
|-------|------|--------|-------|
| iriscode (crates/iris-android lib) | UniFFI binding | 5 | engine_registers_three_transports_over_bridges; start_all_brings_transports_up_over_bridges; round_trip_delivers_over_shared_mesh; engine_owns_a_live_runtime; projection_trait_drives_sync_ops |
| iris_core (lib) | in-crate unit | 553 | 50+ `#[cfg(test)] mod tests` modules |
| tests/crypto_e2e.rs | integration | 3 | E2E crypto paths |
| tests/ml_experiments.rs | integration | 8 | ML routing experiments |
| tests/obs_telemetry.rs | integration | 4 | observability telemetry |
| tests/sim_scenarios.rs | integration | 8 | sim scenarios (1 ignored) |
| iris_desktop (lib) | desktop | 5 | DesktopEngine embedding |
| tests/commands_mock.rs | integration | 3 | desktop commands |
| tests/engine_roundtrip.rs | integration | 3 | engine round-trip |
| iris_storage (lib) | storage | 7 | persistence layer |
| tests/m3_engine.rs | integration | 1 | M3 engine |
| tests/pg_store.rs | integration | 13 | PostgreSQL store (STORE-001) |

### 1.2 In-crate unit-test modules (iris-core, 50+)

protocol/codec, protocol/message_id, protocol/content_type; message_engine/{crypto, ack, dedup, expiry, fragment, lifecycle, queue, mod}; routing/{direct, flood, known_path, opportunistic, prophet, scf, scf_contact, scf_eviction, store, dedup_cache, mod}; transport/{ble, ble_att, ble_advert, internet, manager, simulated, wifiaware, wifiaware_beacon, wifi_direct, wifi_direct_serv}; crypto/{ed25519, kdf, key_directory, keygen, x25519}; identity/{rotate, small_order, store, trust_store}; security/{acl, quota, rate_limiter, replay, reputation, spam}; emergency/{sos, rate_limit, provider}; observability/mod; sim/scenario; message.

### 1.3 Property-based tests (proptest)

- Feature-gated: `cargo test --features proptest` — property suites in
  security/{spam, reputation, replay, rate_limiter, quota, acl}
  (spam.rs:187, reputation.rs:285, replay.rs:125, rate_limiter.rs:301,
  quota.rs:520, acl.rs:591).
- Manual `Arbitrary` impls: `MessagePriority` (message.rs:72), `ContentType`
  (protocol/content_type.rs:46), `ReputationEvent` (reputation.rs:93),
  `MessageClass` (rate_limiter.rs:280).

### 1.4 Fuzzing (cargo-fuzz / libFuzzer)

- `crates/iris-core/fuzz/` — `fuzz_security_engines` target
  (fuzz_targets/fuzz_security_engines.rs, SEC-001 AC-15): byte-stream decoded
  into engine ops against real tokio security engines (token buckets, quotas,
  high-water marks, reputation). Recorded **44M+ clean fuzz executions**.
- libFuzzer build artifacts present (x86_64-pc-windows-msvc).

### 1.5 Coverage

- tarpaulin cobertura: **95.6% line rate** (590/617) on the security path
  (SEC-001 AC-14 MET); recorded in SEC_001_VERIFICATION.md.

### 1.6 Adversarial/negative test patterns per transport

- `wifiaware_beacon.rs` 7 adversarial tests (exhaustive short-len sweep,
  unsupported version, unknown kind, reserved bits, oversized padded,
  candidate zero-pad).
- `wifi_direct_serv.rs` 9 adversarial tests (0..22 short-len, version, kind,
  reserved, oversize).
- `ble_att.rs`, `ble_advert.rs` adversarial cases; transport simulators
  (SimMeshCoordinator, SimP2pCoordinator, SimBle, SimAware) drive
  deterministic negative paths.

## 2. Existing JVM/Android test infrastructure (39, env-gated)

`android/app/src/test/kotlin/` (JUnit5 + kotlinx.coroutines):
- `AdapterLifecycleTest.kt` — **14** tests (rt110 timeouts, rt111 idempotence,
  rt010 invalidate, rt112 ring-buffer outbox, rt109 closed-NDP prune +
  multi-subscriber, rt108 verified-peer reuse).
- `ScanRestartPolicyTest.kt` — **4** (5/30-s backoff).
- `MeshSyncPolicyTest.kt` — **7** (battery-aware deferral).
- `KeystoreEd25519Test.kt` — **8** (TEE/software/Auto backend, persistence).
- `BatteryOptimizationGuidanceTest.kt` (+ nested `PeerIdCodecTest`) — **6**.
- Execution env-gated (no gradle/kotlinc on dev host; JDK 25 vs Gradle 8.9
  needs ≤21; toolchain host/CI).

## 3. Tooling / harness inventory

| Tool | Version/Status | Use |
|------|----------------|-----|
| cargo (Rust) | workspace | build/test/clippy/fmt |
| cargo-fuzz / libFuzzer | target present | fuzz_security_engines |
| proptest | 1.x (optional dep) | property tests |
| tarpaulin | recorded 95.6% | coverage |
| cargo-ndk | 4.1.2 pinned | Android ABI builds (env-gated) |
| uniffi-bindgen | 0.31.2 | Kotlin bindings (generated, working tree) |
| Gradle wrapper | properties-only 8.9 | Android build (env-gated) |

## 4. Gap analysis vs TEST-001 intent

Existing coverage is strong for unit/integration/property/fuzz/adversarial
testing of the Rust core and Android JVM layer. Candidate gaps for DESIGN:

- **G-1 Parallel/next-gen runner**: `cargo-nextest` not configured (no
  `.config/nextest.toml`); parallel sharding/JUnit output absent.
- **G-2 Supply-chain gates**: `cargo-audit`/`cargo-deny` not wired as a
  recurring gate (BLK-0003: SBOM/dependency-pinning node not yet built);
  `cargo audit` ran once at EMERG-001 ACCEPT (0 vulns).
- **G-3 Formal/concurrency**: no Kani/loom proofs; message-engine + routing
  concurrency currently covered by tokio tests only.
- **G-4 Mutation/coverage gate**: no `cargo-mutants`; tarpaulin only on
  security path, not a workspace gate.
- **G-5 Protocol conformance/interop**: no PROTOCOL_CONFORMANCE node
  (BLK-0004) — no interop fixtures/vectors across transports; test vectors
  (codec round-trips) exist in codec.rs only.
- **G-6 CI wiring**: no `.github/workflows` in repo (verification is
  host-driven); no cross-OS/toolchain matrix.
- **G-7 Test vectors/backcompat**: no golden-vector corpus for framing/envelope
  formats (would serve ROUTE/BLE/WiFi interop + future device tests).
- **G-8 Bench/perf harness**: no BENCHMARK harness (BLK-0005 battery/perf
  measurements pending hardware).

## 5. Status

- PROJECT_GRAPH TEST-001: status **DISCOVERED** (evidence +1, stage_note
  DISCOVER COMPLETE). Baseline **613/0/1 clippy 0 rustfmt clean** held
  (read-only pass).
- Next: **RESEARCH (iter ~124, RES-0023)** — best-in-class 2026 Rust test
  infrastructure (websearch evidence passes, L1-L5, no AI citations) over
  G-1..G-8 → verdict → **DESIGN** AC-1..n (iter ~125).