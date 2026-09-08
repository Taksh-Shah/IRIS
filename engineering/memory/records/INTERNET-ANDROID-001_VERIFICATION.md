# INTERNET-ANDROID-001 — Verification

- **Date:** 2026-09-08
- **Decision:** APPROVE SOFTWARE; HOLD NODE AT VERIFY/HUMAN GATE

| Criterion | Status | Evidence |
|---|---|---|
| AC-1 | PASS | Separate LAN/validated-WAN state and Internet-aware startup/diagnostics |
| AC-2 | PASS | Atomic endpoint + TLS-name configuration; empty config reports unavailable truthfully |
| AC-3 | PASS | TLS 1.3, exact ALPN, WebPKI validation, runtime server key material |
| AC-4 | PASS | Verified/authority-root-only bootstrap; NSD publish/withdraw into routing |
| AC-5 | PASS-SOFTWARE | Authenticated IPv4/IPv6 LAN, preference/fallback and lifecycle tests |
| AC-6 | PASS-SOFTWARE | Bidirectional TLS relay and bounded recovery tests; live WAN pending |
| AC-7 | PASS | Identity/source/target/generation and resource/rate hardening tests |
| AC-8 | PASS | Explicit ACK status; unknown/offline is not reported delivered |
| AC-9 | PASS-SOFTWARE | Serialized async-safe NSD/network lifecycle plus generation-lease race regression; device observation pending |
| AC-10 | PARTIAL | Core/property/LAN/relay suites pass; full Android callback, negative TLS/ALPN, heartbeat-expiry and network-change matrix remains open |
| AC-11 | PARTIAL | Android compile/focused JVM, Rust FFI, three ABIs and APK packaging pass; configured install/cold-launch/diagnostics await real relay values |
| AC-12 | GATED | User-run physical same-LAN two-phone bidirectional/recovery test |
| AC-13 | GATED | Public CA-valid relay plus user-run different-network/reconnect test |
| AC-14 | PASS | Security review has no unresolved CRITICAL/HIGH software finding |
| AC-15 | PARTIAL | Evidence is reproducible; final approval waits for AC-10 remainder and AC-12/AC-13 |

The implementation is suitable for a correctly configured hardware candidate.
It is not correct to install an empty or localhost-configured APK and represent
WAN relay as available. Required deployment inputs are a phone-reachable numeric
`IP:port` and the DNS/IP certificate reference name for that relay.
