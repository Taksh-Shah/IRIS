//! Authenticated, bounded IRIS relay server.
//!
//! Usage: `iris-relay-server [ADDR:PORT] CERT_DER KEY_DER`
//! Environment fallbacks: `IRIS_RELAY_CERT_DER`, `IRIS_RELAY_KEY_DER`.
//! Production key material is never compiled into this binary.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use ed25519_dalek::{Signature, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_rustls::TlsAcceptor;

use iris_core::transport::relay_protocol::{
    self, RelayAckStatus, RelayFrame, MAX_RELAY_PAYLOAD, RELAY_HEADER_LEN,
};
use iris_core::transport::tls::RELAY_ALPN;

#[cfg(test)]
#[path = "../transport/test_certs.rs"]
mod test_certs;

const MAX_CONNECTIONS: usize = 256;
const DEFAULT_MAX_CONNECTIONS_PER_IP: usize = MAX_CONNECTIONS;
const SESSION_QUEUE: usize = 8;
const CONTROL_QUEUE: usize = 16;
const GLOBAL_QUEUE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ROUTE_FRAMES_PER_MINUTE: u32 = 120;
const MAX_ROUTE_BYTES_PER_MINUTE: usize = 8 * 1024 * 1024;
const MAX_RATE_IDENTITIES: usize = 4096;
const TLS_TIMEOUT: Duration = Duration::from_secs(10);
const REGISTER_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(120);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

struct QueuedFrame {
    bytes: Vec<u8>,
    _byte_permit: OwnedSemaphorePermit,
}

#[derive(Clone)]
struct Session {
    generation: u64,
    tx: mpsc::Sender<QueuedFrame>,
}

type Router = Arc<Mutex<HashMap<[u8; 32], Session>>>;
type IpCounts = Arc<StdMutex<HashMap<IpAddr, usize>>>;
type RateLimits = Arc<Mutex<HashMap<[u8; 32], RateWindow>>>;

struct RateWindow {
    started: Instant,
    frames: u32,
    bytes: usize,
}

struct IpConnectionGuard {
    ip: IpAddr,
    counts: IpCounts,
}

impl IpConnectionGuard {
    fn try_acquire(ip: IpAddr, counts: IpCounts, limit: usize) -> Option<Self> {
        let mut guard = counts.lock().ok()?;
        let count = guard.entry(ip).or_default();
        if *count >= limit {
            return None;
        }
        *count += 1;
        drop(guard);
        Some(Self { ip, counts })
    }
}

impl Drop for IpConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.counts.lock() {
            if let Some(count) = guard.get_mut(&self.ip) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    guard.remove(&self.ip);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("relay_server=info".parse().expect("valid directive")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let addr: SocketAddr = args
        .first()
        .cloned()
        .unwrap_or_else(|| "0.0.0.0:7890".into())
        .parse()
        .unwrap_or_else(|_| usage("ADDR:PORT is invalid"));
    let cert_paths = required_path(args.get(1), "IRIS_RELAY_CERT_DER", "certificate DER chain");
    let key_path = required_path(args.get(2), "IRIS_RELAY_KEY_DER", "PKCS#8 private-key DER");
    let max_connections_per_ip = std::env::var("IRIS_RELAY_MAX_CONNECTIONS_PER_IP")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CONNECTIONS_PER_IP)
        .min(MAX_CONNECTIONS);

    let certs = cert_paths
        .to_string_lossy()
        .split(',')
        .map(|path| {
            let path = PathBuf::from(path.trim());
            std::fs::read(&path)
                .map(CertificateDer::from)
                .unwrap_or_else(|e| usage(&format!("cannot read {}: {e}", path.display())))
        })
        .collect::<Vec<_>>();
    let key = std::fs::read(&key_path)
        .unwrap_or_else(|e| usage(&format!("cannot read {}: {e}", key_path.display())));

    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key));
    let mut server_config =
        ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_no_client_auth()
            .with_single_cert(certs, key_der)
            .unwrap_or_else(|e| usage(&format!("invalid certificate/key: {e}")));
    server_config.alpn_protocols = vec![RELAY_ALPN.to_vec()];

    let listener = TcpListener::bind(addr).await.expect("bind relay listener");
    let bound = listener.local_addr().expect("bound address");
    tracing::info!(%bound, "IRIS relay listening (TLS 1.3, ALPN iris-relay/1)");

    serve(
        listener,
        TlsAcceptor::from(Arc::new(server_config)),
        max_connections_per_ip,
    )
    .await;
}

