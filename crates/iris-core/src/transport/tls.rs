//! Verified TLS client configuration for IRIS relay connections.
//!
//! Production callers receive the Mozilla/WebPKI trust store and retain normal
//! certificate-chain plus hostname verification. This module deliberately has
//! no insecure verifier; test certificates belong in test-only server fixtures.

use std::net::IpAddr;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use crate::TransportError;

/// ALPN identifier reserved for the versioned IRIS relay protocol.
pub const RELAY_ALPN: &[u8] = b"iris-relay/1";

/// Construct the standard verified TLS connector used for public relay
/// endpoints. The caller must supply the expected DNS name or IP SAN to
/// `connect`; no API exists here to disable certificate verification.
pub fn verified_relay_connector() -> TlsConnector {
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
    TlsConnector::from(Arc::new(config))
}

/// Convert a configured relay server identity to rustls' verifier input.
/// DNS names require a matching certificate SAN; literal IPs require an IP SAN.
pub fn relay_server_name(value: &str) -> Result<ServerName<'static>, TransportError> {
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Ok(ServerName::IpAddress(ip.into()));
    }
    ServerName::try_from(value.to_owned())
        .map_err(|_| TransportError::Protocol("invalid relay TLS server name".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_advertises_only_iris_relay_alpn() {
        let connector = verified_relay_connector();
        assert_eq!(connector.config().alpn_protocols, vec![RELAY_ALPN.to_vec()]);
    }

    #[test]
    fn accepts_dns_and_ip_server_names_but_rejects_invalid_names() {
        assert!(relay_server_name("relay.iris.example").is_ok());
        assert!(relay_server_name("192.0.2.42").is_ok());
        assert!(relay_server_name("bad name with spaces").is_err());
    }
}
