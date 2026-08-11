# L0 Deterministic Networking Layer

## Overview

The deterministic layer is the only required component of the IRIS intelligence stack.
It must work in the worst possible conditions: zero battery to spare, zero connectivity
to any infrastructure, zero history of prior contacts, running on a device with 512MB RAM
and a 2018-era processor.

L0 is the system that saves lives when everything else has failed. It is never disabled.

## Design Constraints

### No External Dependencies

L0 is implemented as a pure Rust module with no dependencies outside the Rust standard library
(and `no_std`-compatible subsets of it):

```toml
# Cargo.toml for iris-routing-l0
[dependencies]
# Nothing. Zero. No crates.
# All algorithms implemented from scratch, audited, and tested independently.

[profile.release]
# Optimized for size and correctness, not speed
opt-level = "s"
overflow-checks = true  # Always check arithmetic overflow
```

### No Floating Point

All computations use fixed-point arithmetic. Rationale:
1. Some embedded targets lack hardware floating point (the IRIS LoRa edge node uses Cortex-M4
   which has FPU, but we maintain no_std compatibility for possible future Cortex-M0 targets)
2. Floating point arithmetic is non-deterministic across platforms (rounding modes differ)
3. Fixed-point is easier to reason about for security review

Probability representation: `u16` in range [0, 65535] representing [0.0, 1.0].
```rust
pub type Probability = u16;
pub const PROB_ZERO: Probability = 0;
pub const PROB_ONE: Probability = 65535;
pub const PROB_HALF: Probability = 32767;

// Multiply two probabilities: (a/65535) × (b/65535) = (a*b)/(65535*65535)
// Use 64-bit intermediate to avoid overflow
pub fn prob_mul(a: Probability, b: Probability) -> Probability {
    ((a as u64 * b as u64) / 65535) as u16
}
```

### Deterministic Output

Given the same inputs (neighbor list, message queue state, routing table), L0 always
produces the same routing decision. No randomness in the critical path.
This is required for reproducible testing and debugging.

The only randomness in L0: message_id generation (UUID v4). This is in message creation,
not routing decisions.

### Property-Based Testing

All L0 algorithms are tested with property-based tests using the `quickcheck` crate:

```rust
#[quickcheck]
fn priority_queue_invariant(messages: Vec<TestMessage>) -> bool {
    let mut queue = PriorityQueue::new();
    for msg in &messages {
        queue.push(msg.clone());
    }
    // Invariant: messages dequeued in priority order (P0 first)
    let mut last_priority = 0u8;
    while let Some(msg) = queue.pop() {
        assert!(msg.priority >= last_priority);
        last_priority = msg.priority;
    }
    true
}
```

## Algorithms

### 1. Direct Delivery

Check if the destination is directly reachable (current neighbor).

```rust
pub fn can_deliver_directly(
    destination: &NodeId,
    current_neighbors: &[NodeId],
) -> bool {
    current_neighbors.contains(destination)
}
```

Time complexity: O(n) where n = number of neighbors. In practice, n ≤ 50 (BLE limit).
Checked first for every routing decision — direct delivery is always preferred.

### 2. Hop-Limited Flood

Forward to all neighbors within hop limit. Used for P0-P1 in normal mode,
and all P0-P3 in emergency mode.

```rust
pub fn flood_next_hops(
    message: &Message,
    neighbors: &[NodeId],
    seen_cache: &SeenMessageCache,
) -> Vec<NodeId> {
    if message.hop_count >= message.max_hops {
        return vec![];  // Hop limit exceeded
    }
    neighbors.iter()
        .filter(|n| !seen_cache.has_forwarded_to(*n, &message.message_id))
        .cloned()
        .collect()
}
```

### 3. Shortest Path (Dijkstra)

When routing topology is known (nodes have shared contact graphs), find the shortest
path to the destination. Used when network is connected enough to have topology knowledge.

