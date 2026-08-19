//! `IrisBody` — foreign envelope-body surface for the iOS shell (IOS-001).
//!
//! The Swift app implements this foreign protocol and installs it on the
//! engine via `IrisEngine::set_body_renderer` (AC-3 G-IOS spike). It carries
//! the envelope projection (`FfiIrisEnvelope`) that the shell uses for
//! notification / Live-Activity presentation (status only — never payload on
//! the Lock Screen, IOS_DESIGN D-8) and includes one **async** method so the
//! UniFFI async-over-FFI foreign-future/oneshot path stays exercised in the
//! generated Swift bindings (RES-0025 RQ-1 finding 2; the surface that needs
//! the Swift-5 language-mode pin, DEC-IOS-0002 / mozilla/uniffi-rs#2929).

use crate::ffi::error::IrisFfiError;

/// Owned envelope projection handed to the Swift body renderer.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiIrisEnvelope {
    /// 16-byte wire message id.
    pub message_id: Vec<u8>,
    /// 32-byte sender node id.
    pub sender_id: Vec<u8>,
    /// 32-byte recipient node id.
    pub recipient_id: Vec<u8>,
    /// 0 = P0 … 7 = P7.
    pub priority: u8,
    /// Unix epoch milliseconds at origination.
    pub originated_at_ms: u64,
    /// Encrypted body bytes (the shell never stores/exposes this beyond
    /// presentation seams).
    pub payload: Vec<u8>,
}

/// Foreign body renderer implemented by the Swift shell (protocol `IrisBody`).
/// `@unchecked Sendable` on the Swift implementation class (the generated
/// protocol is `Sendable`; mutable CoreBluetooth/Keychain state is confined to
/// a dedicated serial queue — IOS_DESIGN D-1 / §8).
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait IrisBody: Send + Sync + 'static {
    /// Synchronous presentation hook (notifications; Live Activity status).
    fn summarize(&self, envelope: FfiIrisEnvelope) -> String;
    /// Async body surface (foreign-future bridge proof; Swift `async` method).
    /// Must return success for a well-formed envelope.
    async fn describe(&self, envelope: FfiIrisEnvelope) -> Result<String, IrisFfiError>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Rust-side twin of the Swift `IrisBody` implementation (G-IOS spike).
    #[derive(Default)]
    pub(crate) struct SimBody {
        pub(crate) summarizes: AtomicUsize,
        pub(crate) describes: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl IrisBody for SimBody {
        fn summarize(&self, envelope: FfiIrisEnvelope) -> String {
            self.summarizes.fetch_add(1, Ordering::Relaxed);
            let hex_id: String = envelope
                .message_id
                .iter()
                .take(2)
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("iris:{hex_id}:p{}", envelope.priority)
        }
        async fn describe(&self, envelope: FfiIrisEnvelope) -> Result<String, IrisFfiError> {
            self.describes.fetch_add(1, Ordering::Relaxed);
            if envelope.payload.is_empty() {
                return Err(IrisFfiError::InvalidArgument(
                    "irisf body must not be empty".into(),
                ));
            }
            Ok("iris-body".into())
        }
    }

    #[tokio::test]
    async fn iris_body_async_surface_completes() {
        let b = SimBody::default();
        let s = b
            .describe(FfiIrisEnvelope {
                message_id: vec![1u8; 16],
                sender_id: vec![2u8; 32],
                recipient_id: vec![3u8; 32],
                priority: 4,
                originated_at_ms: 1234,
                payload: b"hello".to_vec(),
            })
            .await
            .expect("async body surface resolves");
        assert_eq!(s, "iris-body");
        assert_eq!(b.describes.load(Ordering::Relaxed), 1);
    }
}
