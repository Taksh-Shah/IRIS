//! Shared helpers for PostgreSQL-backed integration tests.
//!
//! All PG-backed tests carry `#[ignore]` (TAK-1) and are skipped by default.
//! To run them: set `IRIS_PG_PASSWORD` (and optionally `IRIS_PG_HOST`/`_PORT`/
//! `_USER`/`_DB`) then invoke `cargo test -p iris-storage -- --include-ignored`.
//! Tests are serialized on a global lock so they share one scratch database.
//!
//! Without `#[ignore]`, a missing password would make every test silently pass
//! (`return;` after the `pg_available()` check), giving a false-green CI signal
//! while the entire storage safety surface remained unverified — see TAK-1.

use std::time::{SystemTime, UNIX_EPOCH};

use iris_core::message::MessagePriority;
use iris_core::protocol::{ContentType, Envelope, MessageId};
use iris_storage::{PgStorage, PgStorageConfig};

pub static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub const TEST_DB: &str = "iris_test";

/// True if a Postgres password was supplied via the environment.
pub fn pg_available() -> bool {
    std::env::var("IRIS_PG_PASSWORD")
        .map(|p| !p.is_empty())
        .unwrap_or(false)
}

fn cfg_db(dbname: &str) -> PgStorageConfig {
    let mut c = PgStorageConfig::from_env();
    c.dbname = dbname.to_string();
    c
}

/// Ensure the scratch database exists (created against the maintenance DB).
pub async fn ensure_db() {
    let cfg = cfg_db("postgres");
    let (client, conn) = tokio_postgres::connect(
        &format!(
            "host={} port={} user={} password={} dbname=postgres",
            cfg.host, cfg.port, cfg.user, cfg.password
        ),
        tokio_postgres::NoTls,
    )
    .await
    .expect("connect to maintenance db");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let create = format!("CREATE DATABASE {TEST_DB}");
    let _ = client.execute(&create, &[]).await; // ignore "already exists"
}

/// A fresh store connected to the scratch DB (schema applied, rows cleared).
pub async fn fresh_store() -> PgStorage {
    ensure_db().await;
    let store = PgStorage::connect(cfg_db(TEST_DB))
        .await
        .expect("connect storage");
    store
        .client()
        .await
        .expect("client")
        .execute("TRUNCATE TABLE messages", &[])
        .await
        .expect("truncate");
    store
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn env_for(
    recipient: [u8; 32],
    priority: MessagePriority,
    ts: u64,
    ttl: u64,
    payload: &[u8],
) -> Envelope {
    env_from([0xAA; 32], recipient, priority, ts, ttl, payload)
}

/// Like [`env_for`] but with an explicit sender (TAK-2): needed to construct
/// both a locally-originated and a relayed message against the same store
/// config, to distinguish `is_own_message` in eviction/quota tests.
pub fn env_from(
    sender: [u8; 32],
    recipient: [u8; 32],
    priority: MessagePriority,
    ts: u64,
    ttl: u64,
    payload: &[u8],
) -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: sender.to_vec(),
        recipient_id: recipient.to_vec(),
        priority,
        ttl_seconds: ttl,
        timestamp: ts,
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Text,
        payload_size: payload.len() as u64,
        payload_hash: Envelope::compute_payload_hash(payload),
        payload: payload.to_vec(),
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}
