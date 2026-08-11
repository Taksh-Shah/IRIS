# Diagnostics — Self-Test and Runtime Health Monitoring

**Component:** `iris-core/diagnostics` (Rust), `iris-android/diagnostics`, `iris-ios/diagnostics`
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

IRIS includes a multi-layer diagnostics system for:

1. **Startup self-test:** Verify critical subsystems before accepting traffic
2. **Runtime health monitoring:** Ongoing checks during operation
3. **Field diagnostics:** Exportable diagnostic summary for support teams
4. **Crash reporting:** On-device crash dumps with opt-in upload

Diagnostics must not interfere with P0 message delivery. All diagnostic checks run on a separate thread with lower scheduling priority than the routing engine.

---

## 2. Startup Self-Test Sequence

The startup self-test runs every time IRIS starts. It must complete within 10 seconds. If a critical test fails, IRIS enters degraded mode (routing only, no new features loaded) and alerts the user.

### 2.1 Test Sequence

```
STARTUP SELF-TEST
─────────────────
[1] Crypto subsystem
    [1a] Generate ephemeral keypair (Ed25519)               PASS/FAIL
    [1b] Sign and verify test message                        PASS/FAIL
    [1c] Derive shared secret (X25519 ECDH)                  PASS/FAIL
    [1d] BLAKE3 hash known vector                            PASS/FAIL
    [1e] AES-256-GCM encrypt/decrypt known vector            PASS/FAIL

[2] Storage subsystem
    [2a] Open SQLite database                                PASS/FAIL
    [2b] Write and read test record                          PASS/FAIL
    [2c] Check available disk space (warn if < 100 MB)       PASS/WARN
    [2d] Verify message store schema version                 PASS/FAIL

[3] Transport discovery
    [3a] BLE adapter present and powered                     PASS/FAIL/NA
    [3b] Wi-Fi adapter present                               PASS/FAIL/NA
    [3c] LoRa module connected (if hardware present)         PASS/FAIL/NA
    [3d] Cellular modem responsive (if present)              PASS/FAIL/NA

[4] Configuration
    [4a] Node keypair loaded or generated                    PASS/FAIL
    [4b] Configuration file integrity check                  PASS/FAIL
    [4c] ML model integrity (BLAKE3 hash)                    PASS/WARN

[5] Integration
    [5a] Priority queue initialized                          PASS/FAIL
    [5b] Routing table loaded from persistent store          PASS/FAIL
    [5c] Bloom filter initialized                            PASS/FAIL
```

### 2.2 Test Result Codes

| Code | Meaning | Action |
|---|---|---|
| `PASS` | Test succeeded | Continue startup |
| `WARN` | Non-critical issue detected | Log warning, continue startup |
| `FAIL` | Critical failure | Enter degraded mode or abort |
| `NA` | Hardware not present | Skip test, mark transport unavailable |

### 2.3 Critical vs Non-Critical Failures

| Test | Critical? |
|---|---|
| Crypto keypair generation | Yes — cannot operate without keys |
| Storage open | Yes — cannot store messages |
| All transports fail | Yes — cannot communicate |
| Single transport fails | No — degraded, other transports used |
| ML model integrity | No — falls back to PRoPHET |
| Disk space warning | No — warning only |

### 2.4 Startup Self-Test Implementation (Rust)

```rust
pub async fn run_startup_selftest() -> SelfTestReport {
    let mut report = SelfTestReport::new();

    // Crypto tests
    report.add(test_crypto_keygen().await);
    report.add(test_crypto_sign_verify().await);
    report.add(test_blake3_known_vector().await);

    // Storage tests
    report.add(test_storage_open().await);
    report.add(test_storage_rw().await);
    report.add(test_disk_space().await);

    // Transport discovery
    report.add(test_ble_adapter().await);
    report.add(test_wifi_adapter().await);
    report.add(test_lora_module().await);

    report
}
```

---

## 3. Runtime Diagnostics

Runtime diagnostics run continuously while IRIS is active. They do not require user initiation.

### 3.1 Transport Health Monitoring

Each transport is monitored for:

| Check | Interval | Action on failure |
|---|---|---|
| BLE adapter present | 60 s | Warn, attempt restart |
| BLE scan producing results | 300 s | Warn log |
| Wi-Fi Direct available | 60 s | Warn log |
| LoRa module responsive | 30 s | Warn, attempt AT command |
| Cellular signal strength | 60 s | Info log |

```rust
pub struct TransportHealth {
    pub transport: TransportType,
    pub status: HealthStatus,
    pub last_successful_contact: Option<Timestamp>,
    pub error_count_1h: u32,
    pub bytes_tx_1h: u64,
    pub bytes_rx_1h: u64,
}

pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Failed { reason: String },
    NotPresent,
}
```

