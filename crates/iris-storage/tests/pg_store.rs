//! STORE-001 acceptance tests — PostgreSQL backend against the MessageStorage seam.

mod common;

use common::*;
use iris_core::message::MessagePriority;
use iris_core::message_engine::storage::StorageError;
use iris_core::protocol::codec;
use iris_storage::{MessageStorage, PgStorage, PgStorageConfig, StorageKeySealer};

const PEER_BOB: [u8; 32] = [0xBB; 32];

async fn store_with_max(bytes: u64) -> PgStorage {
    ensure_db().await;
    let mut cfg = cfg_db(TEST_DB);
    cfg.max_storage_bytes = bytes;
    let store = PgStorage::connect(cfg).await.expect("connect storage");
    store.client().execute("TRUNCATE TABLE messages", &[]).await.expect("truncate");
    store
}

fn cfg_db(dbname: &str) -> PgStorageConfig {
    let mut c = PgStorageConfig::from_env();
    c.dbname = dbname.to_string();
    c
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persist_load_roundtrip() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let env = env_for(PEER_BOB, MessagePriority::P4, unix_now() - 100, 3600, b"hello pg");
    store.persist(&env).await.expect("persist");
    let loaded = store.load(&env.message_id).await.expect("load").expect("row exists");
    assert_eq!(loaded, env, "byte-exact round-trip via canonical CBOR");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn insert_or_ignore_dedups_at_storage_layer() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let env = env_for(PEER_BOB, MessagePriority::P3, unix_now() - 100, 3600, b"dup");
    store.persist(&env).await.expect("persist");
    store.persist(&env).await.expect("persist again"); // INSERT ... ON CONFLICT DO NOTHING
    let count: i64 = store.client().query_one("SELECT count(*) FROM messages", &[])
        .await.expect("count").get(0);
    assert_eq!(count, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_status_round_trips() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let env = env_for(PEER_BOB, MessagePriority::P4, unix_now() - 50, 3600, b"status");
    store.persist(&env).await.expect("persist");
    store
        .update_status(&env.message_id, iris_core::message_engine::lifecycle::MessageStatus::InTransit)
        .await
        .expect("update");
    let status: String = store.client()
        .query_one("SELECT status FROM messages WHERE message_id = $1", &[&env.message_id.to_string()])
        .await.expect("query").get(0);
    assert_eq!(status, "IN_TRANSIT");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_removes_row() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let env = env_for(PEER_BOB, MessagePriority::P4, unix_now() - 50, 3600, b"bye");
    store.persist(&env).await.expect("persist");
    store.delete(&env.message_id).await.expect("delete");
    assert!(store.load(&env.message_id).await.expect("load").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evict_expired_reclaims_only_dead_rows() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let now = unix_now();
    // TTL of 10 with timestamp 10 000 s in the past → expires_at already passed.
    let expired = env_for(PEER_BOB, MessagePriority::P4, now - 10_000, 10, b"old");
    let expired_p0 = env_for(PEER_BOB, MessagePriority::P0, now - 10_000, 10, b"old p0");
    let live = env_for(PEER_BOB, MessagePriority::P7, now - 30, 3600, b"fresh");
    store.persist(&expired).await.expect("persist expired");
    store.persist(&expired_p0).await.expect("persist expired p0");
    store.persist(&live).await.expect("persist live");
    let n = store.evict_expired(now).await.expect("evict");
    assert_eq!(n, 2, "expired + expired P0 reclaimed, live untouched");
    assert!(store.load(&expired.message_id).await.expect("load").is_none());
    assert!(store.load(&expired_p0.message_id).await.expect("load").is_none());
    assert!(store.load(&live.message_id).await.expect("load").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evict_by_priority_never_touches_p0() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let now = unix_now();
    let p0 = env_for(PEER_BOB, MessagePriority::P0, now - 30, 3600, b"sos");
    let p7 = env_for(PEER_BOB, MessagePriority::P7, now - 30, 3600, b"video");
    store.persist(&p0).await.expect("persist p0");
    store.persist(&p7).await.expect("persist p7");
    let evicted = store.evict_by_priority(0).await.expect("evict"); // target 0 → hard pressure
    assert_eq!(evicted, 1, "only the P7 is evicted");
    assert!(store.load(&p0.message_id).await.expect("load").is_some(), "INV-ROUTE-003: P0 never evicted");
    assert!(store.load(&p7.message_id).await.expect("load").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evict_by_priority_removes_lowest_priority_first() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let now = unix_now();
    // ~10 KiB payloads so a single row eviction is enough to fall under target.
    let big = vec![0x11u8; 10_000];
    let p2 = env_for(PEER_BOB, MessagePriority::P2, now - 30, 3600, &big);
    let p4 = env_for(PEER_BOB, MessagePriority::P4, now - 30, 3600, &big);
    store.persist(&p2).await.expect("persist p2");
    store.persist(&p4).await.expect("persist p4");
    // Target between one and two rows of usage → only the lowest priority (P4) is evicted.
    let target = (p4.payload.len() as u64 + 2000) as u64;
    let _ = store.evict_by_priority(target).await.expect("evict");
    assert!(store.load(&p4.message_id).await.expect("load").is_none(), "P4 evicted first");
    assert!(store.load(&p2.message_id).await.expect("load").is_some(), "P2 survives under target");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_bytes_counts_stored_rows() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let before = store.usage_bytes().await.expect("usage");
    let now = unix_now();
    let env = env_for(PEER_BOB, MessagePriority::P4, now - 30, 3600, b"payload-1m");
    store.persist(&env).await.expect("persist");
    let after = store.usage_bytes().await.expect("usage");
    assert!(after > before, "usage grows with stored rows");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_queue_orders_p0_first_and_filters_terminal() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let now = unix_now();
    let p4 = env_for(PEER_BOB, MessagePriority::P4, now - 60, 3600, b"txt");
    let p0 = env_for(PEER_BOB, MessagePriority::P0, now - 30, 3600, b"sos");
    store.persist(&p4).await.expect("persist p4");
    store.persist(&p0).await.expect("persist p0");
    // Mark the P4 delivered → excluded from the queue.
    store.update_status(&p4.message_id, iris_core::message_engine::lifecycle::MessageStatus::Delivered)
        .await.expect("update");
    let q = store.get_queue(10).await.expect("queue");
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].message_id, p0.message_id, "P0 first and delivered excluded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quota_rejects_non_p0_but_accepts_p0() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    // Tiny quota that one message exceeds.
    let store = store_with_max(128).await;
    let now = unix_now();
    let normal = env_for(PEER_BOB, MessagePriority::P4, now - 30, 3600, &vec![0x42; 512]);
    let res = store.persist(&normal).await;
    assert_eq!(res, Err(StorageError::StorageFull), "non-P0 over quota refused");
    let sos = env_for(PEER_BOB, MessagePriority::P0, now - 30, 3600, b"sos-small");
    assert!(store.persist(&sos).await.is_ok(), "P0 always accepted (INV-EMERG-001 spirit)");
}

// --- At-rest sealing (CRYPTO-001 gate, AC-7) --------------------------------

/// Persist with the storage-key sealer, then prove the stored blob is NOT the
/// plaintext CBOR envelope and that read paths unseal to the byte-exact original.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sealed_rows_are_not_plaintext_and_round_trip() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let mut cfg = cfg_db(TEST_DB);
    cfg.max_storage_bytes = 0;
    let sealer = StorageKeySealer::from_master_key(b"node-master-key-0x00").expect("sealer");
    let store = PgStorage::connect(cfg)
        .await
        .expect("connect storage")
        .with_sealer(std::sync::Arc::new(sealer));
    store.client().execute("TRUNCATE TABLE messages", &[]).await.expect("truncate");

    let env = env_for(PEER_BOB, MessagePriority::P4, unix_now() - 100, 3600, b"at-rest secret");
    store.persist(&env).await.expect("persist sealed");

    // 1. The stored blob must not be decodable as CBOR (ciphertext at rest).
    let stored: Vec<u8> = store
        .client()
        .query_one("SELECT envelope_cbor FROM messages WHERE message_id = $1", &[&env.message_id.to_string()])
        .await
        .expect("row")
        .get(0);
    assert!(codec::decode(&stored).is_err(), "envelope_cbor must be sealed, not plaintext CBOR");
    assert_ne!(stored, codec::encode(&env).expect("encode"));

    // 2. load() unseals to the byte-exact original.
    let loaded = store.load(&env.message_id).await.expect("load").expect("row exists");
    assert_eq!(loaded, env, "byte-exact round-trip through the sealer");

    // 3. get_queue() returns the unsealed envelope for the sender queue.
    let q = store.get_queue(10).await.expect("queue");
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].message_id, env.message_id);
}

