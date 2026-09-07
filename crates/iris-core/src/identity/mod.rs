//! Identity, key management, trust — IDENT-001.
//!
//! See `docs/implementation/IDENT_DESIGN.md`. This module supplies:
//! - **provisioning** — [`IdentityManager`] (ID-001 D2): provision-on-first-run
//!   / load-with-loud-failure, persisted `NodeIdentityV1`,
//! - **derivation** — key-derived PeerId + short id + human UID (D1, DEC-P0005),
//! - **key store** — [`KeyStore`] seam + protected [`FileKeyStore`] (D2),
//! - **advertisement** — RED-0005 signed X25519-key certificate (SPKI-style),
//! - **trust** — [`TrustStore`] TOFU + verified tier + [`TrustKeyDirectory`],
//! - **chains** — RED-0008 key-anchored `auth_cert_chain` validation,
//! - **rotation/revocation** — D4 signed rotation events (null-rotation),
//! - **small-order guard** — RED-0011 X25519 low-order key rejection.
//!
//! Zero engine breaking changes: the message engine keeps using the
//! `CryptoProvider` seam; identity plugs a real [`IrisCryptoProvider`]
//! (built from the persisted identity) plus a trust-fed key directory.

pub mod advertise;
pub mod chain;
pub mod peer_id;
pub mod provision;
pub mod rotate;
pub mod sas;
pub mod small_order;
pub mod store;
pub mod trust_store;

pub use advertise::{AdvertiseError, KeyAdvertisementV1};
pub use chain::{verify_chain, ChainError, MAX_CHAIN_LEN};
pub use peer_id::{human_uid, peer_id, peer_short, peer_short_from_sender, peer_short_hex};
pub use provision::{IdentityError, IdentityManager, NodeIdentityV1, IDENTITY_FORMAT_VERSION};
pub use rotate::{
    apply as apply_rotation, RotationEventV1, RotationOutcome, KIND_REVOCATION, KIND_ROTATION,
    ROTATION_FORMAT_VERSION,
};
pub use sas::compute_sas;
pub use small_order::{is_small_order, SMALL_ORDER_U};
pub use store::{FileKeyStore, KeyStore, KeyStoreError};
pub use trust_store::{
    AdoptionOutcome, TrustEntry, TrustKeyDirectory, TrustLevel, TrustStore, VerifyError,
};
