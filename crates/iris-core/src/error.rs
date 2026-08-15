//! Transport error types shared across the fabric.

use std::fmt;

/// Errors produced by transport implementations and the transport manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// No active link to the target peer on this transport.
    NotConnected,
    /// The transport cannot perform the requested operation in its current
    /// state (e.g. BLE adapter not present, permission denied).
    NotSupported,
    /// A discovery scan timed out.
    DiscoveryTimeout,
    /// No peer matched the discovery filter.
    PeerNotFound,
    /// A connection attempt failed or timed out.
    ConnectionFailed,
    /// The transport is shutting down.
    ShuttingDown,
    /// The transport is busy (all pooled connections saturated).
    Busy,
    /// Protocol-level framing/decoding failure.
    Protocol(String),
    /// Underlying I/O failure (wraps `std::io::Error`).
    Io(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NotConnected => write!(f, "transport: not connected to peer"),
            TransportError::NotSupported => write!(f, "transport: operation not supported"),
            TransportError::DiscoveryTimeout => write!(f, "transport: discovery timed out"),
            TransportError::PeerNotFound => write!(f, "transport: peer not found"),
            TransportError::ConnectionFailed => write!(f, "transport: connection failed"),
            TransportError::ShuttingDown => write!(f, "transport: shutting down"),
            TransportError::Busy => write!(f, "transport: busy"),
            TransportError::Protocol(msg) => write!(f, "transport protocol error: {msg}"),
            TransportError::Io(msg) => write!(f, "transport io error: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io(e.to_string())
    }
}
