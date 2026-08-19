#!/usr/bin/env bash
# IOS-001 AC-1/AC-2/AC-15 — build the IrisCore.xcframework from iris-ios
# staticlibs (CI-leg; must run on macOS — Windows dev host has no Xcode).
#
#   cargo build per iOS slice
#   uniffi-bindgen generate --library (0.31.x metadata-based flow)
#   xcodebuild -create-xcframework  -> ios/IrisFramework/IrisCore.xcframework
#
# The committed Swift bindings under ios/IRIS/RustFFI/ are regenerated in-place
# so the AC-2 grep/diff guard verifies freshness.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
UNIFFI="${UNIFFI_BINDGEN:-uniffi-bindgen}"
OUT_DIR="ios/IRIS/RustFFI"
FRAMEWORK_DIR="ios/IrisFramework"
mkdir -p "$FRAMEWORK_DIR"

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
for t in "${TARGETS[@]}"; do
  cargo build -p iris-ios --release --target "$t"
done

# 0.31.x: --library (metadata-based); UDL generation is stale for this repo.
"$UNIFFI" generate --library \
  "target/aarch64-apple-ios/release/libIrisCore.a" \
  --language swift --out-dir "$OUT_DIR"

xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/release/libIrisCore.a" \
  -headers "$OUT_DIR" \
  -library "target/aarch64-apple-ios-sim/release/libIrisCore.a" \
  -headers "$OUT_DIR" \
  -library "target/x86_64-apple-ios/release/libIrisCore.a" \
  -headers "$OUT_DIR" \
  -output "$FRAMEWORK_DIR/IrisCore.xcframework"

echo "OK: $FRAMEWORK_DIR/IrisCore.xcframework"