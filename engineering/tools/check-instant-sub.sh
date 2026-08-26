#!/usr/bin/env bash
# SYS-1 guard: ban `Instant::now() -` in production Rust source.
# `Instant::sub` panics on Linux/Android when process uptime < the duration
# (CLOCK_MONOTONIC starts at zero on boot). Use duration_since() instead.
# See neighbor_table.rs:236 for the incident record.
set -euo pipefail

FOUND=$(grep -rn "Instant::now() -" crates/ --include="*.rs" \
        | grep -v "^\s*//" \
        | grep -v "#\[cfg(test)\]" || true)

if [ -n "$FOUND" ]; then
    echo "ERROR: Instant::now() - found in source (SYS-1 regression):"
    echo "$FOUND"
    echo "Use Instant::now().duration_since(earlier) < window instead."
    exit 1
fi
echo "SYS-1 guard: clean."
