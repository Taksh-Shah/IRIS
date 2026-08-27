//! Transport error types shared across the fabric.

use std::fmt;

/// Errors produced by transport implementations and the transport manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// No active link to the target peer on this transport.
    NotConnected,
    /// MG-41: the required radio/hardware is not present on this device —
    /// permanent for this device. Distinct from [`Self::PermissionDenied`]:
    /// the two have opposite remediations (hide the feature vs. prompt the
    /// user) and the old single `NotSupported` variant could express
    /// neither to a caller.
    HardwareUnavailable,
    /// MG-41: a required OS permission has not been granted — user-
    /// actionable (a UI can prompt), and recovery should be immediate once
    /// granted, unlike [`Self::HardwareUnavailable`].
    PermissionDenied { permission: &'static str },
    /// MG-41: the radio exists and is permitted, but is currently switched
    /// off (e.g. Bluetooth toggled off in system settings) — a third case
    /// the finding's own analogous `TransportState` split names
    /// (`NoHardware`/`PermissionDenied`/`RadioOff`) that its `TransportError`
    /// sketch omitted. Neither "hide the feature" ([`Self::HardwareUnavailable`])
    /// nor "prompt an in-app permission dialog" ([`Self::PermissionDenied`])
    /// is the right remediation — the user needs to flip a system toggle.
    RadioDisabled,
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
    /// Genuine protocol-level framing/decoding failure — the message is
    /// corrupt or malformed. MG-40: narrowed to this one meaning; the two
    /// other things this variant used to carry now have their own variant.
    Protocol(String),
    /// MG-40: payload exceeds this transport's frame/MTU capacity —
    /// resolvable by fragmenting upstream and retrying **on the same
    /// transport**, unlike a genuine [`Self::Protocol`] failure or a
    /// [`Self::PolicyDenied`] one.
    MessageTooLarge { limit: usize, actual: usize },
    /// MG-40: this transport can never carry this message (e.g. a priority
    /// class it structurally excludes) — permanent for this transport;
    /// the caller should re-select immediately and never retry here.
    PolicyDenied(&'static str),
    /// Underlying I/O failure. MG-39: preserves `ErrorKind` (lost by the
    /// old `Io(String)`, which stringified immediately) so a caller can
    /// tell a transient failure from a permanent one — see
    /// [`Self::is_retryable`].
    Io { kind: std::io::ErrorKind, msg: String },
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::NotConnected => write!(f, "transport: not connected to peer"),
            TransportError::HardwareUnavailable => {
                write!(f, "transport: required hardware not present on this device")
            }
            TransportError::PermissionDenied { permission } => {
                write!(f, "transport: permission not granted: {permission}")
            }
            TransportError::RadioDisabled => write!(f, "transport: radio is switched off"),
            TransportError::DiscoveryTimeout => write!(f, "transport: discovery timed out"),
            TransportError::PeerNotFound => write!(f, "transport: peer not found"),
            TransportError::ConnectionFailed => write!(f, "transport: connection failed"),
            TransportError::ShuttingDown => write!(f, "transport: shutting down"),
            TransportError::Busy => write!(f, "transport: busy"),
            TransportError::Protocol(msg) => write!(f, "transport protocol error: {msg}"),
            TransportError::MessageTooLarge { limit, actual } => write!(
                f,
                "transport: message of {actual} bytes exceeds the {limit}-byte capacity"
            ),
            TransportError::PolicyDenied(reason) => write!(f, "transport: policy denied: {reason}"),
            TransportError::Io { kind, msg } => write!(f, "transport io error: {msg} ({kind:?})"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io {
            kind: e.kind(),
            msg: e.to_string(),
        }
    }
}

impl TransportError {
    /// MG-39: whether a retry on the **same** transport is worth
    /// attempting. `Io` variants split on `ErrorKind` — `WouldBlock`/
    /// `TimedOut`/`Interrupted` are transient; everything else (a
    /// `PermissionDenied`, `NotFound`, `AddrNotAvailable` `io::Error`, or
    /// any non-`Io` variant not listed here) is not, and the caller should
    /// abandon this transport and re-select rather than burn a retry
    /// budget against a failure that cannot recover on its own.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            TransportError::Busy
                | TransportError::ConnectionFailed
                | TransportError::Io {
                    kind: std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted,
                    ..
                }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mg39_is_retryable_covers_the_transient_cases() {
        assert!(TransportError::Busy.is_retryable());
        assert!(TransportError::ConnectionFailed.is_retryable());
        for kind in [
            std::io::ErrorKind::WouldBlock,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::Interrupted,
        ] {
            assert!(
                TransportError::Io {
                    kind,
                    msg: "x".into()
                }
                .is_retryable(),
                "{kind:?} must be retryable"
            );
        }
    }

    #[test]
    fn mg39_is_retryable_excludes_permanent_failures() {
        // The exact trigger scenario MG-39 describes: a permission failure
        // must not be treated as worth retrying on the same transport.
        assert!(!TransportError::PermissionDenied {
            permission: "bluetooth"
        }
        .is_retryable());
        assert!(!TransportError::HardwareUnavailable.is_retryable());
        assert!(!TransportError::RadioDisabled.is_retryable());
        // MG-40's own trigger scenario.
        assert!(!TransportError::PolicyDenied("satellite carries P0-P2 only").is_retryable());
        assert!(!TransportError::MessageTooLarge {
            limit: 100,
            actual: 200
        }
        .is_retryable());
        assert!(!TransportError::Protocol("bad frame".into()).is_retryable());
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::AddrNotAvailable,
            std::io::ErrorKind::Other,
        ] {
            assert!(
                !TransportError::Io {
                    kind,
                    msg: "x".into()
                }
                .is_retryable(),
                "{kind:?} must not be retryable"
            );
        }
    }

    #[test]
    fn mg39_from_io_error_preserves_kind() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "boom");
        let te: TransportError = io_err.into();
        assert_eq!(
            te,
            TransportError::Io {
                kind: std::io::ErrorKind::TimedOut,
                msg: "boom".to_string()
            }
        );
        assert!(te.is_retryable());
    }

    #[test]
    fn mg42_equality_is_no_longer_purely_message_based_for_io() {
        // MG-42: two Io errors with the same kind but differently-worded
        // messages used to compare unequal under the old Io(String) shape
        // (a semantic dependency on log prose). The kind is now the first
        // field compared, which is the discriminant that actually matters
        // for retry-policy purposes — but message text still participates
        // in equality (no manual PartialEq was introduced, matching the
        // finding's own "if a real source() is wanted" being explicitly
        // optional), so this documents the current behavior rather than
        // claiming full message-independence.
        let a = TransportError::Io {
            kind: std::io::ErrorKind::TimedOut,
            msg: "connection timed out".to_string(),
        };
        let b = TransportError::Io {
            kind: std::io::ErrorKind::TimedOut,
            msg: "connection timed out".to_string(),
        };
        assert_eq!(a, b);
        assert_eq!(a.is_retryable(), b.is_retryable());
    }
}
