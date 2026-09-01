"""HV-2 — `iris_bench`: host-driven two-phone hardware tests for IRIS.

A Mobly suite that runs on the laptop, holds handles to both bench phones at
once, and drives each through the `IrisSnippet` RPC surface (the androidTest
APK). See `docs/bug-hunting/hardware_verification/hardware_problems_loop.md` §4.

Run:  python -m iris_bench --config iris_bench/configs/bench_2phone.yml
"""
