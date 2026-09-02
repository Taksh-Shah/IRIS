//! Discovery engine (layer 6) — DISCO-001.
//!
//! Transport-agnostic neighbor discovery. `DiscoveryManager` drives
//! `Transport::discover_peers()` across all registered transports, dedupes by
//! `peer_id`, feeds the `NeighborTable`, and emits `TopologyEvent`s on a
//! broadcast channel. First contact triggers a CAPABILITY + Bloom handshake
//! (`handshake` module) so peers record each other's capabilities and dedup
//! filters.
//!
//! Real BLE/Wi-Fi-Aware adapters are deferred (BLK-0005); this engine runs
//! against any `Transport` implementation (`SimulatedTransport`,
//! `InternetTransport`) — see `DiscoveryManager::scan_once`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use tokio::sync::RwLock;

use crate::message::MessagePriority::P2;
use crate::message::{DiscoveryConfig as ScanConfig, LinkQuality, PeerId, PeerInfo};
use crate::transport::{TopologyEvent, Transport, TransportId};
use crate::TransportError;

pub mod handshake;
pub mod neighbor_table;

use handshake::{parse_handshake, HandshakeMessage};
use neighbor_table::NeighborTable;

/// Discovery run mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiscoveryMode {
    /// Fast, frequent scans — use when actively looking for low-power peers.
    Active = 0,
    /// Periodic scans at a slow interval — battery-friendly baseline.
    Passive = 1,
}

impl DiscoveryMode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
    pub fn from_u8(v: u8) -> Option<DiscoveryMode> {
        Some(match v {
            0 => DiscoveryMode::Active,
            1 => DiscoveryMode::Passive,
            _ => return None,
        })
    }
}

/// Configuration for the discovery engine.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Interval between full scan rounds when the mesh is quiescent (at least
    /// one live neighbour link) — battery-friendly baseline.
    pub scan_interval: Duration,
    /// HV-91: interval between scan rounds while there is **no** live link
    /// (cold start / a peer just dropped). Short so the first link forms in
    /// seconds, not `scan_interval`. Kept above the BLE `scan_allowed` throttle
    /// budget (5 starts / 30 s) so a fast cadence never trips it.
    pub active_scan_interval: Duration,
    /// Neighbor idle-TTL before eviction.
    pub neighbor_ttl: Duration,
    /// Per-scan timeout handed to `Transport::discover_peers`.
    pub scan_timeout: Duration,
    /// Max peers reported per scan.
    pub max_peers: usize,
    /// Capability tags advertised by this node.
    pub capabilities: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            scan_interval: Duration::from_secs(30),
            // 3 s: harvest-only passes cost the radio nothing (HV-91 changed
            // `discover_peers` to re-arm the scan only every ~5 passes), so a
            // fast cold-start cadence never trips the scan-restart throttle.
            active_scan_interval: Duration::from_secs(3),
            neighbor_ttl: Duration::from_secs(300),
            scan_timeout: Duration::from_secs(10),
            max_peers: 64,
            capabilities: Vec::new(),
        }
    }
}

/// The transport-agnostic discovery engine (DISCO-001).
pub struct DiscoveryManager {
    node_id: PeerId,
    config: DiscoveryConfig,
    table: Arc<NeighborTable>,
    events: broadcast::Sender<TopologyEvent>,
    transports: RwLock<Vec<Arc<dyn Transport>>>,
    mode: AtomicU8,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    // HW-18: per-(transport, peer) exponential backoff after repeated
    // connect() failures. Without this, a peer that keeps failing to
    // connect (e.g. Wi-Fi Direct with an active home-AP STA connection on
    // single-radio hardware, where every connect() attempt makes Android's
    // own WifiP2pService show a real STA-vs-P2P radio-conflict dialog —
    // "Turn off Sharing?" on Samsung builds) gets re-attempted every single
    // scan pass forever, which on that hardware re-surfaces the OS prompt
    // just as often. This cannot make the underlying radio conflict go
    // away (that's a genuine platform limitation, not a bug — see HW-18's
    // own doc), but it turns "every scan pass" into "increasingly rare"
    // after the first couple of failures, which is the only thing in this
    // codebase's control.
    connect_backoff: tokio::sync::Mutex<HashMap<(TransportId, PeerId), ConnectBackoff>>,
    /// HV-15: every `PeerInfo` we have ever tried to connect (its transport
    /// addresses included). When a GATT link drops, its beacon may not be in
    /// the *current* scan buffer for several passes — but `connect()` only
    /// needs the cached address, not a fresh scan. `scan_once` re-attempts
    /// `connect()` for these when there is no live link, so a drop heals in a
    /// scan interval instead of waiting for the beacon to be re-harvested.
    /// Bounded at `2 * max_peers`.
    known_peers: tokio::sync::Mutex<HashMap<PeerId, PeerInfo>>,
    /// HV-91: true once at least one `connect()` has succeeded and the peer has
    /// not since been evicted. Drives the scan cadence — fast (`active_scan_
    /// interval`) until there is a real link, then `scan_interval`. This is a
    /// *connectivity* signal, distinct from `NeighborTable`'s `LinkedUp` which
    /// only means "beacon seen recently" (HV-27).
    has_confirmed_link: std::sync::atomic::AtomicBool,
}