/// A store sealed with the wrong key must fail to read its own rows
/// (DecryptionFailed), proving tamper/rotation detection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sealed_rows_require_the_right_key() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let mut cfg = cfg_db(TEST_DB);
    cfg.max_storage_bytes = 0;
    let sealer = StorageKeySealer::from_master_key(b"node-master-key-0x00").expect("sealer");
    let store = PgStorage::connect(cfg)
        .await
        .expect("connect storage")
        .with_sealer(std::sync::Arc::new(sealer));
    store.client().execute("TRUNCATE TABLE messages", &[]).await.expect("truncate");
    let env = env_for(PEER_BOB, MessagePriority::P4, unix_now() - 100, 3600, b"at-rest secret");
    store.persist(&env).await.expect("persist sealed");

    // Reopen with a different master key → read must fail, not return garbage.
    let wrong = PgStorage::connect(cfg_db(TEST_DB))
        .await
        .expect("connect")
        .with_sealer(std::sync::Arc::new(
            StorageKeySealer::from_master_key(b"wrong-master-key-0x00").expect("sealer"),
        ));
    let res = wrong.load(&env.message_id).await;
    assert_eq!(res, Err(StorageError::DecryptionFailed), "wrong storage key must fail loudly");
}

/// CRYPTO-001: after connect, no plaintext sender/recipient identity columns
/// or recipient index remain in the schema (metadata not inferable at rest).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plaintext_identity_columns_are_removed_from_schema() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;
    let store = fresh_store().await;
    let cols: Vec<String> = store
        .client()
        .query(
            "SELECT column_name FROM information_schema.columns
             WHERE table_name = 'messages'",
            &[],
        )
        .await
        .expect("columns")
        .iter()
        .map(|r| r.get::<_, String>(0))
        .collect();
    assert!(!cols.iter().any(|c| c == "sender_id"), "sender_id must not exist (plaintext metadata)");
    assert!(!cols.iter().any(|c| c == "recipient_id"), "recipient_id must not exist (plaintext metadata)");

    let idxs: i64 = store
        .client()
        .query_one(
            "SELECT count(*) FROM pg_indexes WHERE tablename = 'messages' AND indexname = 'idx_messages_recipient'",
            &[],
        )
        .await
        .expect("index")
        .get(0);
    assert_eq!(idxs, 0, "idx_messages_recipient must be dropped");
}