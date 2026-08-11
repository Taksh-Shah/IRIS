# IRIS Fuzzing Strategy

## Overview

Fuzzing finds bugs that manual review and property tests miss — especially in parsing code that handles untrusted external input. IRIS is a mesh relay: every message comes from an untrusted external source. The CBOR parser is the primary attack surface.

## Tool: cargo-fuzz

`cargo-fuzz` wraps libFuzzer (LLVM's coverage-guided fuzzer). It compiles the fuzz target with AddressSanitizer and UndefinedBehaviorSanitizer, then generates inputs guided by code coverage feedback.

```bash
# Install
cargo install cargo-fuzz

# Initialize (creates fuzz/ directory)
cargo fuzz init

# List targets
cargo fuzz list

# Run a target (indefinitely)
cargo fuzz run fuzz_envelope_parse

# Run for 30 minutes
cargo fuzz run fuzz_envelope_parse -- -max_total_time=1800

# Run with specific corpus
cargo fuzz run fuzz_envelope_parse corpus/envelope/
```

## Fuzz Targets

### Primary Target: CBOR Envelope Parser

All external input enters IRIS through `Envelope::from_cbor()`. This is the highest-priority fuzz target.

```rust
// fuzz/fuzz_targets/fuzz_envelope_parse.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use iris_proto::Envelope;

fuzz_target!(|data: &[u8]| {
    // Must: not panic, not crash
    // May: return Ok or Err — both are acceptable
    let _ = Envelope::from_cbor(data);
});
```

### Target: Message Deserializer

```rust
// fuzz/fuzz_targets/fuzz_message_deserialize.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use iris_proto::{IrisMessage, MessageType};

fuzz_target!(|data: &[u8]| {
    let _ = IrisMessage::from_cbor(data);
    // Also try parsing as specific message types
    let _ = iris_proto::emergency::SosMessage::from_cbor(data);
    let _ = iris_proto::routing::ContactMessage::from_cbor(data);
});
```

### Target: Routing Input Parser

```rust
// fuzz/fuzz_targets/fuzz_routing_input.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use iris_core::routing_engine::ContactMessage;

fuzz_target!(|data: &[u8]| {
    // Routing messages from peers — all untrusted
    if let Ok(msg) = ContactMessage::from_cbor(data) {
        // If parsing succeeds, also test that processing doesn't panic
        let mut router = iris_core::routing_engine::ProphetRouter::new(Default::default());
        let _ = router.process_contact_message(&msg);
    }
});
```

### Target: Storage Input (Query Builder)

```rust
// fuzz/fuzz_targets/fuzz_storage_query.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use iris_storage::QueryBuilder;
use arbitrary::Arbitrary;

#[derive(Debug, Arbitrary)]
struct QueryInput {
    node_id: Vec<u8>,
    priority: u8,
    limit: u32,
    offset: u32,
}

fuzz_target!(|input: QueryInput| {
    // Fuzzer generates arbitrary QueryInput structs
    // Storage layer must safely handle all inputs without SQL injection or panic
    let storage = iris_storage::InMemoryStorage::new_sync();
    let _ = storage.get_messages_for_recipient_sync(
        &input.node_id,
        input.priority.min(7),
        input.limit.min(10000),
        input.offset,
    );
});
```

## Corpus Management

Seed corpus: manually curated valid messages plus boundary cases.

```
fuzz/corpus/fuzz_envelope_parse/
├── valid_p0_sos.cbor          # Valid P0 SOS message
├── valid_p4_text.cbor         # Valid P4 text message
├── valid_broadcast.cbor       # Valid emergency broadcast
├── min_valid.cbor             # Minimum valid envelope (smallest size)
├── max_ttl.cbor               # Maximum TTL value
├── max_hops.cbor              # Maximum hop count
├── zero_payload.cbor          # Zero-length payload
├── max_payload.cbor           # Maximum payload (255 bytes, LoRa limit)
├── all_zeros.cbor             # All-zero bytes
├── all_ones.cbor              # All 0xFF bytes
├── single_byte.cbor           # Single byte inputs
└── cbor_indefinite_len.cbor   # CBOR indefinite-length encoding
```

Generate seed corpus from test fixtures:

```rust
// tools/generate_fuzz_corpus.rs
fn main() {
    let p0_sos = SosMessage::new(NodeId::test_node());
    let envelope = Envelope::wrap(p0_sos, Priority::P0, Ttl::hours(1));
    let bytes = envelope.to_cbor().unwrap();
    std::fs::write("fuzz/corpus/fuzz_envelope_parse/valid_p0_sos.cbor", bytes).unwrap();
    // ... more corpus entries
}
```

## What Must Never Happen

When fuzzing, the following outcomes are bugs:

### Panics
Any `panic!`, `unwrap()`, `expect()` on fuzz input is a bug. All parser code must use `?` or explicit `match`/`if let`.

```rust
// BAD: panics on unexpected CBOR
let priority = cbor_map["priority"].as_u8().unwrap();

// GOOD: returns Err on unexpected CBOR
let priority = cbor_map.get("priority")
    .and_then(|v| v.as_u8())
    .ok_or(ParseError::MissingField("priority"))?;
```

### Memory Unsafety
Any AddressSanitizer report (heap-buffer-overflow, use-after-free, stack-overflow) is a critical bug. Rust prevents most of these; `unsafe` blocks are the primary risk. No `unsafe` in parser code.

### OOM / Allocation Bomb
A fuzz input of 20 bytes that causes IRIS to allocate 1GB of memory is a DoS vulnerability. All size-bounded allocations must check limits:

```rust
fn parse_payload(cbor: &Value) -> Result<Vec<u8>, ParseError> {
    let bytes = cbor.as_bytes().ok_or(ParseError::InvalidType("bytes"))?;
    if bytes.len() > MAX_PAYLOAD_SIZE {
        return Err(ParseError::PayloadTooLarge { size: bytes.len(), max: MAX_PAYLOAD_SIZE });
    }
    Ok(bytes.to_vec())
}
```

`MAX_PAYLOAD_SIZE` is enforced before allocation. The fuzzer should not be able to trigger allocation larger than this limit.

### Integer Overflow
TTL and hop count calculations must use checked arithmetic:

```rust
// BAD: can overflow on malicious input
let new_ttl = received_ttl - elapsed_seconds;

// GOOD: saturating or checked
let new_ttl = received_ttl.saturating_sub(elapsed_seconds);
// or:
let new_ttl = received_ttl.checked_sub(elapsed_seconds)
    .ok_or(ParseError::Overflow("TTL underflow"))?;
```

## CI Integration

Fuzzing runs in CI on PRs that touch parser code:

```yaml
# .github/workflows/fuzz.yml
on:
  push:
    paths:
      - 'crates/iris-proto/src/**'
      - 'crates/iris-core/src/**'
      - 'fuzz/**'

jobs:
  fuzz:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly   # cargo-fuzz requires nightly
      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz
      - name: Fuzz envelope parser (30 min)
        run: cargo fuzz run fuzz_envelope_parse -- -max_total_time=1800 -fork=4
      - name: Fuzz message deserializer (30 min)
        run: cargo fuzz run fuzz_message_deserialize -- -max_total_time=1800 -fork=4
      - name: Upload crash artifacts
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: fuzz-crashes
          path: fuzz/artifacts/
```

When a crash is found:
1. Crash input is saved in `fuzz/artifacts/fuzz_target_name/`
2. CI uploads crash input as artifact
3. Developer reproduces: `cargo fuzz run fuzz_envelope_parse fuzz/artifacts/fuzz_envelope_parse/crash-*`
4. Minimizes input: `cargo fuzz tmin fuzz_envelope_parse crash-input`
5. Fix the bug, add minimized input to corpus as a regression test

## Continuous Fuzzing

For long-running coverage, consider OSS-Fuzz integration. OSS-Fuzz runs fuzz targets continuously on Google infrastructure and reports new crashes. Open-source projects can apply for OSS-Fuzz coverage.

Local long-running fuzzing:
```bash
# Run with coverage measurement
cargo fuzz coverage fuzz_envelope_parse

# View coverage report
cargo fuzz coverage fuzz_envelope_parse --view
```
