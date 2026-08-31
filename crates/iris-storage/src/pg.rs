//! PostgreSQL implementation of the [`MessageStorage`] seam — STORE-001.
//!
//! A single multiplexed `tokio_postgres::Client` (wrapped in an `Arc`) serves
//! concurrent async queries; the engine talks to this backend through the
//! seam trait. Connection parameters come from [`PgStorageConfig`], with the
//! password read from `IRIS_PG_PASSWORD` (never hardcoded / committed).

use std::sync::Arc;
use std::time::Duration;

use iris_core::message::MessagePriority;
use iris_core::message_engine::lifecycle::MessageStatus;
use iris_core::message_engine::storage::{MessageStorage, StorageError};
use iris_core::protocol::{codec, Envelope, MessageId};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::eviction::{delete_expired, evict_lowest_priority, usage_bytes};
use crate::seal::{NoSealer, RowSealer};

/// Whether TLS is required for the PostgreSQL connection (TAK-6).
///
/// Defaults to `Require`. `Disable` is permitted only when the configured
/// host resolves to loopback; attempting to connect to a remote host without
/// TLS returns `StorageError::Backend` at startup rather than leaking the
/// DB password and message CBOR in cleartext.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SslMode {
    /// Enforce TLS (default). The connector uses the platform's native root
    /// CA bundle via webpki-roots to verify the server certificate.
    Require,
    /// Disable TLS. Only permitted for loopback hosts (`127.0.0.1`, `::1`,
    /// `localhost`). Any other host causes `connect` to fail immediately.
    Disable,
}

/// True when `host` is unambiguously loopback — TLS overhead is unnecessary
/// and server-cert verification against a loopback address is meaningless.
fn host_is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// Build a rustls `MakeRustlsConnect` backed by the bundled Mozilla root set.
fn make_tls_connector() -> Result<MakeRustlsConnect, StorageError> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(MakeRustlsConnect::new(tls_config))
}

/// Connection + quota configuration for [`PgStorage`].
///
/// `Debug` is hand-implemented to redact `password` (TAK-16): the derived form
/// printed the literal secret, so any `tracing::debug!(?config, ...)` or
/// panic payload containing the config wrote `IRIS_PG_PASSWORD` to the logs.
#[derive(Clone)]
pub struct PgStorageConfig {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub user: String,
    /// The password. `from_env()` fills this from `IRIS_PG_PASSWORD`.
    pub password: String,
    /// Storage quota in bytes (0 disables quota enforcement).
    pub max_storage_bytes: u64,
    /// Fraction of quota that triggers eviction (STORAGE.md: 0.8).
    pub eviction_threshold: f64,
    /// This node's identity, when known (TAK-10). When set, `persist` derives
    /// the `is_own_message` column by comparing the envelope sender to it, so
    /// the documented eviction policy ("relayed before own") actually has a
    /// live key. `None` keeps every row marked relayed — the pre-TAK-10
    /// behaviour — and is what `from_env` produces until `IRIS_NODE_ID` is
    /// provided. Deliberately carried in config rather than widening the
    /// Section-2-owned `MessageStorage::persist` signature.
    pub node_id: Option<[u8; 32]>,
    /// TAK-6: TLS mode for the PostgreSQL connection. Defaults to `Require`;
    /// set via `IRIS_PG_SSLMODE=disable` for local / loopback-only deployments.
    pub ssl_mode: SslMode,
}

impl std::fmt::Debug for PgStorageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgStorageConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("dbname", &self.dbname)
            .field("user", &self.user)
            .field("password", &"[redacted]")
            .field("max_storage_bytes", &self.max_storage_bytes)
            .field("eviction_threshold", &self.eviction_threshold)
            .field("ssl_mode", &self.ssl_mode)
            .finish()
    }
}

impl PgStorageConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("IRIS_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("IRIS_PG_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5432),
            dbname: std::env::var("IRIS_PG_DB").unwrap_or_else(|_| "iris".into()),
            user: std::env::var("IRIS_PG_USER").unwrap_or_else(|_| "postgres".into()),
            password: std::env::var("IRIS_PG_PASSWORD").unwrap_or_default(),
            max_storage_bytes: std::env::var("IRIS_MAX_STORAGE_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500 * 1024 * 1024),
            eviction_threshold: parse_eviction_threshold(
                std::env::var("IRIS_EVICTION_THRESHOLD").ok(),
            ),
            node_id: std::env::var("IRIS_NODE_ID")
                .ok()
                .as_deref()
                .and_then(parse_node_id),
            ssl_mode: match std::env::var("IRIS_PG_SSLMODE").as_deref() {
                Ok("disable") => SslMode::Disable,
                _ => SslMode::Require,
            },
        }
    }
}

/// Parse a 64-hex-char `IRIS_NODE_ID` into the 32-byte peer identity
/// (TAK-10). Absent or malformed values yield `None` (rows stay marked
/// relayed) rather than a wrong identity.
fn parse_node_id(raw: &str) -> Option<[u8; 32]> {
    let hex = raw.trim();
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)? as u8;
        let lo = (chunk[1] as char).to_digit(16)? as u8;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Parse and range-validate `IRIS_EVICTION_THRESHOLD` (TAK-25). Anything
