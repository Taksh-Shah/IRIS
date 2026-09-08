# NEXT ACTION

**Schema version:** 1.0
**Last updated:** 2026-09-08T19:05:00+05:30

## INTERNET-ANDROID-001 deployment gate

Software verification and APK packaging are green. Obtain:

1. the numeric, phone-reachable public relay endpoint (`IP:port`); and
2. the DNS/IP reference name present in that relay's public CA certificate.

Then rebuild with `IRIS_RELAY_ENDPOINTS` and `IRIS_RELAY_SERVER_NAME`, verify the packaged ABIs/configuration, install only that APK on ADB device `b2fbcd39`, cold-launch, and capture diagnostics. The user will perform AC-12 same-LAN and AC-13 different-network bidirectional/reconnect verification.

Do not install the compile-only localhost APK.
