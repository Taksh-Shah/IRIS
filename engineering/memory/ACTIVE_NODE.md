# ACTIVE NODE

**Schema version:** 1.0
**Last updated:** 2026-09-08T19:05:00+05:30

## INTERNET-ANDROID-001 — Android Internet and LAN Relay Recovery

- **Stage:** VERIFY / external gate
- **Software verdict:** PASS
- **Reproduced:** iris-core 865/865; iris-android 10/10; Internet 24/24; relay codec 8/8; relay server 6/6; focused Android JVM pass; three native ABIs and configured APK packaging pass
- **Connected hardware:** `b2fbcd39`, vivo 2004, Android 12/API 31
- **Gate:** no phone-reachable relay `IP:port` and no CA-certificate reference name are configured. AC-12/AC-13 remain user-run physical verification.

Do not mark COMPLETE or install an empty/localhost relay build. Obtain the two deployment values, build/install the configured APK, then hand physical LAN/WAN testing to the user.