/// absent, unparseable, or outside `0.1..=0.95` falls back to the documented
/// default of `0.8`, so a misconfiguration can never disable eviction
/// (`>= 1.0`) or trigger it on every write (`<= 0.0`).
fn parse_eviction_threshold(raw: Option<String>) -> f64 {
    raw.and_then(|v| v.parse::<f64>().ok())
        .filter(|v| (0.1..=0.95).contains(v))
        .unwrap_or(0.8)
}

/// PostgreSQL message store.
///
/// TAK-4: the connection is supervised. The live [`tokio_postgres::Client`]
/// lives behind `client_slot`; when the connection driver dies the supervisor
/// clears the slot and drives a jittered-exponential reconnect loop
/// (re-applying schema migrations on success), so a routine database restart
/// degrades into transient errors instead of bricking persistence for the
/// process lifetime. Methods surface a distinct "storage unavailable" error
/// while offline rather than a stream of confusing per-query failures.
pub struct PgStorage {
    client_slot: Arc<tokio::sync::RwLock<Option<Arc<tokio_postgres::Client>>>>,
    config: PgStorageConfig,
    /// At-rest row sealing (CRYPTO-001 gate). `NoSealer` by default.
    sealer: Arc<dyn RowSealer>,
    /// Rows quarantined by [`PgStorage::get_queue`] because they could not be
    /// unsealed or decoded (TAK-3). Observable via [`PgStorage::quarantined_rows`].
    quarantined: std::sync::atomic::AtomicU64,
}

/// Schema statements applied on every (re)connection, in order.
const CONNECTION_MIGRATIONS: &[&str] = &[
    crate::schema::DDL,
    crate::schema::MIGRATE_DROP_PLAINTEXT_IDENTITY,
    crate::schema::MIGRATE_NONNEGATIVE_CONSTRAINT,
    crate::schema::MIGRATE_FIX_PARTIAL_INDEXES,
];

const RECONNECT_INITIAL_MS: u64 = 1_000;
const RECONNECT_MAX_MS: u64 = 30_000;

/// Deterministic exponential progression: 1s → 2s → 4s … capped at 30s.
fn doubled_capped(current_ms: u64) -> u64 {
    current_ms.saturating_mul(2).min(RECONNECT_MAX_MS)
}

/// Apply up to ±20% deterministic jitter derived from wall-clock sub-second
/// nanos, so a fleet of nodes does not reconnect in lockstep.
fn apply_jitter(ms: u64) -> u64 {
    let spread = ms / 5;
    if spread == 0 {
        return ms;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    ms - spread + (nanos % (2 * spread + 1))
}

fn initial_backoff_ms() -> u64 {
    RECONNECT_INITIAL_MS
}

fn next_backoff_ms(current_ms: u64) -> u64 {
    apply_jitter(doubled_capped(current_ms))
}

/// Establish one connection, spawn its driver task internally, and apply
/// every schema migration. Used both for the initial `connect` and by the
/// supervisor on every reconnect, so a new connection is always fully
/// migrated before it becomes visible to callers.
///
/// TAK-6: when `ssl_mode` is `Require` the connection uses rustls; when it
/// is `Disable` the caller must have already verified the host is loopback.
async fn establish(
    pg: &tokio_postgres::Config,
    host: &str,
    ssl_mode: &SslMode,
) -> Result<Arc<tokio_postgres::Client>, StorageError> {
    // TAK-6: refuse non-loopback connections with TLS disabled.
    if *ssl_mode == SslMode::Disable && !host_is_loopback(host) {
        return Err(StorageError::Backend(
            "TLS required for non-loopback Postgres — set IRIS_PG_SSLMODE=require or use a loopback host".into(),
        ));
    }
    let (client, connection) = if *ssl_mode == SslMode::Require {
        let tls = make_tls_connector()?;
        pg.connect(tls)
            .await
            .map_err(|e| StorageError::Backend(format!("pg connect (tls): {e}")))?
    } else {
        pg.connect(tokio_postgres::NoTls)
            .await
            .map_err(|e| StorageError::Backend(format!("pg connect: {e}")))?
    };
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            tracing::debug!("pg connection driver ended: {e}");
        }
    });
    for statement in CONNECTION_MIGRATIONS {
        client
            .batch_execute(statement)
            .await
            .map_err(|e| StorageError::Backend(format!("pg migrate: {e}")))?;
    }
    Ok(Arc::new(client))
}

/// How often the supervisor probes the installed connection. A probe failure
/// (or a dead driver) clears the slot and starts reconnecting.
const HEALTH_PROBE_SECS: u64 = 5;

async fn connection_is_alive(client: &tokio_postgres::Client) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(3), client.simple_query("SELECT 1")).await,
        Ok(Ok(_))
    )
}