fn required_path(arg: Option<&String>, env_name: &str, label: &str) -> PathBuf {
    arg.cloned()
        .or_else(|| std::env::var(env_name).ok())
        .map(PathBuf::from)
        .unwrap_or_else(|| usage(&format!("missing {label} path")))
}

fn usage(message: &str) -> ! {
    eprintln!("error: {message}");
    eprintln!("usage: iris-relay-server [ADDR:PORT] CERT_DER[,INTERMEDIATE_DER...] KEY_DER");
    eprintln!("or set IRIS_RELAY_CERT_DER and IRIS_RELAY_KEY_DER");
    std::process::exit(2)
}

async fn serve(listener: TcpListener, acceptor: TlsAcceptor, max_connections_per_ip: usize) {
    let router: Router = Arc::new(Mutex::new(HashMap::new()));
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let queue_budget = Arc::new(Semaphore::new(GLOBAL_QUEUE_BYTES));
    let ip_counts: IpCounts = Arc::new(StdMutex::new(HashMap::new()));
    let generations = Arc::new(AtomicU64::new(1));
    let rate_limits: RateLimits = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(error = %e, "relay accept failed");
                continue;
            }
        };
        let Ok(permit) = permits.clone().try_acquire_owned() else {
            tracing::warn!(%peer_addr, "relay connection limit reached");
            continue;
        };
        let Some(ip_guard) = IpConnectionGuard::try_acquire(
            peer_addr.ip(),
            ip_counts.clone(),
            max_connections_per_ip,
        ) else {
            tracing::warn!(%peer_addr, "relay per-IP connection limit reached");
            continue;
        };
        stream.set_nodelay(true).ok();
        let acceptor = acceptor.clone();
        let router = router.clone();
        let generation = generations.fetch_add(1, Ordering::Relaxed);
        let queue_budget = queue_budget.clone();
        let rate_limits = rate_limits.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ip_guard = ip_guard;
            let mut tls = match tokio::time::timeout(TLS_TIMEOUT, acceptor.accept(stream)).await {
                Ok(Ok(tls)) => tls,
                Ok(Err(e)) => {
                    tracing::warn!(%peer_addr, error = %e, "TLS handshake failed");
                    return;
                }
                Err(_) => {
                    tracing::warn!(%peer_addr, "TLS handshake timed out");
                    return;
                }
            };
            if tls.get_ref().1.alpn_protocol() != Some(RELAY_ALPN) {
                tracing::warn!(%peer_addr, "required relay ALPN not negotiated");
                return;
            }

            let mut challenge = [0u8; 32];
            OsRng.fill_bytes(&mut challenge);
            let challenge_frame =
                relay_protocol::encode(&RelayFrame::Challenge { nonce: challenge })
                    .expect("challenge frame");
            if write_bounded(&mut tls, &challenge_frame).await.is_err() {
                return;
            }

            let (read, write) = tokio::io::split(tls);
            let (tx, mut rx) = mpsc::channel::<QueuedFrame>(SESSION_QUEUE);
            let (control_tx, mut control_rx) = mpsc::channel::<QueuedFrame>(CONTROL_QUEUE);
            let (writer_failed_tx, writer_failed_rx) = oneshot::channel();
            let writer = tokio::spawn(async move {
                let mut write = write;
                let heartbeat =
                    relay_protocol::encode(&RelayFrame::Heartbeat).expect("bounded heartbeat");
                let mut ticker = tokio::time::interval_at(
                    tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
                    HEARTBEAT_INTERVAL,
                );
                loop {
                    let bytes = tokio::select! {
                        biased;
                        queued = control_rx.recv() => match queued {
                            Some(frame) => frame.bytes,
                            None => break,
                        },
                        queued = rx.recv() => match queued {
                            Some(frame) => frame.bytes,
                            None => break,
                        },
                        _ = ticker.tick() => heartbeat.clone(),
                    };
                    if write_bounded(&mut write, &bytes).await.is_err() {
                        break;
                    }
                }
                let _ = writer_failed_tx.send(());
            });
            handle_client(
                read,
                tx,
                control_tx,
                router,
                queue_budget,
                rate_limits,
                challenge,
                generation,
                writer_failed_rx,
            )
            .await;
            writer.abort();
        });
    }
}