/// HW-18: backoff state for one (transport, peer) pair. `next_attempt` gates
/// whether `scan_once` bothers calling `connect()` at all this pass;
/// `failures` drives the doubling.
struct ConnectBackoff {
    next_attempt: Instant,
    failures: u32,
}

/// First retry stays at the normal scan cadence (no perceptible behavior
/// change on a single transient failure); only a second consecutive failure
/// starts backing off, doubling up to `CONNECT_BACKOFF_MAX`.
const CONNECT_BACKOFF_BASE: Duration = Duration::from_secs(30);
const CONNECT_BACKOFF_MAX: Duration = Duration::from_secs(300);

impl DiscoveryManager {
    pub fn new(node_id: PeerId, config: DiscoveryConfig) -> Self {
        let (events, _) = broadcast::channel(256);
        DiscoveryManager {
            node_id,
            table: Arc::new(NeighborTable::new(config.neighbor_ttl)),
            events,
            transports: RwLock::new(Vec::new()),
            mode: AtomicU8::new(DiscoveryMode::Passive as u8),
            task: tokio::sync::Mutex::new(None),
            connect_backoff: tokio::sync::Mutex::new(HashMap::new()),
            known_peers: tokio::sync::Mutex::new(HashMap::new()),
            has_confirmed_link: std::sync::atomic::AtomicBool::new(false),
            config,
        }
    }

    /// Register a transport to scan. Takes effect on the next scan round.
    pub async fn register_transport(&self, transport: Arc<dyn Transport>) {
        self.transports.write().await.push(transport);
    }

    pub async fn deregister_transport(&self, id: &TransportId) {
        self.transports
            .write()
            .await
            .retain(|t| t.transport_id() != id);
    }

    pub fn mode(&self) -> DiscoveryMode {
        DiscoveryMode::from_u8(self.mode.load(Ordering::Relaxed)).unwrap_or(DiscoveryMode::Passive)
    }

    pub fn set_mode(&self, mode: DiscoveryMode) {
        self.mode.store(mode.as_u8(), Ordering::Relaxed);
    }

    /// Reactive API: a transport may push a discovered peer without the poll
    /// loop (e.g. an event-driven BLE callback later). Dedup + emit handled
    /// identically to the scan path.
    pub async fn report_peer(
        &self,
        peer: &crate::message::PeerInfo,
        via: &TransportId,
        quality: LinkQuality,
    ) {
        if let Some(ev) = self.table.upsert(peer, via, quality).await {
            self.events.send(ev).ok();
        }
    }

    /// Ingest an inbound handshake envelope (CAPABILITY bundle or Bloom
    /// exchange). Updates the neighbor table with the peer's capabilities and
    /// records its dedup Bloom filter for the dispatch layer.
    /// The claimed node id must be the envelope's own authenticated sender.
    ///
    /// Without this binding, the table was keyed on an id read out of the
    /// PAYLOAD while the envelope came from somebody else — so any peer able to
    /// deliver one Capability envelope could rewrite an arbitrary victim's
    /// capabilities (claiming `gateway` to attract routing, say) or plant a
    /// hostile Bloom filter that makes the dispatch layer suppress every real
    /// message to that peer.
    fn sender_matches(env: &crate::protocol::Envelope, claimed: &PeerId) -> bool {
        env.sender_id.len() == 32 && env.sender_id.as_slice() == claimed.as_bytes()
    }

    pub async fn ingest_handshake(&self, env: &crate::protocol::Envelope) {
        // Handshakes mutate routing-visible state, so they must be
        // authenticated first. An unsigned handshake is not ingestable.
        if env.signature.is_none() {
            return;
        }
        let Ok(msg) = parse_handshake(env) else {
            return;
        };
        match msg {
            HandshakeMessage::Capability(bundle) => {
                let node_id = bundle.node_id;
                if !Self::sender_matches(env, &node_id) {
                    return;
                }
                self.table.set_capabilities(&node_id, bundle).await;
            }
            HandshakeMessage::BloomExchange(exchange) => {
                if !Self::sender_matches(env, &exchange.node_id) {
                    return;
                }
                if let Some(filter) = exchange.into_filter() {
                    self.table.set_peer_bloom(exchange.node_id, filter).await;
                }
            }
        }
    }