/// Drive reconnects for the lifetime of the process (TAK-4): probe the
/// installed connection on a short interval, clear the slot the moment it is
/// unusable, and re-dial with jittered exponential backoff — re-applying all
/// migrations on every fresh connection.
fn spawn_connection_supervisor(
    pg: tokio_postgres::Config,
    host: String,
    ssl_mode: SslMode,
    client_slot: Arc<tokio::sync::RwLock<Option<Arc<tokio_postgres::Client>>>>,
) {
    tokio::spawn(async move {
        let mut backoff_ms = initial_backoff_ms();
        loop {
            // The INITIAL connection is established inline by `connect` — the
            // supervisor only takes over once that one has died and cleared
            // the slot, so two live connections never coexist.
            while client_slot.read().await.is_some() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            match establish(&pg, &host, &ssl_mode).await {
                Ok(client) => {
                    backoff_ms = initial_backoff_ms();
                    tracing::info!("pg connected");
                    *client_slot.write().await = Some(client.clone());
                    // Supervise: poll until this connection stops answering,
                    // then evict it so callers see "unavailable" immediately.
                    while connection_is_alive(&client).await {
                        tokio::time::sleep(Duration::from_secs(HEALTH_PROBE_SECS)).await;
                        let still_installed = matches!(
                            &*client_slot.read().await,
                            Some(installed) if Arc::ptr_eq(installed, &client)
                        );
                        if !still_installed {
                            break; // replaced or cleared elsewhere; stand down
                        }
                    }
                    let mut guard = client_slot.write().await;
                    if matches!(&*guard, Some(installed) if Arc::ptr_eq(installed, &client)) {
                        tracing::warn!("pg connection lost; supervisor will reconnect");
                        *guard = None;
                    }
                    drop(guard);
                }
                Err(e) => {
                    tracing::error!(error = %e, backoff_ms, "pg reconnect failed");
                }
            }
            tokio::time::sleep(Duration::from_millis(next_backoff_ms(backoff_ms))).await;
            backoff_ms = next_backoff_ms(backoff_ms);
        }
    });
}

fn status_to_text(s: MessageStatus) -> &'static str {
    match s {
        MessageStatus::Created | MessageStatus::PendingSend => "PENDING_SEND",
        MessageStatus::InTransit => "IN_TRANSIT",
        MessageStatus::Delivered => "DELIVERED",
        MessageStatus::Acknowledged => "ACKNOWLEDGED",
        MessageStatus::Expired => "EXPIRED",
        MessageStatus::DeliveryFailed => "DELIVERY_FAILED",
    }
}

