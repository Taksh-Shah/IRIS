//! # Emergency System (EMERG-001)
//!
//! SOS + emergency-broadcast + disaster-mode module layered onto the existing
//! wire (`protocol`), identity/trust (`identity::chain::verify_chain` +
//! `identity::trust_store`), and delivery (`message_engine`) subsystems.
//!
//! ## Design reality (verified against real APIs, iter 67+)
//!
//! - The codec uses the same canonical-CBOR (RFC 8949 §4.2) machinery as the
//!   envelope codec (`ciborium::Value`, CDE: ascending integer keys, definite
//!   lengths). There is no `cbor_lite.rs`.
//! - `KeyAdvertisementV1` carries **no** authority profile fields (no
//!   role/scope/severity/drill). The authority profile is **payload-level**:
//!   `AuthorityMeta` rides inside the signed `EmergencyBroadcast` (top line of
//!   EMERG_DESIGN.md §3). Enforcement binds it to the sender via the envelope
//!   Ed25519 signature + `verify_chain` RED-0008 anchor.
//! - `verify_chain(chain, trust, sender_id: &[u8])` returns `Result<(), _>`;
//!   the leaf identity is recovered by decoding the leaf advertisement
//!   ourselves.
//!
//! Module layout:
//!   model        — payload structs + enum wire codes (AC-1)
//!   codec        — canonical CBOR encode/decode (AC-1)
//!   authority    — chain + payload-profile enforcement (AC-2/3)
//!   sos          — SOS build/cancel/verify policy (AC-4/6)
//!   rate_limit   — per-sender rolling-window SOS limiter (AC-5)
//!   mode         — disaster state machine (AC-7)
//!   audit        — pseudonymous metadata audit trail (AC-9)
//!   drill        — TEST_MODE drill helpers (AC-10, DEC-EMERG-0008)
//!   broadcast    — verify-and-relay pipeline (AC-8/10)
//!   provider     — EmergencyProvider trait + NoopEmergencyProvider (AC-11)

pub mod audit;
pub mod authority;
pub mod broadcast;
pub mod codec;
pub mod drill;
pub mod model;
pub mod mode;
pub mod provider;
pub mod rate_limit;
pub mod sos;

pub use audit::{AuditEvent, AuditLog, EmergencyAuditRecord};
pub use authority::{AuthorityError, VerifiedAuthority, verify_authoritative};
pub use broadcast::{VerifyOutcome, is_emergency_content, verify_and_classify};
pub use codec::{decode_broadcast, decode_sos, encode_broadcast, encode_sos};
pub use model::{
    AlertMessageType, AuthorityMeta, Certainty, EmergencyBroadcast, LocationSource, ModelError,
    Severity, SosKind, SosMessage, SosReason,
};
pub use mode::{DisasterMode, ModeTransition, Triggers, guarded_transition};
pub use provider::{EmergencyProvider, EmergencyGateway, NoopEmergencyProvider};
pub use rate_limit::{RateLimitDecision, SosRateLimiter};
pub use sos::{SosError, SosOutcome, classify_sos};