### 3.2 Queue Depth Monitoring

| Queue depth | Alert level |
|---|---|
| < 50% capacity | Normal |
| 50–80% capacity | INFO log every 5 minutes |
| 80–95% capacity | WARN log every minute, coordinator notification |
| > 95% capacity | ERROR log, reject P5+ relay requests |

### 3.3 Routing Table Health

| Check | Alert condition |
|---|---|
| Routing table size | > 5,000 entries (warn), > 10,000 (error) |
| Delivery probability staleness | > 24h without update for any node (warn) |
| Aging step completion | > 5s per aging step (warn — performance issue) |

---

## 4. Diagnostic Commands

### 4.1 CLI (Edge Server)

```bash
# Overall health summary
iris-cli diagnostics status

# Detailed transport status
iris-cli diagnostics transports

# Queue status
iris-cli diagnostics queue

# Routing table summary
iris-cli diagnostics routing-table --top 20

# Run startup self-test on demand
iris-cli diagnostics selftest

# Export diagnostic bundle
iris-cli diagnostics export --output iris-diag-$(date +%Y%m%d).tar.gz
```

Example output:

```
IRIS Diagnostics — 2026-08-11 14:32 UTC
Node: edge-server-001 (sha256:a3f7)
Version: 1.2.0  Uptime: 4h 23m

TRANSPORTS
  BLE:          HEALTHY   last contact: 2m ago  peers seen 1h: 23
  Wi-Fi Direct: HEALTHY   last contact: 8m ago  peers seen 1h: 7
  LoRa:         HEALTHY   last contact: 12m ago peers seen 1h: 3
  Cellular:     NOT PRESENT
  Satellite:    HEALTHY   next Iridium window: 6m 14s

QUEUE
  P0: 0/10    P1: 2/50    P2: 45/200   P3: 234/500  P4: 891/2000
  Total: 1172/2760 (42% full)

ROUTING TABLE
  Entries: 847   Last aging: 47s ago   Stalest entry: 2h ago

STORAGE
  Messages: 1,172   DB size: 124 MB   Disk available: 8.4 GB
```

### 4.2 In-App Diagnostic Screen (Android/iOS)

Accessible via Settings → Diagnostics. Shows:

- Transport status summary (green/yellow/red indicators)
- Queue depth bar charts by priority
- Last 20 routing events
- Network graph summary (node count, bridge nodes)
- Export diagnostic report button

---

## 5. Crash Reporting

### 5.1 On-Device Crash Dump

When IRIS crashes, a crash dump is written to local storage before the process exits:

```
{filesDir}/iris/crashes/crash-2026-08-11T14-32-15.dump
```

Crash dump contents:
- Timestamp and IRIS version
- Platform and OS version
- Panic message and stack trace
- Last 100 log entries (in-memory circular buffer)
- Transport state at time of crash
- Queue depth at time of crash

**Message content is never included in crash dumps.**

### 5.2 Upload (Opt-In)

If the user has enabled crash reporting (Settings → Privacy → Send crash reports), the crash dump is uploaded when Internet connectivity is available:

```
POST https://crashes.iris-resilience.org/api/v1/crash
Authorization: Bearer <anonymous_session_token>
Content-Type: application/octet-stream+zstd
```

The upload token is anonymous — not linked to the user's identity. Crash reports are retained for 90 days.

### 5.3 Rust Panic Handler

```rust
use std::panic;

pub fn install_crash_handler() {
    panic::set_hook(Box::new(|info| {
        let dump = CrashDump::collect(info);
        dump.write_to_disk().expect("Failed to write crash dump");
        // Log final entry
        error!(event = "crash", message = %info, "IRIS crashed");
    }));
}
```

---

## 6. QR-Code Diagnostic Export

For field support, the diagnostic summary can be exported as a QR code that support staff can scan with their device:

```
iris-cli diagnostics qr
```

QR code encodes a compressed JSON diagnostic packet (< 2,953 bytes, QR Code version 40):

```json
{
  "v": "1.2.0",
  "ts": "2026-08-11T14:32:00Z",
  "nid": "a3f7",
  "uptime_s": 15780,
  "transports": {"ble": "ok", "wifi": "ok", "lora": "ok"},
  "queue_pct": 42,
  "routing_entries": 847,
  "errors_1h": 0,
  "warns_1h": 3,
  "last_p0_delivered_s": 1847
}
```

The QR is displayed in the app (Share → Diagnostic QR) and at the CLI.

---

## 7. References

- SQLite diagnostics: https://www.sqlite.org/pragma.html#pragma_integrity_check
- Rust panic handling: https://doc.rust-lang.org/std/panic/
- IRIS logging: `docs/operations/LOGGING.md`
- IRIS incident management: `docs/operations/INCIDENT_MANAGEMENT.md`
