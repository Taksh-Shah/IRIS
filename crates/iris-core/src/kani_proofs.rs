//! Kani proofs for TEST-001 AC-3 (DEC-TEST-0003).
//!
//! Scope: **pure functions only**. Kani does not model concurrency, so every
//! harness here targets a `pub(crate)`/`pub` pure function with no tokio/I/O
//! surface. Compiled only when `--cfg kani` is set (i.e. inside `cargo kani`);
//! regular builds never include this module.
//!
//! Targets (≥ 4 pure-fn families, per TEST_001_DESIGN.md AC-3):
//!  1. codec decode bounds — `MessageId::from_slice` (exact 16-byte gate),
//!     `ContentType::from_u8`/`as_u8`, `MessagePriority::from_u8`/`as_u8`
//!     (wire codes 1..=16 / 0..=7 round-trip; out-of-range rejected).
//!  2. fragment arithmetic — `fragment::split_payload` (chunk ≤ budget +
//!     byte-conservation reassembly).
//!  3. quota/rate-limiter arithmetic — `rate_limiter::refill_tokens`
//!     (saturating, capped ≤ burst) and `quota::evict_amount`
//!     (bounded ≤ used, ≥ 1 when the account holds bytes; no wraparound).
//!  4. PRoPHET invariant — `prophet::floor_intervals` (floor division,
//!     zero-interval guard, no u128->u64 truncation).
//!
//! Run (Linux CI): `cargo kani --package iris-core` — the `kani` crate and
//! `--cfg kani` are injected by the Kani toolchain; the proof runtime is
//! CBMC-based and is not expected to run on the Windows development host.

#![allow(missing_docs)]

use crate::message::MessagePriority;
use crate::message_engine::fragment::split_payload;
use crate::protocol::{content_type::ContentType, message_id::MessageId};
use crate::routing::prophet::floor_intervals;
use crate::security::{quota::evict_amount, rate_limiter::refill_tokens};

/// Proof 1a — `MessageId::from_slice` is an exact 16-byte gate: `Some` iff the
/// slice length is exactly 16, no panic on longer/shorter transient buffers.
#[kani::proof]
fn codec_message_id_length_gate() {
    let buf: [u8; 24] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= buf.len());
    let s = &buf[..len];
    match MessageId::from_slice(s) {
        Some(id) => {
            kani::assert(len == MessageId::LEN, "Some only when len == 16");
            kani::assert(id.as_bytes() == &buf[..16], "bytes preserved");
        }
        None => kani::assert(len != MessageId::LEN, "None only when len != 16"),
    }
}

/// Proof 1b — `ContentType` wire-code bounds and decode/encode round-trip.
#[kani::proof]
fn codec_content_type_wire_bounds() {
    let v: u8 = kani::any();
    match ContentType::from_u8(v) {
        Some(ct) => {
            kani::assert((1..=16).contains(&v), "accepted only in 1..=16");
            kani::assert(ct.as_u8() == v, "as_u8 round-trips from_u8");
        }
        None => kani::assert(!(1..=16).contains(&v), "out-of-range rejected"),
    }
}

/// Proof 1c — `MessagePriority` wire-code bounds and round-trip.
#[kani::proof]
fn codec_priority_wire_bounds() {
    let v: u8 = kani::any();
    match MessagePriority::from_u8(v) {
        Some(p) => {
            kani::assert((0..=7).contains(&v), "accepted only in 0..=7");
            kani::assert(p.as_u8() == v, "as_u8 round-trips from_u8");
        }
        None => kani::assert(!(0..=7).contains(&v), "out-of-range rejected"),
    }
}

/// Proof 2 — `split_payload` never emits a chunk over `max_chunk` and the
/// concatenation is byte-for-byte the original payload (no bytes lost/added).
#[kani::proof]
#[kani::unwind(18)]
fn fragment_split_payload_conservation_and_budget() {
    let payload: [u8; 16] = kani::any();
    let max_chunk: usize = kani::any();
    kani::assume(max_chunk >= 1 && max_chunk <= 16);

    let chunks = split_payload(&payload, max_chunk);
    kani::assert(!chunks.is_empty(), "always at least one chunk (even empty)");
    if payload.is_empty() {
        kani::assert(
            chunks.len() == 1 && chunks[0].is_empty(),
            "empty payload -> exactly one empty chunk",
        );
    } else {
        for c in &chunks {
            kani::assert(!c.is_empty(), "non-empty payload yields non-empty chunks");
        }
    }
    let mut reassembled = Vec::new();
    for c in &chunks {
        kani::assert(c.len() <= max_chunk, "chunk never exceeds budget");
        kani::assume(reassembled.len() + c.len() <= 32);
        reassembled.extend_from_slice(c);
    }
    kani::assert(reassembled.len() == payload.len(), "no bytes lost or added");
    kani::assert(
        &reassembled[..] == payload.as_slice(),
        "byte-exact reassembly",
    );
}

/// Proof 3a — srTCM refill arithmetic never exceeds `burst` and never wraps.
#[kani::proof]
fn rate_limiter_refill_never_exceeds_burst() {
    let tokens: u64 = kani::any();
    let intervals: u64 = kani::any();
    let rate_per_sec: u64 = kani::any();
    let burst: u64 = kani::any();
    let out = refill_tokens(tokens, intervals, rate_per_sec, burst);
    kani::assert(out <= burst, "refill capped at burst");
    kani::assert(
        out >= tokens.min(burst),
        "monotone non-decreasing toward burst",
    );
}

/// Proof 3b — quota eviction amount: bounded by `used`, and strictly positive
/// whenever the account holds bytes (no zero/overshoot eviction).
#[kani::proof]
fn quota_eviction_bounded_and_positive() {
    let used: u64 = kani::any();
    let quota: u64 = kani::any();
    let out = evict_amount(used, quota);
    kani::assert(out <= used, "evict at most what the account holds");
    if used > 0 {
        kani::assert(out >= 1, "non-empty account always yields >= 1 byte");
    } else {
        kani::assert(out == 0, "empty account evicts nothing");
    }
}

/// Proof 4 — PRoPHET aging `floor_intervals` is exact floor division when the
/// interval is non-zero (never truncated by the u128→u64 cast), and a zero
/// interval safely maps to 0 (no division by zero).
#[kani::proof]
fn prophet_floor_intervals_division() {
    let elapsed_ns: u64 = kani::any();
    let interval_ns: u64 = kani::any();
    let elapsed = std::time::Duration::from_nanos(elapsed_ns);
    let interval = std::time::Duration::from_nanos(interval_ns);

    let k = floor_intervals(elapsed, interval);
    if interval_ns == 0 {
        kani::assert(k == 0, "zero interval guard returns 0");
    } else {
        kani::assert(k == elapsed_ns / interval_ns, "exact floor division");
    }
}
