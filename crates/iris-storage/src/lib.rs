//! # IRIS Storage (STORE-001)
//!
//! Persistent message store implementing the [`MessageStorage`] seam that the
//! message engine (MSG-001) depends on. **PostgreSQL** backend via
//! `tokio-postgres` (operator-directed 2026-08-13 — replaces the SQLite/SQLCipher
//! design in STORAGE.md). Full-fidelity persistence uses the canonical CBOR
//! envelope (PROTO-001 codec), so `load` returns the byte-exact original.
//!
//! No SQL appears in `iris-core` — it all lives here (STORAGE.md).

mod eviction;
pub mod gc;
mod pg;
mod schema;
mod seal;

pub use pg::{PgStorage, PgStorageConfig};
pub use seal::{NoSealer, RowSealer, StorageKeySealer};

/// Re-export the storage error surface from iris-core so downstream consumers
/// need only this crate.
pub use iris_core::message_engine::storage::{MessageStorage, StorageError};
