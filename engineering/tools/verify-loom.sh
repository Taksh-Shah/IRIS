#!/usr/bin/env bash
# DEC-TEST-0004 / TEST-001 AC-4: run the loom leaf-structure concurrency models.
#
# `--cfg loom` must be applied ONLY to the loom test crate: a global
# RUSTFLAGS="--cfg loom" also re-cfgs tokio and breaks its `net` module
# (`#![cfg(not(loom))]`), so we build the target via `cargo rustc -- --cfg loom`
# and then execute the produced binary.
#
# Windows dev host equivalent:
#   cargo rustc -p iris-core --test loom_models -- --cfg loom
#   <newest target/debug/deps/loom_models-*.exe>
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo rustc -p iris-core --test loom_models -- --cfg loom

exe="$(ls -t target/debug/deps/loom_models-* 2>/dev/null \
  | grep -vE '\.(d|rmeta|pdb|exp|lib)$' | head -n1)"
if [ -z "$exe" ]; then
  echo "::error::loom model binary not found" >&2
  exit 1
fi

"$exe" --test-threads=1