```rust
pub fn dijkstra_next_hop(
    source: &NodeId,
    destination: &NodeId,
    topology: &ContactGraph,
) -> Option<NodeId> {
    // Standard Dijkstra, returns first hop on shortest path
    // Weights: 1 per hop (unweighted — all links equal)
    // Returns None if destination unreachable from current topology knowledge
    dijkstra_shortest_path(source, destination, topology)
        .map(|path| path[1].clone())  // path[0] = source, path[1] = first hop
}
```

### 4. TTL Enforcement

```rust
pub fn is_message_expired(message: &Message, current_time: i64) -> bool {
    current_time > message.timestamp + message.ttl_seconds as i64
}

pub fn filter_expired(
    messages: impl Iterator<Item = Message>,
    current_time: i64,
) -> impl Iterator<Item = Message> {
    messages.filter(move |m| !is_message_expired(m, current_time))
}
```

### 5. Priority Queue

```rust
pub struct PriorityQueue {
    // Binary max-heap, priority ordering
    // P0 at top (highest priority = lowest numeric value, so we invert)
    inner: BinaryHeap<PriorityEntry>,
}

impl PriorityQueue {
    pub fn push(&mut self, message: Message) {
        self.inner.push(PriorityEntry { message });
    }
    
    pub fn pop(&mut self) -> Option<Message> {
        self.inner.pop().map(|e| e.message)
    }
    
    // P0 always at front, then P1, etc.
    // Within same priority: FIFO order (earliest timestamp first)
}
```

### 6. Deduplication (Bloom Filter + LRU)

```rust
pub struct SeenMessageCache {
    bloom: BloomFilter,           // Fast approximate check (1MB, 1M entries)
    exact: LruCache<MessageId, ()>, // Exact check for recent messages (50K entries)
}

impl SeenMessageCache {
    pub fn is_seen(&self, message_id: &MessageId) -> bool {
        // Bloom filter: if not in bloom, definitely not seen (no false negatives)
        if !self.bloom.might_contain(message_id) {
            return false;
        }
        // Bloom says maybe seen: check exact cache
        self.exact.contains(message_id)
    }
    
    pub fn mark_seen(&mut self, message_id: MessageId) {
        self.bloom.insert(&message_id);
        self.exact.put(message_id, ());
    }
}
```

### 7. Storage Eviction

When storage is full, evict messages in this order:
1. Expired messages (by TTL) — always evict first
2. P7 messages (lowest priority) — oldest first
3. P6, then P5, then P4 — oldest first within each priority
4. P3 — oldest first (only evict P3 if needed for P0-P2)
5. P2 and P1 — only evict if needed for P0
6. P0 — never evicted (P0 evicts everything else if needed)

```rust
pub fn evict_for_space(
    store: &mut MessageStore,
    needed_bytes: u64,
    incoming_priority: Priority,
) -> Result<u64, EvictionError> {
    let mut freed = 0u64;
    
    // Phase 1: evict expired
    freed += store.evict_expired(now());
    if freed >= needed_bytes { return Ok(freed); }
    
    // Phase 2: evict by priority (P7 down to incoming_priority)
    for priority in (incoming_priority + 1..=7u8).rev() {
        freed += store.evict_oldest_of_priority(Priority::from(priority), needed_bytes - freed);
        if freed >= needed_bytes { return Ok(freed); }
    }
    
    Err(EvictionError::InsufficientSpace { freed, needed: needed_bytes })
}
```

## Performance Targets

| Operation | Target | Measured (reference hardware) |
|-----------|--------|-------------------------------|
| Direct delivery check | <10μs | ~3μs |
| Deduplication check | <100μs | ~15μs (Bloom) + ~20μs (LRU) |
| Priority queue push | <500μs | ~50μs |
| Priority queue pop | <500μs | ~50μs |
| TTL check | <10μs | ~2μs |
| Full routing decision | <1ms | ~200μs |

Reference hardware: Pixel 5 (Snapdragon 765G). The critical path must be faster than
this on all supported hardware, including budget Android phones (Snapdragon 460).
