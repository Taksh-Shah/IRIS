//! # IRIS Core
//!
//! Core protocol engine of the IRIS resilient communication fabric (ADR-0003).
//!
//! This crate implements the durable engineering state from `docs/`:
//! - Protocol wire types + canonical CBOR codec (`protocol`) — PROTO-001
//! - Transport abstraction layer (`transport`) — TRANSPORT-001
//! - Message model (`message`) — MSG-001
//!
//! Platform-specific adapters (Android BLE, iOS BLE, Wi-Fi Aware) are injected
//! from outside this crate via the `Transport` trait object — the core never
//! depends on a concrete transport at compile time.
//!
//! Phase: CORE_PROTOCOL_IMPLEMENTATION (authorized 2026-08-12).

pub mod crypto;
pub mod discovery;
pub mod emergency;
pub mod error;
pub mod gateway;
pub mod identity;
pub mod message;
pub mod message_engine;
pub mod observability;
pub mod protocol;
pub mod routing;
pub mod security;
pub mod sim;
pub mod transport;

#[cfg(kani)]
mod kani_proofs;

pub use discovery::{DiscoveryManager, DiscoveryMode};
pub use error::TransportError;
pub use gateway::{
    compute_gateway_quality, self_internet_gateway, GatewayCapability, GatewayManager,
    GatewaySelection, GatewayType,
};
pub use identity::{IdentityManager, KeyStore, TrustStore};
pub use message::{
    DiscoveryConfig, IncomingMessage, LinkQuality, MessagePriority, NodeAdvertisement, PeerId,
    PeerInfo, SendReceipt, SerializedMessage, TransportLink,
};
pub use observability::{DeliveryWindow, MetricsRegistry};
pub use protocol::{ContentType, EncryptionHdr, Envelope, EnvelopeError, MessageId, RoutingHints};
pub use routing::{
    DeliveryPredictability, OpportunisticDecision, OpportunisticRouter, ProphetConfig, SprayBudget,
};
pub use security::{
    AclDecision, AlertClass, EmergencyAcl, FullSecurityPolicy, MessageClass, NoopSecurityPolicy,
    QuotaDecision, QuotaManager, RateLimitDecision, RateLimiter, ReplayDecision, ReplayEngine,
    ReplaySnapshot, ReputationEngine, ReputationEvent, SecurityPolicy, SpamDecision, SpamEngine,
};
pub use transport::{
    Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportManager, TransportState, TransportStateEvent,
};
