# CURRENT STATE

**Schema version:** 1.0
**Last updated:** 2026-09-08T19:05:00+05:30

`INTERNET-ANDROID-001` is software-verified and held at VERIFY for deployment/hardware evidence. The original outage causes—empty relay configuration, incomplete Android lifecycle/state wiring, missing trusted-peer rendezvous, and insecure/incomplete relay/LAN behavior—have been repaired and audited.

Verified: TLS 1.3/WebPKI/exact ALPN; runtime-key relay server; authenticated registration and IPv4/IPv6 LAN; honest ACKs and bidirectional relay; bounded resources/retry; separate LAN/validated-WAN state; trust-gated stale-safe NSD; E2EE-only addressed ingress; 865/865 core; 10/10 Android Rust; focused Internet/relay/JVM tests; three ABIs and APK packaging.

Not verified: AC-12 two-phone same-LAN delivery/recovery, AC-13 different-network delivery/reconnect, and a real public CA-valid relay deployment.

Phone `b2fbcd39` is connected. Installation waits for the real relay endpoint and certificate name; empty/localhost configuration would reproduce unavailable WAN.
