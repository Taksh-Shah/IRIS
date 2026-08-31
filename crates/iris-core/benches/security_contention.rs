//! PRY-13: concurrency benchmark for the security seam.
//!
//! Every inbound message crosses `rate_limit` → `check_quota` → `check_replay`
//! → `score_spam` on `FullSecurityPolicy`. Before sharding, each of those took
//! one process-wide `RwLock` in write mode, so flooding a node serialised all
//! inbound processing. This bench runs N tokio tasks hammering one policy and
//! reports throughput for two shapes:
//!
//! - **distinct senders** — the shards spread the load; throughput should rise
//!   with worker count.
//! - **one hot sender** — all traffic hits a single shard; this is the
//!   worst case and a lower bound (still one lock, but only for that sender).
//!
//! Run: `cargo bench -p iris-core --bench security_contention`.

use std::sync::Arc;
use std::time::Instant;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use iris_core::message::{MessagePriority, MessagePriority as P};
use iris_core::protocol::content_type::ContentType;
use iris_core::protocol::message_id::MessageId;
use iris_core::protocol::Envelope;
use iris_core::security::{FullSecurityPolicy, MessageClass, SecurityPolicy};
use iris_core::identity::TrustStore;
use std::hint::black_box;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .enable_all()
        .build()
        .expect("runtime")
}

fn envelope(sender: [u8; 16]) -> Envelope {
    let mut sender_id = vec![0u8; 32];
    sender_id[..16].copy_from_slice(&sender);
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id,
        recipient_id: vec![],
        priority: P::P4,
        ttl_seconds: 3600,
        timestamp: iris_core::message_engine::expiry::unix_now(),
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Text,
        payload_size: 32,
        payload_hash: [0u8; 32],
        payload: vec![0u8; 32],
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

async fn inbound_check(policy: &FullSecurityPolicy, sender: [u8; 16], seq: u64) {
    let now = iris_core::message_engine::expiry::unix_now();
    black_box(policy.rate_limit(sender, MessageClass::P4).await);
    black_box(policy.check_quota(sender, 32, MessagePriority::P4).await);
    black_box(policy.check_replay(sender, now, seq).await);
    black_box(policy.score_spam(sender, &envelope(sender)).await);
}

fn bench(c: &mut Criterion) {
    let rt = runtime();
    let mut group = c.benchmark_group("security_contention");

    const TASKS: usize = 64;
    const PER_TASK: u64 = 200;
    group.throughput(Throughput::Elements(TASKS as u64 * PER_TASK));

    group.bench_function("distinct_senders", |b| {
        b.iter_batched(
            || Arc::new(FullSecurityPolicy::new(Arc::new(TrustStore::new()))),
            |policy| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(TASKS);
                    for t in 0..TASKS {
                        let p = policy.clone();
                        handles.push(tokio::spawn(async move {
                            for i in 0..PER_TASK {
                                let mut s = [0u8; 16];
                                s[0] = t as u8;
                                s[1..5].copy_from_slice(&(i as u32).to_le_bytes());
                                inbound_check(&p, s, i + 1).await;
                            }
                        }));
                    }
                    for h in handles {
                        h.await.unwrap();
                    }
                });
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("one_hot_sender", |b| {
        b.iter_batched(
            || Arc::new(FullSecurityPolicy::new(Arc::new(TrustStore::new()))),
            |policy| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(TASKS);
                    for _ in 0..TASKS {
                        let p = policy.clone();
                        handles.push(tokio::spawn(async move {
                            let s = [0xAAu8; 16];
                            for i in 0..PER_TASK {
                                inbound_check(&p, s, i + 1).await;
                            }
                        }));
                    }
                    for h in handles {
                        h.await.unwrap();
                    }
                });
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();

    // A quick non-criterion sanity print so `cargo bench` output shows the
    // headline ratio even without a full statistical run.
    let policy = Arc::new(FullSecurityPolicy::new(Arc::new(TrustStore::new())));
    let start = Instant::now();
    rt.block_on(async {
        let mut handles = Vec::new();
        for t in 0..TASKS {
            let p = policy.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..PER_TASK {
                    let mut s = [0u8; 16];
                    s[0] = t as u8;
                    s[1..5].copy_from_slice(&(i as u32).to_le_bytes());
                    inbound_check(&p, s, i + 1).await;
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
    });
    let elapsed = start.elapsed();
    eprintln!(
        "security_contention: {} checks in {:?} ({:.0}/s, {} shards)",
        TASKS as u64 * PER_TASK,
        elapsed,
        (TASKS as f64 * PER_TASK as f64) / elapsed.as_secs_f64(),
        64,
    );
}

criterion_group!(benches, bench);
criterion_main!(benches);
