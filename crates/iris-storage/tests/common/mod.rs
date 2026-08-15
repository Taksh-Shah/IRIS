//! Shared helpers for PostgreSQL-backed integration tests.
//!
//! All DB tests are gated on `IRIS_PG_PASSWORD` being set (the operator-supplied
//! credential is read from the environment, never committed to source). Tests
//! are serialized on a global lock so they share one scratch database safely.

use std::time::{SystemTime, UNIX_EPOCH};

use iris_core::message::MessagePriority;
use iris_core::protocol::{ContentType, Envelope, MessageId};
use iris_storage::{PgStorage, PgStorageConfig};

pub static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub const TEST_DB: &str = "iris_test";

/// True if a Postgres password was supplied via the environment.
pub fn pg_available() -> bool {
    std::env::var("IRIS_PG_PASSWORD").map(|p| !p.is_empty()).unwrap_or(false)
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
    tokio::spawn(async move { let _ = conn.await; });
    let create = format!("CREATE DATABASE {TEST_DB}");
    let _ = client.execute(&create, &[]).await; // ignore "already exists"
}

/// A fresh store connected to the scratch DB (schema applied, rows cleared).
pub async fn fresh_store() -> PgStorage {
    ensure_db().await;
    let store = PgStorage::connect(cfg_db(TEST_DB)).await.expect("connect storage");
    store
        .client()
        .execute("TRUNCATE TABLE messages", &[])
        .await
        .expect("truncate");
    store
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn env_for(recipient: [u8; 32], priority: MessagePriority, ts: u64, ttl: u64, payload: &[u8]) -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: vec![0xAA; 32],
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