async fn handle_client<R: AsyncRead + Unpin>(
    mut read: R,
    tx: mpsc::Sender<QueuedFrame>,
    control_tx: mpsc::Sender<QueuedFrame>,
    router: Router,
    queue_budget: Arc<Semaphore>,
    rate_limits: RateLimits,
    challenge: [u8; 32],
    generation: u64,
    mut writer_failed: oneshot::Receiver<()>,
) {
    let frame = match read_frame(&mut read, REGISTER_TIMEOUT).await {
        Some(frame) => frame,
        None => return,
    };
    let (registered_id, signature) = match frame {
        RelayFrame::Register { peer_id, signature } => (peer_id.0, signature),
        _ => {
            tracing::warn!("first client frame was not authenticated Register");
            return;
        }
    };
    if !verify_registration(&challenge, &registered_id, &signature) {
        tracing::warn!(peer = %hex::encode(registered_id), "registration signature rejected");
        return;
    }

    let registered = relay_protocol::encode(&RelayFrame::Heartbeat).expect("registration ack");
    if queue_frame(&control_tx, &queue_budget, registered).is_err() {
        return;
    }
    // The ACK must be ahead of any routed data. Publishing first allowed a
    // concurrent sender to enqueue Route before Heartbeat, causing a valid
    // client to reject its own registration nondeterministically.
    router.lock().await.insert(
        registered_id,
        Session {
            generation,
            tx: tx.clone(),
        },
    );
    tracing::info!(peer = %hex::encode(registered_id), generation, "client registered");

    loop {
        let frame = tokio::select! {
            frame = read_frame(&mut read, READ_TIMEOUT) => frame,
            _ = &mut writer_failed => None,
        };
        let Some(frame) = frame else { break };
        if !is_current_generation(&router, &registered_id, generation).await {
            break;
        }
        match frame {
            RelayFrame::Route {
                route_id,
                source,
                target,
                payload,
            } => {
                if source.0 != registered_id {
                    tracing::warn!(peer = %hex::encode(registered_id), "route source spoofing attempt");
                    break;
                }
                if !charge_route(&rate_limits, registered_id, payload.len()).await {
                    tracing::warn!(peer = %hex::encode(registered_id), "relay route rate limit exceeded");
                    break;
                }
                let target_session = router.lock().await.get(&target.0).cloned();
                let status = if let Some(session) = target_session {
                    let outbound = relay_protocol::encode(&RelayFrame::Route {
                        route_id,
                        source,
                        target,
                        payload,
                    });
                    match outbound {
                        Ok(bytes)
                            if is_current_generation(&router, &target.0, session.generation)
                                .await =>
                        {
                            match queue_frame(&session.tx, &queue_budget, bytes) {
                                Ok(()) => RelayAckStatus::ForwardedToLiveSession,
                                Err(_) => RelayAckStatus::TargetBackpressured,
                            }
                        }
                        Err(_) => RelayAckStatus::TargetBackpressured,
                        Ok(_) => RelayAckStatus::TargetOffline,
                    }
                } else {
                    RelayAckStatus::TargetOffline
                };
                let ack = relay_protocol::encode(&RelayFrame::Ack {
                    route_id,
                    target,
                    status,
                })
                .expect("bounded ack");
                if queue_frame(&control_tx, &queue_budget, ack).is_err() {
                    break;
                }
            }
            RelayFrame::Heartbeat => {
                // Client response to the server's periodic liveness probe.
            }
            RelayFrame::Challenge { .. } | RelayFrame::Register { .. } | RelayFrame::Ack { .. } => {
                tracing::warn!(peer = %hex::encode(registered_id), "unexpected relay frame");
                break;
            }
        }
    }

    remove_if_generation(&router, &registered_id, generation).await;
}

