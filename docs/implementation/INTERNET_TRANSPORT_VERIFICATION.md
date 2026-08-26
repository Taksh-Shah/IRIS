# INTERNET-001 Verification — Transport Implementation

**Date**: 2026-08-12
**Node**: INTERNET-001
**Phase**: CORE_PROTOCOL_IMPLEMENTATION (authorized Option A)

## What Was Built

Rust workspace scaffolded at repo root (`Cargo.toml`, `crates/iris-core/`):

- `src/transport/mod.rs` — `Transport` trait, `TransportState` state machine,
  `TransportCapabilities`, `TransportCost`, cost classes, `RadioConflictGroup`,
  `TopologyEvent` (implements TRANSPORT-001 spec)
- `src/transport/manager.rs` — `TransportManager` registry + `select_transports`
  scoring (state bonus, latency, cost penalty, bandwidth bonus, congestion)
- `src/transport/internet.rs` — INTERNET-001: TCP length-prefixed framing,
  connection pool (max 3/relay, 90s idle), exponential backoff 1s→30s ±25%
  jitter, relay address persistence
- `src/transport/ble.rs` — BLE-001 scaffold: `BleAdapter` trait, `BleTransport`
  state machine, `SimulatedBleAdapter` (no hardware)
- `src/transport/simulated.rs` — deterministic seeded in-memory transport
  (SIM-001 groundwork)
- `src/message.rs` — `MessagePriority` P0–P7, `PeerId`, message/link types

## Acceptance Evidence

| Criterion (ACCEPTANCE_POLICY TRANSPORT) | Status |
|------------------------------------------|--------|
| TransportAdapter trait implemented and type-checked | ✅ `cargo build` clean; `cargo clippy` 0 warnings |
| Capability matrix entry completed | ✅ `internet_capabilities()` + `BLE_COST`/`WIFI_AWARE_COST`/`LORA_COST` |
| Platform limitations documented | ✅ INTERNET.md + inline docs (CGNAT, no hole-punching) |
| Unit + integration tests passing | ✅ 27/27 (incl. localhost TCP roundtrip) |
| Background capability tested | ✅ code-level; device test deferred |
| Battery impact measured (BENCH-XXX) | ⛔ deferred — BLK-0005, no benchmark run |
| Physical device test | ⛔ deferred — BLK-0005, requires phone + real relay |

## Test Results (2026-08-12)

```
cargo test — 27 passed, 0 failed
transport::internet: frame_roundtrip, frame_rejects_oversized_payload,
                     backoff_sequence, backoff_jitter_bounds,
                     send_receive_localhost_roundtrip (real TCP), 
                     send_without_connect, initial_state, cost_snapshot,
                     capability_matrix_facts
transport::manager: register/select/unavailable/size-filter/multipath/single-best/dup/deregister
transport::ble: adapter present/absent, send, gatt_write->incoming, ios-background flag, mac parse
transport::simulated: zero-loss deliver, full-loss drop, state transitions
```

## Known Limitations (tracked under BLK-0005)

1. Physical-device test on 2+ Android phones not yet performed — requires hardware
2. Battery measurement (EXP-003 / GAP-003) not run
3. QUIC path (quinn) and WebSocket fallback not yet implemented — TCP framing is the
   shared substrate; QUIC/WS layer deferred
4. Android ConnectivityManager / iOS NWPathMonitor network-change hooks pending
   platform crate
5. **TLS not implemented (RF-35):** connections are plaintext TCP. `INTERNET.md` §TCP+TLS
   specifies `tokio-rustls` with relay SPKI pinning; this has not been added. Payload
   confidentiality is provided by the IRIS envelope (CRYPTO-001), but frame metadata
   (timing, sizes, endpoints) is exposed and frames are unauthenticated at the transport
   layer, enabling injection into the envelope parser. Fix: wrap `TcpStream` in
   `tokio-rustls` `TlsStream` with pinned relay certificate before the pool layer.
