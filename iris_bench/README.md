# iris_bench — two-phone hardware test harness (HV-2)

Host-driven Mobly suite. Runs on the laptop, drives both bench phones at once
through the `IrisSnippet` RPC surface (the `androidTest` APK).

## Build the APKs (once per code change)

```
cd android
JAVA_HOME=<jdk21> ANDROID_HOME=<sdk> \
  java -cp gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain \
  :app:assembleDebug :app:assembleDebugAndroidTest -Dorg.gradle.java.home=<jdk21>
```

Rebuild `libiriscode.so` + regenerate the UniFFI Kotlin first if the FFI surface
changed (see `hardware_fix_log.md` HV-84).

## Run

```
pip install mobly
python -m iris_bench                                   # tier-0 smoke, default config
python -m iris_bench --tests test_p1_sends_p2_receives_10x
python -m iris_bench -c iris_bench/configs/bench_2phone.yml
```

Edit `configs/bench_2phone.yml` for your serials (`adb devices`). Set
`install: "false"` under `TestParams` to skip re-installing unchanged APKs.

## What it captures

Per test + on every failure, into
`docs/bug-hunting/hardware_verification/evidence/session-NN/<tag>/`:
- `<P>.logcat.txt` — full logcat, both phones
- `<P>.dumpsys.bluetooth_manager.txt` / `.dumpsys.wifip2p.txt`
- `<P>.meshSnapshot.json` — the `/diag` struct (transports, neighbours, counters, ring)
- `btsnoop` HCI log via `adb bugreport` — best-effort (non-root vivo), call
  `evidence.pull_btsnoop(ad, dir)` explicitly when the ATT timeline is needed.

## Snippet RPCs (`android/app/src/androidTest/.../IrisSnippet.kt`)

`startMesh` · `stopMesh` · `nodeId` · `staticX25519` · `registerPeerKey` ·
`sendText` · `awaitDelivered` · `meshSnapshot`
