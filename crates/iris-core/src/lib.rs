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

pub mod error;
pub mod message;
pub mod message_engine;
pub mod observability;
pub mod protocol;
pub mod transport;
pub mod discovery;
pub mod routing;
pub mod sim;
pub mod gateway;
pub mod crypto;
pub mod identity;
pub mod emergency;

pub use error::TransportError;
pub use message::{
    DiscoveryConfig, IncomingMessage, LinkQuality, MessagePriority, NodeAdvertisement, PeerId,
    PeerInfo, SendReceipt, SerializedMessage, TransportLink,
};
pub use protocol::{
    ContentType, EncryptionHdr, Envelope, EnvelopeError, MessageId, RoutingHints,
};
pub use transport::{
    Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportManager, TransportState, TransportStateEvent,
};
pub use discovery::{DiscoveryManager, DiscoveryMode};
pub use gateway::{
    GatewayCapability, GatewayManager, GatewaySelection, GatewayType,
    compute_gateway_quality, self_internet_gateway,
};
pub use routing::{
    DeliveryPredictability, OpportunisticDecision, OpportunisticRouter, ProphetConfig,
    SprayBudget,
};
pub use observability::{DeliveryWindow, MetricsRegistry, PriorityTally};
pub use identity::{IdentityManager, KeyStore, TrustStore};
