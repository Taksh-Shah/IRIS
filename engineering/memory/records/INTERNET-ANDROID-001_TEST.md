# INTERNET-ANDROID-001 — Software test record

- **Date:** 2026-09-08
- **Scope:** Android Internet/LAN client, authenticated relay protocol/server,
  routing ingress, lifecycle, native packaging
- **Verdict:** SOFTWARE PASS; live infrastructure and physical AC-12/AC-13 gated

## Reproduced results

| Test surface | Result |
|---|---|
| `cargo test -p iris-core --all-features --lib` | **865 passed, 0 failed** in 239.71 s |
| `transport::internet` focused module | **24 passed, 0 failed** |
| `transport::relay_protocol` focused module | **8 passed, 0 failed** |
| `iris-relay-server` binary tests | **6 passed, 0 failed** |
| `cargo test -p iris-android --all-features` | **10 passed, 0 failed** after deterministic routing-fixture repair |
| Android Internet/NSD focused JVM tests | **BUILD SUCCESSFUL** |
| Three-ABI release native build | **PASS**: arm64-v8a, armeabi-v7a, x86_64 |
| Configured `:app:assembleDebug` | **BUILD SUCCESSFUL**; all three `libiriscode.so` entries present |

APK software-build artifact SHA-256 (localhost compile-only configuration):
`E09D08B48D11859D155DFC1B4191BAEFEB8B33151B700F20686E5B6C85FACF21`.
It was intentionally **not installed** because localhost is not a phone-reachable
production relay.

## Coverage highlights

- TLS 1.3 plus exact `iris-relay/1` ALPN and CA/server-name validation.
- Signed challenge registration, source binding, cross-domain signature
  separation, generation-safe replacement, bounded queues/connections and
  reconnect-persistent identity rate limiting.
- Actual TLS server bidirectional A→B/B→A relay exchange and honest ACK status.
- Authenticated LAN IPv4/IPv6, wrong-identity rejection, relay fallback,
  connection reuse without cross-peer scope reuse, network-loss invalidation,
  bounded retry and listener restart/shutdown.
- Android validated-WAN policy, NSD address formatting, serialized lifecycle,
  generation-owned listener lease regression, trust-gated route publication and
  nonblocking FFI.
- Production message ingress rejects addressed signed plaintext without E2EE.

## Honest exclusions

- The full Android JVM suite is 99/100 because of an unrelated, pre-existing
  Android/desktop command-list parity test; focused Internet JVM tests pass.
- No public relay endpoint or CA-certificate reference name exists in repository
  configuration. A compile-only localhost value is not deployment evidence.
- AC-12 same-LAN two-phone and AC-13 different-network relay tests require the
  user's physical run and a deployed named relay.
- The Android suite does not yet simulate the full `NsdManager` callback graph,
  real certificate/ALPN handshake rejection, heartbeat expiry, or end-to-end
  network-change reconnect. These AC-10 rows remain partial rather than inferred.