/// Charge a route to an identity-scoped window that survives reconnects.
/// Stale windows are purged before the bounded map admits another identity.
async fn charge_route(rate_limits: &RateLimits, peer: [u8; 32], bytes: usize) -> bool {
    let now = Instant::now();
    let mut limits = rate_limits.lock().await;
    limits.retain(|_, window| now.duration_since(window.started) < Duration::from_secs(120));
    if !limits.contains_key(&peer) && limits.len() >= MAX_RATE_IDENTITIES {
        return false;
    }
    let window = limits.entry(peer).or_insert(RateWindow {
        started: now,
        frames: 0,
        bytes: 0,
    });
    if now.duration_since(window.started) >= Duration::from_secs(60) {
        *window = RateWindow {
            started: now,
            frames: 0,
            bytes: 0,
        };
    }
    window.frames = window.frames.saturating_add(1);
    window.bytes = window.bytes.saturating_add(bytes);
    window.frames <= MAX_ROUTE_FRAMES_PER_MINUTE && window.bytes <= MAX_ROUTE_BYTES_PER_MINUTE
}

fn queue_frame(
    tx: &mpsc::Sender<QueuedFrame>,
    budget: &Arc<Semaphore>,
    bytes: Vec<u8>,
) -> Result<(), ()> {
    let permits = u32::try_from(bytes.len()).map_err(|_| ())?;
    let byte_permit = budget
        .clone()
        .try_acquire_many_owned(permits)
        .map_err(|_| ())?;
    tx.try_send(QueuedFrame {
        bytes,
        _byte_permit: byte_permit,
    })
    .map_err(|_| ())
}

async fn is_current_generation(router: &Router, peer: &[u8; 32], generation: u64) -> bool {
    router
        .lock()
        .await
        .get(peer)
        .is_some_and(|session| session.generation == generation)
}

async fn remove_if_generation(router: &Router, peer: &[u8; 32], generation: u64) {
    let mut map = router.lock().await;
    if map
        .get(peer)
        .is_some_and(|session| session.generation == generation)
    {
        map.remove(peer);
    }
}

fn verify_registration(challenge: &[u8; 32], peer_id: &[u8; 32], signature: &[u8; 64]) -> bool {
    let Ok(verifying) = VerifyingKey::from_bytes(peer_id) else {
        return false;
    };
    let signable =
        relay_protocol::registration_signable(challenge, &iris_core::message::PeerId(*peer_id));
    verifying
        .verify_strict(&signable, &Signature::from_bytes(signature))
        .is_ok()
}

async fn write_bounded<W: tokio::io::AsyncWrite + Unpin>(
    write: &mut W,
    frame: &[u8],
) -> std::io::Result<()> {
    tokio::time::timeout(WRITE_TIMEOUT, async {
        write.write_all(frame).await?;
        write.flush().await
    })
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "relay write timeout"))?
}