fn backend(e: tokio_postgres::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

/// Free-function core of [`PgStorage::decode_stored_blob`] so the dispatch is
/// unit-testable without a database (TAK-15).
///
/// Interpretations are probed in priority order and each candidate is
/// VALIDATED before acceptance (plaintext by successful CBOR decode; sealed
/// by AEAD authentication), so a legacy blob whose first byte coincides with
/// a tag value cannot be misrouted — its tagged reading fails validation and
/// the legacy reading still resolves. Sealed candidates are tried against
/// every supplied AAD in order: current-format first, then historical
/// formats (TAK-14 v1 fallback).
pub(crate) fn decode_stored_blob_with(
    sealer: &dyn crate::seal::RowSealer,
    aads: &[Vec<u8>],
    blob: &[u8],
) -> Result<Vec<u8>, StorageError> {
    let try_plain = |payload: &[u8]| codec::decode(payload).map(|_| payload.to_vec());
    let try_sealed = |payload: &[u8]| {
        for aad in aads {
            if let Ok(cbor) = sealer.unseal(aad, payload) {
                return Ok(cbor);
            }
        }
        Err(StorageError::DecryptionFailed)
    };

    match crate::seal::split_format_tag(blob) {
        Some((tag, rest)) if tag == crate::seal::envelope_format::PLAINTEXT_CBOR => {
            if let Ok(cbor) = try_plain(rest) {
                return Ok(cbor);
            }
            try_legacy(sealer, aads, blob)
        }
        Some((tag, rest)) if tag == crate::seal::envelope_format::SEALED => {
            if let Ok(cbor) = try_sealed(rest) {
                return Ok(cbor);
            }
            try_legacy(sealer, aads, blob)
        }
        // First byte is neither tag: only the legacy reading exists.
        _ => try_legacy(sealer, aads, blob),
    }
}

/// Untagged-era rows: verbatim plaintext CBOR, else a raw sealed container
/// under any known AAD generation (TAK-14 fallback).
fn try_legacy(
    sealer: &dyn crate::seal::RowSealer,
    aads: &[Vec<u8>],
    blob: &[u8],
) -> Result<Vec<u8>, StorageError> {
    match codec::decode(blob) {
        Ok(_) => Ok(blob.to_vec()),
        Err(_) => {
            for aad in aads {
                if let Ok(cbor) = sealer.unseal(aad, blob) {
                    return Ok(cbor);
                }
            }
            Err(StorageError::DecryptionFailed)
        }
    }
}

impl PgStorage {
    /// Connect, spawn the supervised connection driver, and apply the schema.
    pub async fn connect(config: PgStorageConfig) -> Result<Self, StorageError> {
        let mut pg = tokio_postgres::Config::new();
        pg.host(&config.host)
            .port(config.port)
            .dbname(&config.dbname)
            .user(&config.user)
            .password(&config.password);

        // Initial establishment happens inline so callers see real connection
        // errors at startup; afterwards the supervisor owns reconnection.
        let client = establish(&pg, &config.host, &config.ssl_mode).await.map_err(|e| {
            tracing::error!(error = %e, "initial pg connection failed");
            e
        })?;

        let client_slot = Arc::new(tokio::sync::RwLock::new(Some(client)));
        spawn_connection_supervisor(pg, config.host.clone(), config.ssl_mode.clone(), client_slot.clone());

        Ok(Self {
            client_slot,
            config,
            sealer: Arc::new(NoSealer),
            quarantined: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Connect using environment configuration.
    pub async fn connect_from_env() -> Result<Self, StorageError> {
        Self::connect(PgStorageConfig::from_env()).await
    }

    /// Replace the at-rest row sealer (CRYPTO-001). Call after `connect`.
    pub fn with_sealer(mut self, sealer: Arc<dyn RowSealer>) -> Self {
        self.sealer = sealer;
        self
    }

    /// Run one GC tick: reclaim expired rows, then evict by priority if usage
    /// exceeds the quota threshold (STORAGE.md). P0 never evicted while live.
    pub async fn gc_once(&self, now_unix: u64) -> Result<(), StorageError> {
        let client = self.live_client().await?;
        let _expired = delete_expired(&client, now_unix).await.map_err(backend)?;
        if self.config.max_storage_bytes > 0 {
            let target =
                (self.config.max_storage_bytes as f64 * self.config.eviction_threshold) as u64;
            if usage_bytes(&client).await.map_err(backend)? > target {
                let _ = evict_lowest_priority(&client, target)
                    .await
                    .map_err(backend)?;
            }
        }
        Ok(())
    }

    /// Current live client, or the distinct "storage unavailable" error
    /// (TAK-4) while the supervisor is reconnecting. Callers treat this as
    /// retry-later rather than as bad data.
    async fn live_client(&self) -> Result<Arc<tokio_postgres::Client>, StorageError> {
        self.client_slot
            .read()
            .await
            .clone()
            .ok_or_else(|| StorageError::Backend("storage unavailable: reconnecting".into()))
    }

    /// Whether a connection is currently installed and responsive (TAK-4):
    /// runs a trivial round-trip so "slot filled" means genuinely usable.
    /// The engine can poll this to degrade deliberately during outages.
    pub async fn is_healthy(&self) -> bool {
        match self.live_client().await {
            Ok(client) => matches!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    client.simple_query("SELECT 1")
                )
                .await,
                Ok(Ok(_))
            ),
            Err(_) => false,
        }
    }

    /// Access the raw client for maintenance/administration.
    ///
    /// TAK-4: now falls through to [`PgStorage::live_client`], so it returns
    /// the same "unavailable" error instead of handing out a dead handle.
    pub async fn client(&self) -> Result<Arc<tokio_postgres::Client>, StorageError> {
        self.live_client().await
    }

    /// Rows quarantined by `get_queue` so far (TAK-3): unreadable rows are
    /// skipped, marked `DELIVERY_FAILED` and counted instead of bricking the
    /// send queue. A sudden rise equal to the whole queue indicates a
    /// systemic cause — most likely a sealing-key mismatch — and should page
    /// an operator before more traffic is accepted.
    pub fn quarantined_rows(&self) -> u64 {
        self.quarantined.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Move a row out of the send queue after an unrecoverable read failure:
    /// `status -> DELIVERY_FAILED` leaves the `get_queue` predicate and makes
    /// the row evictable. Failures to quarantine itself are logged and do not
    /// propagate into the drain path.
    async fn quarantine_row(&self, message_id: &str) {
        let Ok(client) = self.live_client().await else {
            return;
        };
        if let Err(e) = client
            .execute(
                "UPDATE messages SET status = 'DELIVERY_FAILED' WHERE message_id = $1",
                &[&message_id],
            )
            .await
        {
            tracing::error!(message_id = %message_id, error = %e, "quarantine update failed");
        }
    }

    /// Recover canonical CBOR from a stored blob (TAK-15). Tagged rows
    /// dispatch deterministically; untagged legacy rows (pre-format) fall
    /// back to plaintext-then-sealed probing so enabling sealing can never
    /// strand an existing plaintext store. Genuine failures surface to the
    /// caller, which quarantines per TAK-3.
    /// AAD candidates in try-order for a stored row: current v2 format first,
    /// then historical v1 (TAK-14).
    #[allow(clippy::too_many_arguments)]
    fn aad_candidates(
        &self,
        id: &str,
        priority: i16,
        expires_at: i64,
        status: &str,
        created_at: i64,
        hop_count: i16,
        max_hops: Option<i16>,
        payload_size: i64,
        is_own_message: bool,
    ) -> Vec<Vec<u8>> {
        vec![
            crate::seal::row_aad_v2(
                id,
                priority as u8,
                expires_at as u64,
                status,
                created_at,
                hop_count,
                max_hops,
                payload_size,
                is_own_message,
            ),
            crate::seal::row_aad(id, priority as u8, expires_at as u64),
        ]
    }

    fn decode_stored_blob(&self, aads: Vec<Vec<u8>>, blob: &[u8]) -> Result<Vec<u8>, StorageError> {
        decode_stored_blob_with(self.sealer.as_ref(), &aads, blob)
    }

    /// One-shot migration to the current row format (TAK-15 + TAK-14):
    /// rewrites every plaintext-tagged or untagged legacy plaintext row into a
    /// SEALED, explicitly tagged row under the v2 AAD (full-scalar
    /// authentication), using each row's live status column. A no-op for
    /// `NoSealer`. Intended to run once after `with_sealer` on upgrade; reads
    /// remain compatible with all historical shapes either way.
    pub async fn migrate_envelope_format(&self) -> Result<usize, StorageError> {
        if self.sealer.is_identity() {
            return Ok(0);
        }
        let client = self.live_client().await?;
        let rows = client
            .query(
                "SELECT message_id, priority, expires_at, envelope_cbor, status,
                        created_at, hop_count, max_hops, payload_size, is_own_message
                 FROM messages",
                &[],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("migrate scan: {e}")))?;
        let mut migrated = 0usize;
        for r in rows {
            let id: String = r.get(0);
            let priority: i16 = r.get(1);
            let expires_at: i64 = r.get(2);
            let blob: Vec<u8> = r.get(3);
            let status: String = r.get(4);
            let created_at: i64 = r.get(5);
            let hop_count: i16 = r.get(6);
            let max_hops: Option<i16> = r.get(7);
            let payload_size: i64 = r.get(8);
            let is_own: bool = r.get(9);
            let already_sealed = matches!(
                crate::seal::split_format_tag(&blob),
                Some((tag, _)) if tag == crate::seal::envelope_format::SEALED
            );
            if already_sealed {
                continue;
            }
            let cbor = self.decode_stored_blob(
                self.aad_candidates(
                    &id,
                    priority,
                    expires_at,
                    &status,
                    created_at,
                    hop_count,
                    max_hops,
                    payload_size,
                    is_own,
                ),
                &blob,
            )?;
            let sealed = self.sealer.seal(
                &crate::seal::row_aad_v2(
                    &id,
                    priority as u8,
                    expires_at as u64,
                    &status,
                    created_at,
                    hop_count,
                    max_hops,
                    payload_size,
                    is_own,
                ),
                &cbor,
            )?;
            let mut tagged = Vec::with_capacity(1 + sealed.len());
            tagged.push(crate::seal::envelope_format::SEALED);
            tagged.extend_from_slice(&sealed);
            client
                .execute(
                    "UPDATE messages SET envelope_cbor = $2 WHERE message_id = $1",
                    &[&id, &tagged],
                )
                .await
                .map_err(|e| StorageError::Backend(format!("migrate rewrite {id}: {e}")))?;
            migrated += 1;
        }
        Ok(migrated)
    }

    fn projected_row_bytes(cbor_len: u64) -> u64 {
        cbor_len + crate::eviction::ROW_OVERHEAD_BYTES
    }
}

#[async_trait::async_trait]
impl MessageStorage for PgStorage {
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError> {
        let client = self.live_client().await?;
        // TAK-9: same-width `as` casts silently reinterpret bits, letting a
        // peer-chosen u64 timestamp/expiry/payload_size land as a NEGATIVE
        // BIGINT (instant-expiring or immortal rows, corrupted ordering).
        // Reject out-of-domain values at the boundary instead.
        let created_at = i64::try_from(envelope.timestamp)
            .map_err(|_| StorageError::Backend("timestamp out of i64 range".into()))?;
        // TAK-2: clamp the claimed TTL before computing expiry — an
        // attacker-controlled `ttl_seconds` (up to u64::MAX) would otherwise
        // make a row immortal, permanently unreclaimable by `delete_expired`.
        // Shares `MAX_TTL_SECS` with the in-memory SCF store
        // (`iris_core::routing::scf`) so both storage layers agree on the
        // same ceiling, per this finding's own dependency note.
        // Deliberately does NOT also reject an already-expired `expires_at`
        // at ingest (the finding's fix text suggests this too) — a message
        // that is already expired the moment it arrives is the *opposite*
        // of TAK-2's actual attack (an immortal, never-reclaimable row): it
        // is immediately reclaimable by the very next `delete_expired`
        // pass, and `evict_expired_reclaims_only_dead_rows` (this crate's
        // own test suite) deliberately persists an already-expired message
        // to prove exactly that GC path. Rejecting it here would conflict
        // with that established, tested behavior for no corresponding
        // security benefit.
        let clamped_ttl = envelope
            .ttl_seconds
            .min(iris_core::routing::scf::MAX_TTL_SECS);
        let expires_u64 = envelope.timestamp.saturating_add(clamped_ttl);
        let expires_at = i64::try_from(expires_u64)
            .map_err(|_| StorageError::Backend("expiry out of i64 range".into()))?;
        let payload_size = i64::try_from(envelope.payload_size)
            .map_err(|_| StorageError::Backend("payload_size out of i64 range".into()))?;

        let cbor = codec::encode(envelope).map_err(|e| StorageError::Backend(e.to_string()))?;
        // TAK-10: real ownership when the node identity is configured — the
        // eviction policy's "relayed before own" tie-break needs a live key.
        // Malformed sender vectors simply stay marked relayed.
        let is_own = match (
            &self.config.node_id,
            <[u8; 32]>::try_from(envelope.sender_id.as_slice()),
        ) {
            (Some(node_id), Ok(sender)) => *node_id == sender,
            _ => false,
        };
        // TAK-14: rows are sealed under the v2 AAD, which authenticates every
        // scalar column including status (fresh rows always start PENDING_SEND).
        let aad = crate::seal::row_aad_v2(
            &envelope.message_id.to_string(),
            envelope.priority as u8,
            expires_u64,
            "PENDING_SEND",
            created_at,
            envelope.hop_count as i16,
            envelope.max_hops.map(|h| h as i16),
            payload_size,
            is_own,
        );
        // TAK-15: every row written carries an explicit 1-byte format tag so
        // reads never guess the blob's nature.
        let stored_body = self
            .sealer
            .seal(&aad, &cbor)
            .map_err(|e| StorageError::Backend(format!("seal: {e}")))?;
        let mut stored = Vec::with_capacity(1 + stored_body.len());
        if matches!(self.sealer.as_ref(), sealer if sealer.is_identity()) {
            stored.push(crate::seal::envelope_format::PLAINTEXT_CBOR);
        } else {
            stored.push(crate::seal::envelope_format::SEALED);
        }
        stored.extend_from_slice(&stored_body);

        // Duplicate suppression keeps its old semantics (a repeated persist
        // of the same id is Ok): check first so a 0-row-affected result from
        // the conditional insert below can only mean quota refusal (or a
        // benign concurrent duplicate, noted below).
        let dup: Option<i64> = client
            .query_opt(
                "SELECT 1 FROM messages WHERE message_id = $1",
                &[&envelope.message_id.to_string()],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("persist dedup: {e}")))?
            .map(|_| 1);
        if dup.is_some() {
            return Ok(());
        }

        let is_p0 = envelope.priority == MessagePriority::P0;
        // TAK-2: the quota bypass exempted every P0 message, own or
        // relayed — a remote peer could flood P0 traffic (bypassing quota
        // on ingest) that `evict_lowest_priority` then also hard-excluded
        // from reclamation (`WHERE priority > 0`), permanently exhausting
        // the store for a value that later persist() calls can never make
        // room for. INV-ROUTE-003 ("never evict P0") is about *this
        // node's own* emergency traffic, not an unauthenticated peer's
        // claim to be P0 — narrow the exemption to match.
        let own_p0_exempt = is_own && is_p0;
        let grow = Self::projected_row_bytes(stored.len() as u64);

        // TAK-5: the old read-check-insert sequence was a cross-store TOCTOU
        // (twenty concurrent writers all observed pre-insert usage and all
        // admitted) and never reclaimed anything on refusal. The admission
        // decision now happens ATOMICALLY inside one statement — usage is
        // summed server-side in the same snapshot that evaluates the guard —
        // and a refusal triggers exactly one eviction pass + retry before
        // StorageFull surfaces, instead of failing writes while reclaimable
        // rows sit idle until the next GC tick.
        const ADMISSION_SQL: &str = "INSERT INTO messages
                    (message_id, priority, status, created_at,
                     expires_at, hop_count, max_hops, payload_size, envelope_cbor, is_own_message)
                 SELECT $1,$2,'PENDING_SEND',$3,$4,$5,$6,$7,$8,$9
                 WHERE $10::bool OR (
                     SELECT COALESCE(SUM(octet_length(envelope_cbor) + $11::bigint), 0)::bigint
                     FROM messages
                 ) + $12::bigint <= $13::bigint
                 ON CONFLICT (message_id) DO NOTHING";

        for attempt in 0..2 {
            let admitted = client
                .execute(
                    ADMISSION_SQL,
                    &[
                        &envelope.message_id.to_string(),
                        &(envelope.priority as i16),
                        &created_at,
                        &expires_at,
                        &(envelope.hop_count as i16),
                        &envelope.max_hops.map(|h| h as i16),
                        &payload_size,
                        &stored,
                        &is_own,
                        &own_p0_exempt,
                        &(crate::eviction::ROW_OVERHEAD_BYTES as i64),
                        &(grow as i64),
                        &(self.config.max_storage_bytes as i64),
                    ],
                )
                .await
                .map_err(|e| StorageError::Backend(format!("persist: {e}")))?;
            if admitted > 0 {
                return Ok(());
            }
            // Refused: try to make room once (relayed P0 is now evictable
            // — TAK-2 — so a store genuinely full of nothing but this
            // node's own P0 is the only residue that evicts nothing and
            // falls through to StorageFull on the second pass), then retry
            // the atomic admission against the post-eviction usage.
            if attempt == 0 && self.config.max_storage_bytes > 0 {
                let target =
                    (self.config.max_storage_bytes as f64 * self.config.eviction_threshold) as u64;
                evict_lowest_priority(&client, target)
                    .await
                    .map_err(backend)?;
                continue;
            }
            break;
        }
        Err(StorageError::StorageFull)
    }

    async fn load(&self, id: &MessageId) -> Result<Option<Envelope>, StorageError> {
        let client = self.live_client().await?;
        let id_hex = id.to_string();
        let row = client
            .query_opt(
                "SELECT envelope_cbor, priority, expires_at, status, created_at,
                        hop_count, max_hops, payload_size, is_own_message
                 FROM messages WHERE message_id = $1",
                &[&id_hex],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("load: {e}")))?;
        match row {
            Some(r) => {
                let bytes: Vec<u8> = r.get(0);
                let priority: i16 = r.get(1);
                let expires_at: i64 = r.get(2);
                let status: String = r.get(3);
                let created_at: i64 = r.get(4);
                let hop_count: i16 = r.get(5);
                let max_hops: Option<i16> = r.get(6);
                let payload_size: i64 = r.get(7);
                let is_own: bool = r.get(8);
                // TAK-15: tagged dispatch replaces the equality heuristic
                // that could never run for a real sealer (the `?` above it
                // returned first, stranding every legacy plaintext row the
                // moment sealing was enabled). TAK-14: sealed candidates are
                // verified against the full v2 AAD, falling back to v1.
                let cbor = self.decode_stored_blob(
                    self.aad_candidates(
                        &id_hex,
                        priority,
                        expires_at,
                        &status,
                        created_at,
                        hop_count,
                        max_hops,
                        payload_size,
                        is_own,
                    ),
                    &bytes,
                )?;
                codec::decode(&cbor)
                    .map(Some)
                    .map_err(|e| StorageError::Backend(format!("load decode: {e}")))
            }
            None => Ok(None),
        }
    }

    async fn update_status(
        &self,
        id: &MessageId,
        status: MessageStatus,
    ) -> Result<(), StorageError> {
        let client = self.live_client().await?;
        let id_hex = id.to_string();
        let new_status = status_to_text(status);
        // TAK-14: status is authenticated, so a plain single-column UPDATE
        // would desynchronise the AEAD tag. Read-modify-reseal instead, made
        // atomic by an optimistic CAS on the old status in the final UPDATE
        // (no &mut client available under the Arc slot for a real transaction;
        // concurrent status transitions on one id are not a production shape,
        // and the CAS makes any race fail loudly rather than corrupt).
        for _ in 0..3 {
            let row = client
                .query_opt(
                    "SELECT envelope_cbor, priority, expires_at, status, created_at,
                            hop_count, max_hops, payload_size, is_own_message
                     FROM messages WHERE message_id = $1",
                    &[&id_hex],
                )
                .await
                .map_err(|e| StorageError::Backend(format!("update_status read: {e}")))?;
            let Some(r) = row else { return Ok(()) }; // nothing stored: no-op
            let bytes: Vec<u8> = r.get(0);
            let priority: i16 = r.get(1);
            let expires_at: i64 = r.get(2);
            let old_status: String = r.get(3);
            let created_at: i64 = r.get(4);
            let hop_count: i16 = r.get(5);
            let max_hops: Option<i16> = r.get(6);
            let payload_size: i64 = r.get(7);
            let is_own: bool = r.get(8);

            if old_status == new_status {
                return Ok(());
            }
            let cbor = self.decode_stored_blob(
                self.aad_candidates(
                    &id_hex,
                    priority,
                    expires_at,
                    &old_status,
                    created_at,
                    hop_count,
                    max_hops,
                    payload_size,
                    is_own,
                ),
                &bytes,
            )?;
            let sealed = self.sealer.seal(
                &crate::seal::row_aad_v2(
                    &id_hex,
                    priority as u8,
                    expires_at as u64,
                    new_status,
                    created_at,
                    hop_count,
                    max_hops,
                    payload_size,
                    is_own,
                ),
                &cbor,
            )?;
            let mut tagged = Vec::with_capacity(1 + sealed.len());
            tagged.push(crate::seal::envelope_format::SEALED);
            tagged.extend_from_slice(&sealed);

            let updated = client
                .execute(
                    "UPDATE messages SET status = $2, envelope_cbor = $3
                     WHERE message_id = $1 AND status = $4",
                    &[&id_hex, &new_status, &tagged, &old_status],
                )
                .await
                .map_err(|e| StorageError::Backend(format!("update_status: {e}")))?;
            if updated > 0 {
                return Ok(());
            }
            // CAS lost: another writer changed status between read and write.
            // Loop and re-read; after 3 attempts surface the contention.
        }
        Err(StorageError::Backend(format!(
            "update_status: concurrent status transition on {id_hex}"
        )))
    }

    async fn delete(&self, id: &MessageId) -> Result<(), StorageError> {
        let client = self.live_client().await?;
        client
            .execute(
                "DELETE FROM messages WHERE message_id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("delete: {e}")))?;
        Ok(())
    }

    async fn evict_expired(&self, before_unix: u64) -> Result<usize, StorageError> {
        let client = self.live_client().await?;
        delete_expired(&client, before_unix)
            .await
            .map(|n| n as usize)
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn evict_by_priority(&self, target_bytes: u64) -> Result<usize, StorageError> {
        let client = self.live_client().await?;
        evict_lowest_priority(&client, target_bytes)
            .await
            .map(|n| n as usize)
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn usage_bytes(&self) -> Result<u64, StorageError> {
        let client = self.live_client().await?;
        usage_bytes(&client)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_queue(&self, limit: usize) -> Result<Vec<Envelope>, StorageError> {
        let client = self.live_client().await?;
        let rows = client
            .query(
                "SELECT envelope_cbor, message_id, priority, expires_at, status, created_at,
                        hop_count, max_hops, payload_size, is_own_message
                 FROM messages
                 WHERE status IN ('PENDING_SEND', 'IN_TRANSIT')
                 ORDER BY priority ASC, created_at ASC, message_id ASC
                 LIMIT $1",
                &[&(limit as i64)],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("get_queue: {e}")))?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let bytes: Vec<u8> = r.get(0);
            let id: String = r.get(1);
            let priority: i16 = r.get(2);
            let expires_at: i64 = r.get(3);
            let status: String = r.get(4);
            let created_at: i64 = r.get(5);
            let hop_count: i16 = r.get(6);
            let max_hops: Option<i16> = r.get(7);
            let payload_size: i64 = r.get(8);
            let is_own: bool = r.get(9);
            // TAK-3: unreadable rows are quarantined, not fatal (see below).
            // TAK-14: candidates cover v2 + legacy AADs.
            let cbor = self.decode_stored_blob(
                self.aad_candidates(
                    &id,
                    priority,
                    expires_at,
                    &status,
                    created_at,
                    hop_count,
                    max_hops,
                    payload_size,
                    is_own,
                ),
                &bytes,
            );
            let cbor = match cbor {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(message_id = %id, error = %e, "quarantining unsealable row");
                    self.quarantined
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.quarantine_row(&id).await;
                    continue;
                }
            };
            match codec::decode(&cbor) {
                Ok(env) => out.push(env),
                Err(e) => {
                    tracing::error!(message_id = %id, error = %e, "quarantining undecodable row");
                    self.quarantined
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.quarantine_row(&id).await;
                    continue;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_parser_accepts_well_formed_hex() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = parse_node_id(hex).expect("valid id");
        assert_eq!(parsed[0], 0x01);
        assert_eq!(parsed[15], 0xef);
        assert_eq!(parsed[31], 0xef);
    }

    #[test]
    fn node_id_parser_rejects_malformed_input() {
        assert_eq!(parse_node_id(""), None);
        assert_eq!(parse_node_id("zz"), None);
        assert_eq!(parse_node_id("0123"), None); // too short
        let long_odd = format!("0{}", "a".repeat(64));
        assert_eq!(parse_node_id(&long_odd), None);
        let not_hex = "g".repeat(64);
        assert_eq!(parse_node_id(&not_hex), None);
    }

    #[test]
    fn threshold_defaults_when_absent_or_invalid() {
        assert_eq!(parse_eviction_threshold(None), 0.8);
        assert_eq!(parse_eviction_threshold(Some("not-a-number".into())), 0.8);
        // Out-of-range values must fall back, never disable or hyper-trigger.
        assert_eq!(parse_eviction_threshold(Some("1.5".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("0.0".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("-0.2".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("0.96".into())), 0.8);
    }

    #[test]
    fn threshold_accepts_in_range_values() {
        assert_eq!(parse_eviction_threshold(Some("0.5".into())), 0.5);
        assert_eq!(parse_eviction_threshold(Some("0.1".into())), 0.1);
        assert_eq!(parse_eviction_threshold(Some("0.95".into())), 0.95);
    }
}

#[cfg(test)]
mod tak4_backoff_tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_caps_at_thirty_seconds() {
        // Deterministic progression, no jitter in the way.
        let mut ms = initial_backoff_ms();
        for expected in [2_000u64, 4_000, 8_000, 16_000] {
            ms = doubled_capped(ms);
            assert_eq!(ms, expected);
        }
        assert_eq!(doubled_capped(ms), 30_000);
        assert_eq!(doubled_capped(30_000), 30_000, "stays capped");
    }

    #[test]
    fn jitter_stays_within_twenty_percent_and_never_zero() {
        for base in [1_000u64, 5_000, 30_000] {
            let j = apply_jitter(base);
            let spread = base / 5;
            assert!(
                j >= base - spread && j <= base + spread,
                "base={base} j={j}"
            );
            assert!(j > 0);
        }
    }

    #[test]
    fn backoff_sequence_bounded_forever() {
        let mut ms = initial_backoff_ms();
        for _ in 0..30 {
            ms = next_backoff_ms(ms);
            assert!(
                (RECONNECT_INITIAL_MS - RECONNECT_INITIAL_MS / 5
                    ..=RECONNECT_MAX_MS + RECONNECT_MAX_MS / 5)
                    .contains(&ms),
                "ms={ms}"
            );
        }
    }
}