    /// Run one scan round: for each registered transport, call
    /// `discover_peers`, upsert each peer into the neighbor table, and emit
    /// topology events. Begins the handshake for newly discovered peers.
    ///
    /// PS-5 fix: transport scans run concurrently via `join_all`. All shared
    /// state (NeighborTable, connect_backoff) uses async locks and is safe for
    /// concurrent access. Radio contention between BLE/WiFi is handled by the
    /// OS arbiter — the worst case equals the prior serial path; independent
    /// transports (internet, LoRa, satellite) benefit immediately.
    pub async fn scan_once(&self) {
        let transports = self.transports.read().await.clone();
        let per_transport: Vec<(Vec<PeerId>, Vec<PeerId>)> =
            futures_util::future::join_all(transports.iter().map(|t| self.scan_transport(t)))
                .await;
        let mut new_peers: Vec<PeerId> = Vec::new();
        let mut seen_peers: Vec<PeerId> = Vec::new();
        for (new, seen) in per_transport {
            new_peers.extend(new);
            seen_peers.extend(seen);
        }
        // HV-14: the scan cadence is driven by `has_confirmed_link`. Recompute it
        // from *actual* transport state every pass, not just "a connect() has
        // ever succeeded" — so when `close_peer` drops the last live GATT link
        // (transport falls back to `Available`) the very next pass resumes the
        // fast `active_scan_interval` instead of waiting the 300 s neighbor TTL.
        // The targeted reconnect (HV-15) is the fast path; this keeps discovery
        // scanning fast as the backstop while it retries.
        let any_connected = transports
            .iter()
            .any(|t| t.state() == crate::transport::TransportState::Connected);
        self.has_confirmed_link
            .store(any_connected, std::sync::atomic::Ordering::Relaxed);
        // HV-15: with no live link, re-attempt `connect()` for every known peer
        // whose beacon was NOT harvested this pass — a dropped GATT link's
        // beacon can be out of the scan buffer for several passes, but the
        // cached address is all `connect()` needs. Skipped while a link is up
        // (the mesh is working; the per-beacon path covers the rest).
        if !any_connected {
            let seen: std::collections::HashSet<PeerId> = seen_peers.iter().copied().collect();
            // Only retry a known peer that is still in the neighbor table
            // (i.e. its beacon was seen within `neighbor_ttl` — the peer is
            // around, the link just dropped) — not one that has genuinely gone
            // away. `connect_backoff` bounds the wasted `connect()` blocking
            // after the 2nd consecutive failure. Cap per pass so one flapping
            // peer cannot stall the loop.
            let mut stale: Vec<PeerInfo> = Vec::new();
            {
                let known = self.known_peers.lock().await;
                for p in known.values() {
                    if seen.contains(&p.peer_id) || stale.len() >= 3 {
                        continue;
                    }
                    if self.table.get(&p.peer_id).await.is_some() {
                        stale.push(p.clone());
                    }
                }
            }
            for peer in &stale {
                for t in &transports {
                    self.attempt_connect(t, peer).await;
                }
            }
        }
        // First-contact handshake for peers never seen before.
        for peer_id in new_peers {
            self.run_handshake(peer_id).await;
        }
        // TTL eviction pass.
        for ev in self.table.sweep().await {
            if let TopologyEvent::PeerLost { peer_id, .. } = &ev {
                tracing::info!(
                    event = "discovery.peer_lost",
                    peer = %peer_id,
                    "peer evicted from neighbor table (TTL expired)"
                );
                // HV-91: a lost peer means we may have no link — resume fast
                // scanning until a connect succeeds again.
                self.has_confirmed_link
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            self.events.send(ev).ok();
        }
    }

    /// Scan one transport: discover peers, upsert into the neighbor table,
    /// attempt connect for each visible peer, emit topology events.
    /// Returns the peer ids of newly discovered (never-seen-before) peers.
    async fn scan_transport(&self, transport: &Arc<dyn Transport>) -> (Vec<PeerId>, Vec<PeerId>) {
        let scan_cfg = ScanConfig {
            timeout: self.config.scan_timeout,
            max_peers: self.config.max_peers,
            filter: None,
        };
        let mut stream = match transport.discover_peers(scan_cfg).await {
            Ok(s) => s,
            Err(e) => {
                // HW-3: silent `continue` used to leave no trace when a
                // transport failed to start a scan (e.g. NotSupported).
                tracing::debug!(
                    event = "discovery.discover_peers_failed",
                    transport = %transport.transport_id(),
                    error = %e,
                    "discover_peers() failed for this pass; skipping this transport this round"
                );
                return (Vec::new(), Vec::new());
            }
        };
        let mut new_peers = Vec::new();
        let mut seen_ids: Vec<PeerId> = Vec::new();
        let mut seen_this_pass = 0usize;
        while let Some(peer) = futures_util::StreamExt::next(&mut stream).await {
            seen_this_pass += 1;
            seen_ids.push(peer.peer_id);
            let ev = self
                .table
                .upsert(&peer, transport.transport_id(), LinkQuality::Good)
                .await;
            if matches!(ev, Some(TopologyEvent::PeerDiscovered { .. })) {
                new_peers.push(peer.peer_id);
            }
            // HV-15: remember this peer (address included) so `scan_once` can
            // re-attempt `connect()` even on passes that do not re-harvest its
            // beacon.
            {
                let mut known = self.known_peers.lock().await;
                if known.len() >= self.config.max_peers.saturating_mul(2)
                    && !known.contains_key(&peer.peer_id)
                {
                    // Bounded: drop an arbitrary entry (churn guard, not LRU —
                    // a genuinely active peer is re-inserted next pass anyway).
                    if let Some(k) = known.keys().next().copied() {
                        known.remove(&k);
                    }
                }
                known.insert(peer.peer_id, peer.clone());
            }
            // HW-3: attempt connect unconditionally each pass — `connect()`
            // is idempotent/no-op when the link already exists (AC-9), so
            // this retries dropped links without requiring a re-discovery.
            self.attempt_connect(transport, &peer).await;
            if let Some(ev) = ev {
                self.events.send(ev).ok();
            }
        }
        tracing::debug!(
            event = "discovery.scan_pass_complete",
            transport = %transport.transport_id(),
            peers_seen = seen_this_pass,
            "discovery pass complete for this transport"
        );
        (new_peers, seen_ids)
    }

    /// One backoff-gated `connect()` attempt for `peer` over `transport`.
    /// `connect()` is idempotent (a live link is a no-op), so this is the
    /// single reconnect path — used both for freshly-scanned peers and, via
    /// `scan_once`, for a known peer whose link dropped but whose beacon is not
    /// in the current scan buffer (HV-15).
    async fn attempt_connect(&self, transport: &Arc<dyn Transport>, peer: &PeerInfo) {
        let backoff_key = (transport.transport_id().clone(), peer.peer_id);
        if let Some(state) = self.connect_backoff.lock().await.get(&backoff_key) {
            if Instant::now() < state.next_attempt {
                tracing::debug!(
                    event = "discovery.connect_backoff_skip",
                    peer = %peer.peer_id,
                    transport = %transport.transport_id(),
                    failures = state.failures,
                    "skipping connect() this pass — backing off after repeated failures"
                );
                return;
            }
        }
        let connect_started = std::time::Instant::now();
        match transport.connect(peer).await {
            Ok(_) => {
                self.connect_backoff.lock().await.remove(&backoff_key);
                self.has_confirmed_link
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                tracing::debug!(
                    event = "discovery.connect_ok",
                    peer = %peer.peer_id,
                    transport = %transport.transport_id(),
                    elapsed_ms = connect_started.elapsed().as_millis() as u64,
                    "link to known peer confirmed live (idempotent no-op if already connected)"
                );
            }
            Err(e) => {
                let mut backoff = self.connect_backoff.lock().await;
                let failures = backoff.get(&backoff_key).map_or(0, |s| s.failures) + 1;
                // First failure keeps the normal scan cadence;
                // second+ consecutive failure starts doubling.
                if failures >= 2 {
                    let delay = CONNECT_BACKOFF_BASE
                        .saturating_mul(1 << (failures - 2).min(4))
                        .min(CONNECT_BACKOFF_MAX);
                    backoff.insert(
                        backoff_key.clone(),
                        ConnectBackoff { next_attempt: Instant::now() + delay, failures },
                    );
                } else {
                    backoff.insert(
                        backoff_key.clone(),
                        ConnectBackoff { next_attempt: Instant::now(), failures },
                    );
                }
                drop(backoff);
                tracing::warn!(
                    event = "discovery.connect_failed",
                    peer = %peer.peer_id,
                    transport = %transport.transport_id(),
                    error = %e,
                    elapsed_ms = connect_started.elapsed().as_millis() as u64,
                    "failed to establish/confirm link with peer this scan pass — will retry next pass"
                );
            }
        }
    }

    /// Exchange capabilities + dedup Bloom with a peer. Builds the handshake
    /// envelopes; delivery is asynchronous — the remote side ingests via
    /// `ingest_handshake`. Sent over every transport that links to the peer
    /// (ROUTE-001 later selects the optimal one).
    async fn run_handshake(&self, peer_id: PeerId) {
        let transports = self.transports.read().await.clone();
        if transports.is_empty() {
            return;
        }
        let bloom = crate::message_engine::dedup::BloomFilter::default();
        let (cap_env, bloom_env) = match handshake::build_handshake(
            self.node_id,
            self.config.capabilities.clone(),
            transports
                .iter()
                .map(|t| t.transport_id().as_str().to_string())
                .collect(),
            &bloom,
            &peer_id,
        ) {
            Ok(pair) => pair,
            Err(_) => return,
        };
        for transport in &transports {
            Self::try_send(transport, &peer_id, &cap_env).await.ok();
            Self::try_send(transport, &peer_id, &bloom_env).await.ok();
        }
    }

    /// Best-effort send of a handshake envelope over a transport.
    async fn try_send(
        transport: &Arc<dyn Transport>,
        peer: &PeerId,
        env: &crate::protocol::Envelope,
    ) -> Result<(), TransportError> {
        let payload = match crate::protocol::codec::encode(env) {
            Ok(p) => p,
            Err(_) => return Ok(()), // encode failure: skip, not a transport error
        };
        let msg = crate::message::SerializedMessage {
            message_id: env.message_id,
            priority: P2,
            payload,
        };
        transport.send(peer, &msg).await.map(|_| ())
    }

    /// Start the background discovery loop. Idempotent. The loop runs until
    /// [`DiscoveryManager::stop`] aborts it.
    pub async fn start(self: &Arc<Self>) {
        let mut guard = self.task.lock().await;
        if guard.is_some() {
            return;
        }
        let this = self.clone();
        let handle = tokio::spawn(async move {
            loop {
                this.scan_once().await;
                // HV-91: scan fast until a `connect()` has actually succeeded,
                // then back off. A missed first scan must not cost a full
                // `scan_interval` (30 s) — that was "the first message takes
                // ~30 s". `NeighborTable::LinkedUp` is not enough (it means
                // "beacon seen", HV-27) — gate on `has_confirmed_link`.
                let linked = this
                    .has_confirmed_link
                    .load(std::sync::atomic::Ordering::Relaxed);
                let sleep_for = if linked && this.mode() != DiscoveryMode::Active {
                    this.config.scan_interval
                } else {
                    this.config.active_scan_interval
                };
                tokio::time::sleep(sleep_for).await;
            }
        });
        *guard = Some(handle);
    }

    pub async fn stop(&self) {
        if let Some(handle) = self.task.lock().await.take() {
            handle.abort();
        }
    }

    /// Broadcast channel for `TopologyEvent`s.
    pub fn events(&self) -> broadcast::Receiver<TopologyEvent> {
        self.events.subscribe()
    }

    /// Shared neighbor table (routing queries ride on this).
    pub fn neighbors(&self) -> Arc<NeighborTable> {
        self.table.clone()
    }

    pub fn node_id(&self) -> PeerId {
        self.node_id
    }
}

/// Re-export for consumers of the discovery API.
pub use handshake::HandshakeError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MessagePriority, PeerInfo, SerializedMessage};
    use crate::message_engine::dedup::BloomFilter;
    use crate::protocol::codec;
    use crate::transport::simulated::{SimConfig, SimulatedTransport};
    use crate::transport::TransportManager;
    use futures_util::StreamExt;

    const ALICE: [u8; 32] = [0xAA; 32];
    const BOB: [u8; 32] = [0xBB; 32];

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn manager(id: u8) -> Arc<DiscoveryManager> {
        Arc::new(DiscoveryManager::new(
            pid(id),
            DiscoveryConfig {
                scan_interval: Duration::from_millis(20),
                active_scan_interval: Duration::from_millis(20),
                neighbor_ttl: Duration::from_secs(60),
                scan_timeout: Duration::from_millis(100),
                max_peers: 64,
                capabilities: vec!["relay".into()],
            },
        ))
    }

    fn peer_info(id: u8) -> PeerInfo {
        PeerInfo {
            peer_id: pid(id),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    #[test]
    fn mode_round_trips() {
        assert_eq!(DiscoveryMode::from_u8(0), Some(DiscoveryMode::Active));
        assert_eq!(DiscoveryMode::from_u8(1), Some(DiscoveryMode::Passive));
        assert_eq!(DiscoveryMode::from_u8(2), None);
        assert_eq!(DiscoveryMode::Active.as_u8(), 0);
    }

    #[tokio::test]
    async fn scan_without_transports_is_noop() {
        let m = manager(1);
        m.scan_once().await;
        assert_eq!(m.neighbors().len().await, 0);
    }

    /// HV-91: with no live link the background loop scans at
    /// `active_scan_interval`, not the 30 s `scan_interval` — so a missed first
    /// scan costs seconds, not half a minute. Here `connect()` always fails, so
    /// no link ever forms and the loop must keep scanning fast: many
    /// `connect()` attempts in a short window.
    #[tokio::test]
    async fn hv91_cold_start_loop_scans_at_active_interval() {
        let m = Arc::new(DiscoveryManager::new(
            pid(1),
            DiscoveryConfig {
                scan_interval: Duration::from_secs(40),
                active_scan_interval: Duration::from_millis(20),
                neighbor_ttl: Duration::from_secs(60),
                scan_timeout: Duration::from_millis(5),
                max_peers: 8,
                capabilities: vec![],
            },
        ));
        let sim = crate::transport::simulated::SimulatedTransport::new(
            "sim-hv91",
            "Sim HV-91",
            SimConfig { connect_failure_rate: 1.0, ..SimConfig::default() },
        );
        let t = Arc::new(CountingConnectTransport {
            inner: sim,
            connect_calls: std::sync::atomic::AtomicUsize::new(0),
            scan_calls: std::sync::atomic::AtomicUsize::new(0),
            seeded_peer: peer_info(9),
        });
        m.register_transport(t.clone()).await;
        m.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        m.stop().await;

        let scans = t.scan_calls.load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            scans >= 4,
            "unlinked loop must scan at active_scan_interval — only {scans} \
             scan passes in 300 ms (a 40 s scan_interval would give 1)"
        );
    }

    #[tokio::test]
    async fn report_peer_populates_table_and_emits_event() {
        let m = manager(1);
        let mut rx = m.events();
        let peer = peer_info(9);
        m.report_peer(&peer, &TransportId::from("sim"), LinkQuality::Excellent)
            .await;
        assert_eq!(m.neighbors().len().await, 1);
        assert!(m.neighbors().is_up(&peer.peer_id).await);
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::PeerDiscovered { .. }));
        assert_eq!(
            m.neighbors()
                .transports_to(&peer.peer_id)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn neighbor_lifecycle_up_update_down() {
        let m = manager(2);
        let mut rx = m.events();
        let peer = peer_info(7);
        m.report_peer(&peer, &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::PeerDiscovered { .. }));
        // Quality update on the same transport emits LinkQualityUpdated.
        m.report_peer(&peer, &TransportId::from("sim-a"), LinkQuality::Fair)
            .await;
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::LinkQualityUpdated { .. }));
        // Down.
        let ev = m
            .neighbors()
            .mark_down(&peer.peer_id, &TransportId::from("sim-a"))
            .await;
        assert!(matches!(ev, Some(TopologyEvent::PeerLost { .. })));
        assert!(!m.neighbors().is_up(&peer.peer_id).await);
    }

    /// HW-3 regression fixture: forwards every `Transport` method to an inner
    /// `SimulatedTransport` EXCEPT `discover_peers` (the real one always
    /// returns an empty stream — nothing to seed it with — so this yields a
    /// fixed injected peer on every call, matching real `discover_peers()`
    /// semantics: it re-yields every currently-visible peer on every scan
    /// pass, not just new ones) and `connect` (counted, so the test can
    /// observe how many times `scan_once()` actually attempted it).
    struct CountingConnectTransport {
        inner: crate::transport::simulated::SimulatedTransport,
        connect_calls: std::sync::atomic::AtomicUsize,
        scan_calls: std::sync::atomic::AtomicUsize,
        seeded_peer: PeerInfo,
    }

    impl Default for CountingConnectTransport {
        fn default() -> Self {
            Self {
                inner: crate::transport::simulated::SimulatedTransport::new(
                    "sim-count",
                    "Sim Count",
                    SimConfig::default(),
                ),
                connect_calls: std::sync::atomic::AtomicUsize::new(0),
                scan_calls: std::sync::atomic::AtomicUsize::new(0),
                seeded_peer: peer_info(9),
            }
        }
    }

    #[async_trait::async_trait]
    impl Transport for CountingConnectTransport {
        fn transport_id(&self) -> &TransportId {
            self.inner.transport_id()
        }
        fn display_name(&self) -> &str {
            self.inner.display_name()
        }
        fn capabilities(&self) -> &crate::transport::TransportCapabilities {
            self.inner.capabilities()
        }
        fn state(&self) -> crate::transport::TransportState {
            self.inner.state()
        }
        fn state_stream(
            &self,
        ) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = crate::transport::TransportStateEvent> + Send>>
        {
            self.inner.state_stream()
        }
        async fn discover_peers(
            &self,
            _config: ScanConfig,
        ) -> Result<std::pin::Pin<Box<dyn futures_util::Stream<Item = PeerInfo> + Send>>, TransportError>
        {
            self.scan_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(Box::pin(futures_util::stream::iter(vec![
                self.seeded_peer.clone()
            ])))
        }
        async fn stop_discovery(&self) -> Result<(), TransportError> {
            self.inner.stop_discovery().await
        }
        async fn start_advertising(
            &self,
            info: crate::message::NodeAdvertisement,
        ) -> Result<(), TransportError> {
            self.inner.start_advertising(info).await
        }
        async fn stop_advertising(&self) -> Result<(), TransportError> {
            self.inner.stop_advertising().await
        }
        async fn connect(
            &self,
            peer: &PeerInfo,
        ) -> Result<crate::message::TransportLink, TransportError> {
            self.connect_calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.inner.connect(peer).await
        }
        async fn send(
            &self,
            peer: &PeerId,
            message: &SerializedMessage,
        ) -> Result<crate::message::SendReceipt, TransportError> {
            self.inner.send(peer, message).await
        }
        fn incoming_messages(
            &self,
        ) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = crate::message::IncomingMessage> + Send>>
        {
            self.inner.incoming_messages()
        }
        fn cost_snapshot(&self) -> crate::transport::TransportCost {
            self.inner.cost_snapshot()
        }
        fn set_send_priority_hint(&self, priority: MessagePriority) {
            self.inner.set_send_priority_hint(priority)
        }
        async fn shutdown(&self) -> Result<(), TransportError> {
            self.inner.shutdown().await
        }
    }

    #[tokio::test]
    async fn hw3_scan_reconnects_an_already_known_peer_every_pass() {
        // HW-3: found on real hardware — scan_once() used to gate connect()
        // behind `PeerDiscovered` alone, so a peer seen once (first scan)
        // never got a SECOND connect() attempt on any later scan pass, even
        // though discover_peers() re-yields it every pass. A link that
        // dropped after the first connect — confirmed live via real BLE GATT
        // cycling — was never re-established: nothing else in the engine
        // ever calls connect(). This test proves the peer gets a fresh
        // connect() attempt on the SECOND scan pass too, not just the first.
        let m = manager(5);
        let sim = crate::transport::simulated::SimulatedTransport::new(
            "sim-hw3",
            "Sim HW-3",
            SimConfig::default(),
        );
        let t = Arc::new(CountingConnectTransport {
            inner: sim,
            connect_calls: std::sync::atomic::AtomicUsize::new(0),
            scan_calls: std::sync::atomic::AtomicUsize::new(0),
            seeded_peer: peer_info(9),
        });
        m.register_transport(t.clone()).await;

        m.scan_once().await;
        let after_first = t.connect_calls.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(after_first, 1, "first scan pass must connect the newly discovered peer");

        m.scan_once().await;
        let after_second = t.connect_calls.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            after_second, 2,
            "second scan pass of an ALREADY-KNOWN peer must still attempt connect() \
             (idempotent no-op if still linked, a genuine retry if the link dropped) — \
             this is the exact fix for HW-3"
        );
    }

    #[tokio::test]
    async fn background_loop_scans_periodically() {
        let m = manager(3);
        m.start().await;
        // No transports registered → loop must not spin or panic.
        tokio::time::sleep(Duration::from_millis(60)).await;
        m.stop().await;
    }

    // --- Handshake relay across SimulatedTransport (M4-style connector) ---

    #[tokio::test(flavor = "multi_thread")]
    async fn handshake_from_a_different_sender_is_refused() {
        // Regression: the table was keyed on the node id read out of the
        // PAYLOAD while the envelope came from somebody else, and nothing
        // compared the two. Any peer able to deliver one Capability envelope
        // could rewrite an arbitrary victim's capabilities — claiming
        // "gateway" to attract routing — or plant a hostile Bloom filter that
        // makes the dispatch layer suppress every real message to that peer.
        let mut bloom = BloomFilter::new(1000, 0.001);
        bloom.insert([0x11; 16]);
        let (cap_env, _bloom_env) = handshake::build_handshake(
            PeerId::from_bytes(ALICE),
            vec!["gateway".into()],
            vec!["sim-alice".into()],
            &bloom,
            &PeerId::from_bytes(BOB),
        )
        .unwrap();

        let dm = manager(0xBB);
        dm.report_peer(
            &PeerInfo {
                peer_id: PeerId::from_bytes(ALICE),
                addresses: vec![],
                transport_addresses: vec![],
                last_seen: None,
            },
            &TransportId::from("sim-alice"),
            LinkQuality::Good,
        )
        .await;

        // Same payload, but delivered by CAROL rather than ALICE.
        let mut spoofed = cap_env.clone();
        spoofed.signature = Some([0u8; 64]);
        spoofed.sender_id = vec![0xCC; 32];
        dm.ingest_handshake(&spoofed).await;

        let alice = dm.neighbors().get(&PeerId::from_bytes(ALICE)).await;
        assert!(
            alice.map(|n| n.capabilities.is_none()).unwrap_or(true),
            "a handshake whose sender is not the claimed node must be refused"
        );

        // An unsigned handshake is refused even when the sender does match.
        let mut unsigned = cap_env.clone();
        unsigned.signature = None;
        dm.ingest_handshake(&unsigned).await;
        let alice = dm.neighbors().get(&PeerId::from_bytes(ALICE)).await;
        assert!(
            alice.map(|n| n.capabilities.is_none()).unwrap_or(true),
            "an unsigned handshake must be refused"
        );
    }

    #[tokio::test]
    async fn handshake_round_trip_records_capabilities_and_bloom() {
        // Alice's side: handshake envelopes flow through the simulated
        // transport to Bob's discovery manager (M4-style connector).
        let a_transport = Arc::new(SimulatedTransport::new(
            "sim-alice",
            "Sim A",
            SimConfig {
                packet_loss_rate: 0.0,
                ..Default::default()
            },
        ));
        let a_manager = Arc::new(TransportManager::new());
        a_manager.register(a_transport.clone()).await.unwrap();
        let _ = (&a_manager, &Arc::new(TransportManager::new()));

        // Alice builds + sends the CAPABILITY and Bloom envelopes to Bob.
        let mut bloom = BloomFilter::new(1000, 0.001);
        bloom.insert([0x11; 16]);
        let (cap_env, bloom_env) = handshake::build_handshake(
            PeerId::from_bytes(ALICE),
            vec!["relay".into()],
            vec!["sim-alice".into()],
            &bloom,
            &PeerId::from_bytes(BOB),
        )
        .unwrap();
        for mut env in [cap_env, bloom_env] {
            // `ingest_handshake` only accepts an envelope that has been through
            // verification (`process_incoming` runs `crypto.verify` before any
            // discovery state is touched). Stamp a signature so this exercises
            // the authenticated path rather than bypassing it.
            env.signature = Some([0u8; 64]);
            let payload = codec::encode(&env).unwrap();
            a_transport
                .send(
                    &PeerId::from_bytes(BOB),
                    &SerializedMessage {
                        message_id: env.message_id,
                        priority: MessagePriority::P2,
                        payload,
                    },
                )
                .await
                .unwrap();
        }

        // Bob's discovery manager ingests what his transport delivers.
        let bob_dm = manager(0xBB);
        // Real flow: Alice is first discovered (upsert), then handshake lands.
        bob_dm
            .report_peer(
                &PeerInfo {
                    peer_id: PeerId::from_bytes(ALICE),
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim-alice"),
                LinkQuality::Good,
            )
            .await;
        let mut stream = a_transport.incoming_messages();
        for _ in 0..2 {
            let msg = tokio::time::timeout(Duration::from_millis(500), stream.next())
                .await
                .expect("transport delivers")
                .expect("stream item");
            let env = codec::decode(&msg.payload).unwrap();
            bob_dm.ingest_handshake(&env).await;
        }
        let nb = bob_dm.neighbors().get(&PeerId::from_bytes(ALICE)).await;
        let nb = nb.expect("capability recorded for alice");
        let caps = nb.capabilities.as_ref().expect("capabilities set");
        assert_eq!(caps.capabilities, vec!["relay".to_string()]);
        assert_eq!(caps.bloom_m, bloom.m());
        assert_eq!(caps.bloom_k, bloom.k());
        assert!(caps.transports.contains(&"sim-alice".to_string()));
        // Bloom filter recorded on the neighbor.
        let peer_bloom = bob_dm
            .neighbors()
            .peer_bloom(&PeerId::from_bytes(ALICE))
            .await;
        assert!(peer_bloom.is_some());
        assert!(peer_bloom.unwrap().contains([0x11; 16]));
    }
}
