# CHECKPOINT — CHK-0009

**Schema version:** 1.0
**Timestamp:** 2026-09-08T19:05:00+05:30
**Node:** INTERNET-ANDROID-001
**Stage:** VERIFY / external gate

Software implementation, adversarial review and automated verification are complete. Reproduced: iris-core 865/865, iris-android 10/10, Internet 24/24, relay codec 8/8, relay server 6/6, Android focused JVM, three release native ABIs and configured APK packaging.

Evidence: `FAIL-0009.md`, `INTERNET-ANDROID-001_TEST.md`, `INTERNET-ANDROID-001_SECURITY_REVIEW.md`, and `INTERNET-ANDROID-001_VERIFICATION.md`.

External gate: obtain a real public relay `IP:port` and CA certificate reference name before building/installing the hardware candidate. AC-12/AC-13 remain user-run.