async fn read_frame<R: AsyncRead + Unpin>(read: &mut R, timeout: Duration) -> Option<RelayFrame> {
    let mut header = [0u8; RELAY_HEADER_LEN];
    match tokio::time::timeout(timeout, read.read_exact(&mut header)).await {
        Ok(Ok(_)) => {}
        _ => return None,
    }
    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&header[66..70]);
    let payload_len = u32::from_le_bytes(len_bytes) as usize;
    if payload_len > MAX_RELAY_PAYLOAD + relay_protocol::RELAY_SIGNATURE_LEN {
        return None;
    }
    let mut full = vec![0u8; RELAY_HEADER_LEN + payload_len];
    full[..RELAY_HEADER_LEN].copy_from_slice(&header);
    if payload_len > 0
        && !matches!(
            tokio::time::timeout(timeout, read.read_exact(&mut full[RELAY_HEADER_LEN..])).await,
            Ok(Ok(_))
        )
    {
        return None;
    }
    relay_protocol::decode(&full).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, RootCertStore};
    use tokio::net::TcpStream;
    use tokio_rustls::client::TlsStream;
    use tokio_rustls::TlsConnector;

    fn test_server_config() -> ServerConfig {
        let cert = CertificateDer::from(test_certs::TEST_RELAY_CERT_DER.to_vec());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            test_certs::TEST_RELAY_KEY_DER.to_vec(),
        ));
        let mut config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap();
        config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
        config
    }

    fn test_connector() -> TlsConnector {
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(
                test_certs::TEST_RELAY_CERT_DER.to_vec(),
            ))
            .unwrap();
        let mut config = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
        TlsConnector::from(Arc::new(config))
    }

    async fn register_test_client(
        addr: SocketAddr,
        key: &ed25519_dalek::SigningKey,
    ) -> TlsStream<TcpStream> {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let mut tls = test_connector()
            .connect(ServerName::try_from("localhost").unwrap(), tcp)
            .await
            .unwrap();
        let RelayFrame::Challenge { nonce } = read_frame(&mut tls, Duration::from_secs(2))
            .await
            .expect("relay challenge")
        else {
            panic!("expected challenge");
        };
        let peer = iris_core::message::PeerId(key.verifying_key().to_bytes());
        let signature = key
            .sign(&relay_protocol::registration_signable(&nonce, &peer))
            .to_bytes();
        let register = relay_protocol::encode(&RelayFrame::Register {
            peer_id: peer,
            signature,
        })
        .unwrap();
        write_bounded(&mut tls, &register).await.unwrap();
        assert!(matches!(
            read_frame(&mut tls, Duration::from_secs(2)).await,
            Some(RelayFrame::Heartbeat)
        ));
        tls
    }

    #[tokio::test]
    async fn actual_tls_server_authenticates_and_routes_bidirectionally() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve(
            listener,
            TlsAcceptor::from(Arc::new(test_server_config())),
            MAX_CONNECTIONS,
        ));

        let key_a = ed25519_dalek::SigningKey::from_bytes(&[0x41; 32]);
        let key_b = ed25519_dalek::SigningKey::from_bytes(&[0x42; 32]);
        let peer_a = iris_core::message::PeerId(key_a.verifying_key().to_bytes());
        let peer_b = iris_core::message::PeerId(key_b.verifying_key().to_bytes());
        let mut a = register_test_client(addr, &key_a).await;
        let mut b = register_test_client(addr, &key_b).await;

        let route_id = [0x77; 16];
        let route = relay_protocol::encode(&RelayFrame::Route {
            route_id,
            source: peer_a,
            target: peer_b,
            payload: b"real relay e2e".to_vec(),
        })
        .unwrap();
        write_bounded(&mut a, &route).await.unwrap();

        assert_eq!(
            read_frame(&mut b, Duration::from_secs(2)).await,
            Some(RelayFrame::Route {
                route_id,
                source: peer_a,
                target: peer_b,
                payload: b"real relay e2e".to_vec(),
            })
        );
        assert_eq!(
            read_frame(&mut a, Duration::from_secs(2)).await,
            Some(RelayFrame::Ack {
                route_id,
                target: peer_b,
                status: RelayAckStatus::ForwardedToLiveSession,
            })
        );

        let return_id = [0x78; 16];
        let response = relay_protocol::encode(&RelayFrame::Route {
            route_id: return_id,
            source: peer_b,
            target: peer_a,
            payload: b"return path".to_vec(),
        })
        .unwrap();
        write_bounded(&mut b, &response).await.unwrap();
        assert!(matches!(
            read_frame(&mut a, Duration::from_secs(2)).await,
            Some(RelayFrame::Route { route_id, source, target, .. })
                if route_id == return_id && source == peer_b && target == peer_a
        ));
        assert!(matches!(
            read_frame(&mut b, Duration::from_secs(2)).await,
            Some(RelayFrame::Ack { route_id, status: RelayAckStatus::ForwardedToLiveSession, .. })
                if route_id == return_id
        ));
        server.abort();
    }

    #[test]
    fn registration_proof_is_bound_to_challenge_and_peer() {
        let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
        let peer_id = key.verifying_key().to_bytes();
        let challenge = [9; 32];
        let signable =
            relay_protocol::registration_signable(&challenge, &iris_core::message::PeerId(peer_id));
        let signature = ed25519_dalek::Signer::sign(&key, &signable).to_bytes();
        assert!(verify_registration(&challenge, &peer_id, &signature));
        assert!(!verify_registration(&[8; 32], &peer_id, &signature));

        let other = ed25519_dalek::SigningKey::from_bytes(&[6; 32])
            .verifying_key()
            .to_bytes();
        assert!(!verify_registration(&challenge, &other, &signature));
    }

    #[tokio::test]
    async fn stale_generation_cannot_remove_replacement_session() {
        let router: Router = Arc::new(Mutex::new(HashMap::new()));
        let (old_tx, _old_rx) = mpsc::channel(1);
        let (new_tx, _new_rx) = mpsc::channel(1);
        let peer = [3; 32];
        router.lock().await.insert(
            peer,
            Session {
                generation: 1,
                tx: old_tx,
            },
        );
        router.lock().await.insert(
            peer,
            Session {
                generation: 2,
                tx: new_tx,
            },
        );

        let mut map = router.lock().await;
        if map
            .get(&peer)
            .is_some_and(|session| session.generation == 1)
        {
            map.remove(&peer);
        }
        assert_eq!(map.get(&peer).map(|session| session.generation), Some(2));
    }

    #[test]
    fn per_ip_admission_is_bounded_and_released() {
        let counts: IpCounts = Arc::new(StdMutex::new(HashMap::new()));
        let ip: IpAddr = "192.0.2.1".parse().unwrap();
        let limit = 8;
        let guards: Vec<_> = (0..limit)
            .map(|_| IpConnectionGuard::try_acquire(ip, counts.clone(), limit).unwrap())
            .collect();
        assert!(IpConnectionGuard::try_acquire(ip, counts.clone(), limit).is_none());
        drop(guards);
        assert!(IpConnectionGuard::try_acquire(ip, counts.clone(), limit).is_some());
    }

    #[tokio::test]
    async fn queue_is_bounded_by_slots_and_releases_byte_budget() {
        let budget = Arc::new(Semaphore::new(128));
        let (tx, mut rx) = mpsc::channel(1);
        queue_frame(&tx, &budget, vec![1; 64]).unwrap();
        assert_eq!(budget.available_permits(), 64);
        assert!(queue_frame(&tx, &budget, vec![2; 1]).is_err());
        drop(rx.recv().await.unwrap());
        assert_eq!(budget.available_permits(), 128);
    }

    #[tokio::test]
    async fn identity_rate_limit_survives_reconnect() {
        let limits: RateLimits = Arc::new(Mutex::new(HashMap::new()));
        let peer = [9; 32];
        for _ in 0..MAX_ROUTE_FRAMES_PER_MINUTE {
            assert!(charge_route(&limits, peer, 1).await);
        }
        // A new socket calls the same identity-scoped table; it cannot reset
        // the budget by reconnecting.
        assert!(!charge_route(&limits, peer, 1).await);
        assert!(charge_route(&limits, [8; 32], 1).await);
    }
}
