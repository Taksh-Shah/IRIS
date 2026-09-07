//! IRIS relay server — routes [`RelayFrame`] messages between registered clients.
//!
//! # Usage
//!
//! ```text
//! relay_server [ADDR:PORT]          (default: 0.0.0.0:7890)
//! ```
//!
//! # Protocol
//!
//! Each client connects and immediately sends `RelayFrame::Register { peer_id }`.
//! The relay then accepts `RelayFrame::Route { source, target, payload }` from any
//! registered client and forwards the full frame to the target's write half.
//! Unregistered senders and unknown targets are logged and dropped.
//!
//! For software testing, connections are plain TCP. A production deployment should
//! add tokio-rustls on the accept path and supply a real server certificate.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use iris_core::transport::relay_protocol::{
    self, RelayFrame, RELAY_HEADER_LEN, MAX_RELAY_PAYLOAD,
};

/// Shared routing table: PeerId → write half of the registered connection.
type Router = Arc<Mutex<HashMap<[u8; 32], tokio::net::tcp::OwnedWriteHalf>>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("relay_server=info".parse().unwrap()),
        )
        .init();

    let addr: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:7890".into())
        .parse()
        .expect("ADDR:PORT argument");

    let listener = TcpListener::bind(addr).await.expect("bind relay listener");
    let bound = listener.local_addr().unwrap();
    tracing::info!("IRIS relay listening on {bound}");

    let router: Router = Arc::new(Mutex::new(HashMap::new()));

    loop {
        match listener.accept().await {
            Ok((stream, peer_addr)) => {
                tracing::info!(peer_addr = %peer_addr, "new connection");
                let r = router.clone();
                tokio::spawn(handle_client(stream, r));
            }
            Err(e) => {
                tracing::error!("accept error: {e}");
                break;
            }
        }
    }
}

async fn handle_client(stream: TcpStream, router: Router) {
    stream.set_nodelay(true).ok();
    let peer_addr = stream.peer_addr().ok();
    let (mut read, write) = stream.into_split();

    // First frame MUST be Register.
    let Some(frame) = read_frame(&mut read).await else {
        tracing::warn!(?peer_addr, "client disconnected before registering");
        return;
    };
    let registered_id = match frame {
        RelayFrame::Register { peer_id } => peer_id.0,
        _ => {
            tracing::warn!(?peer_addr, "first frame was not Register — dropping");
            return;
        }
    };
    tracing::info!(
        ?peer_addr,
        peer_id = hex::encode(registered_id),
        "client registered"
    );
    {
        let mut map = router.lock().await;
        map.insert(registered_id, write);
    }

    // Route loop.
    loop {
        let Some(frame) = read_frame(&mut read).await else {
            break;
        };
        match frame {
            RelayFrame::Route {
                source,
                target,
                payload,
            } => {
                let outbound = relay_protocol::encode(&RelayFrame::Route {
                    source,
                    target,
                    payload,
                });
                let outbound = match outbound {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!("relay encode error: {e:?}");
                        continue;
                    }
                };
                let mut map = router.lock().await;
                if let Some(w) = map.get_mut(&target.0) {
                    if w.write_all(&outbound).await.is_err() || w.flush().await.is_err() {
                        tracing::warn!(
                            target = hex::encode(target.0),
                            "write to target failed; evicting"
                        );
                        map.remove(&target.0);
                    }
                } else {
                    tracing::warn!(
                        target = hex::encode(target.0),
                        "unknown target peer — frame dropped"
                    );
                }
            }
            RelayFrame::Heartbeat => {
                // Echo heartbeat back so the client's idle timer resets.
                let mut map = router.lock().await;
                if let Some(w) = map.get_mut(&registered_id) {
                    let hb = relay_protocol::encode(&RelayFrame::Heartbeat).unwrap_or_default();
                    w.write_all(&hb).await.ok();
                    w.flush().await.ok();
                }
            }
            RelayFrame::Register { .. } => {
                tracing::warn!(?peer_addr, "unexpected Register after handshake");
            }
        }
    }

    tracing::info!(?peer_addr, "client disconnected; removing registration");
    router.lock().await.remove(&registered_id);
}

/// Read one complete RelayFrame from the stream, returning None on EOF or error.
async fn read_frame(read: &mut tokio::net::tcp::OwnedReadHalf) -> Option<RelayFrame> {
    let mut header = [0u8; RELAY_HEADER_LEN];
    match tokio::time::timeout(
        std::time::Duration::from_secs(60),
        read.read_exact(&mut header),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
        Ok(Err(e)) => {
            tracing::warn!("relay read error: {e}");
            return None;
        }
        Err(_) => {
            tracing::warn!("relay read timeout");
            return None;
        }
    }

    let mut len_bytes = [0u8; 4];
    len_bytes.copy_from_slice(&header[66..70]);
    let payload_len = u32::from_le_bytes(len_bytes) as usize;
    if payload_len > MAX_RELAY_PAYLOAD {
        tracing::warn!("relay: oversized payload {payload_len}");
        return None;
    }

    let mut full = vec![0u8; RELAY_HEADER_LEN + payload_len];
    full[..RELAY_HEADER_LEN].copy_from_slice(&header);
    if payload_len > 0 {
        if read.read_exact(&mut full[RELAY_HEADER_LEN..]).await.is_err() {
            return None;
        }
    }

    relay_protocol::decode(&full).ok()
}
