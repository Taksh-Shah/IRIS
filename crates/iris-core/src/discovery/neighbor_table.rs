//! Neighbor table — DISCO-001.
//!
//! Registry of discovered peers with per-transport link quality, last-seen
//! timestamps, TTL-based expiry, and capability/belief records consumed by the
//! routing layer (ROUTE-001). Transport-agnostic: entries arrive from
//! `DiscoveryManager` events, not from a concrete radio.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::message::{LinkQuality, PeerId, PeerInfo};
use crate::message_engine::dedup::BloomFilter;
use crate::transport::{TopologyEvent, TransportId};

use super::handshake::CapabilityBundle;

/// Link-state of a known neighbor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeighborState {
    /// At least one transport has a live link to the peer.
    LinkedUp,
    /// All links dropped; kept for history until TTL eviction.
    LinkedDown,
}

/// Per-transport link quality record for a neighbor.
#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub transport: TransportId,
    pub quality: LinkQuality,
    pub last_seen: Instant,
}

/// A single known peer (transport-agnostic aggregate).
#[derive(Debug, Clone)]
pub struct Neighbor {
    pub peer_id: PeerId,
    pub state: NeighborState,
    /// Per-transport links. Empty means "no live link".
    pub links: Vec<LinkRecord>,
    pub first_seen: Instant,
    pub last_seen: Instant,
    /// Capabilities exchanged at first contact (DISCO-001 handshake).
    pub capabilities: Option<CapabilityBundle>,
    /// Peer's dedup Bloom filter (received in the first-contact Bloom
    /// exchange) — used by the dispatch layer to skip messages the peer has
    /// already seen (DEDUPLICATION.md).
    pub peer_bloom: Option<BloomFilter>,
}

/// Cheap projection of a [`Neighbor`] — see [`NeighborTable::neighbor_summaries`]
/// (ROUT-20). Carries `links` (needed for ROUT-21 transport selection) but
/// never `peer_bloom` or `capabilities`, the two fields that made cloning a
/// full `Neighbor` expensive.
#[derive(Debug, Clone)]
pub struct NeighborSummary {
    pub peer_id: PeerId,
    pub state: NeighborState,
    pub links: Vec<LinkRecord>,
}

/// Pick the best-quality live link's transport (ROUT-21).
///
/// `LinkQuality` is ordered `Excellent < Good < Fair < Poor` (lower =
/// better, see `routing::known_path::RoutingTable::upsert`'s established
/// convention), so the best link is the minimum, not the maximum.
pub fn best_transport(links: &[LinkRecord]) -> Option<TransportId> {
    links
        .iter()
        .min_by_key(|l| l.quality)
        .map(|l| l.transport.clone())
}

impl Neighbor {
    fn new(peer: &PeerInfo) -> Self {
        Neighbor {
            peer_id: peer.peer_id,
            state: NeighborState::LinkedUp,
            links: Vec::new(),
            first_seen: Instant::now(),
            last_seen: Instant::now(),
            capabilities: None,
            peer_bloom: None,
        }
    }
}

/// Hard ceiling on concurrent tracked neighbors.
///
/// Without a bound, a flood of distinct peer identities grows `NeighborTable`
/// at `rate × TTL × entry_size` with no ceiling. On eviction the
/// least-recently-seen neighbor is removed.
///
/// **Section 3 coordination required**: the specific cap value and eviction
/// key were chosen conservatively here; the routing layer must confirm they
/// do not break path-selection correctness before this is considered final.
pub const MAX_NEIGHBORS: usize = 512;

/// Transport-agnostic peer registry with TTL eviction and capacity cap.
///
/// All mutations return an optional `TopologyEvent` describing the resulting
/// topology change so the caller can forward it to observers (the
/// `DiscoveryManager` broadcast channel, routing, UI).
#[derive(Debug, Default)]
pub struct NeighborTable {
    inner: Mutex<HashMap<PeerId, Neighbor>>,
    /// Idle TTL before a neighbor is evicted.
    ttl: Duration,
}

impl NeighborTable {
    pub fn new(ttl: Duration) -> Self {
        NeighborTable {
            inner: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Insert or refresh a peer discovered via `transport` with `quality`.
    ///
    /// - First sight → `PeerDiscovered` (LinkedUp).
    /// - Existing peer via a new transport or improved quality →
    ///   `LinkQualityUpdated`.
    pub async fn upsert(
        &self,
        peer: &PeerInfo,
        transport: &TransportId,
        quality: LinkQuality,
    ) -> Option<TopologyEvent> {
        let mut map = self.inner.lock().await;
        let now = Instant::now();
        match map.get_mut(&peer.peer_id) {
            None => {
                // PM-5: enforce the neighbor cap before inserting a new entry.
                // Evict the least-recently-seen neighbor (LRU-by-last_seen) so
                // a flood of distinct peer identities cannot grow this table
                // without bound. Section 3 must confirm this eviction key is
                // sound for path selection.
                if map.len() >= MAX_NEIGHBORS {
                    if let Some(lru_id) = map
                        .iter()
                        .min_by_key(|(_, n)| n.last_seen)
                        .map(|(id, _)| *id)
                    {
                        map.remove(&lru_id);
                    }
                }
                let mut nb = Neighbor::new(peer);
                nb.links.push(LinkRecord {
                    transport: transport.clone(),
                    quality,
                    last_seen: now,
                });
                map.insert(peer.peer_id, nb);
                Some(TopologyEvent::PeerDiscovered {
                    peer: peer.clone(),
                    via_transport: transport.clone(),
                    link_quality: quality,
                })
            }
            Some(nb) => {
                let existing = nb.links.iter_mut().find(|l| l.transport == *transport);
                match existing {
                    Some(rec) => {
                        let changed = rec.quality != quality || nb.state != NeighborState::LinkedUp;
                        rec.quality = quality;
                        rec.last_seen = now;
                        nb.last_seen = now;
                        nb.state = NeighborState::LinkedUp;
                        if changed {
                            Some(TopologyEvent::LinkQualityUpdated {
                                peer_id: peer.peer_id,
                                transport_id: transport.clone(),
                                new_quality: quality,
                            })
                        } else {
                            None
                        }
                    }
                    None => {
                        // New transport path to an already-known peer.
                        nb.links.push(LinkRecord {
                            transport: transport.clone(),
                            quality,
                            last_seen: now,
                        });
                        nb.last_seen = now;
                        nb.state = NeighborState::LinkedUp;
                        Some(TopologyEvent::LinkQualityUpdated {
                            peer_id: peer.peer_id,
                            transport_id: transport.clone(),
                            new_quality: quality,
                        })
                    }
                }
            }
        }
    }

    /// Mark a transport link down. When the last live link is removed the
    /// neighbor transitions to `LinkedDown` and `PeerLost` is emitted.
    pub async fn mark_down(
        &self,
        peer_id: &PeerId,
        transport: &TransportId,
    ) -> Option<TopologyEvent> {
        let mut map = self.inner.lock().await;
        let nb = map.get_mut(peer_id)?;
        nb.links.retain(|l| l.transport != *transport);
        if nb.links.is_empty() {
            nb.state = NeighborState::LinkedDown;
            return Some(TopologyEvent::PeerLost {
                peer_id: *peer_id,
                via_transport: transport.clone(),
            });
        }
        None
    }

    /// Record capabilities exchanged during the first-contact handshake.
    pub async fn set_capabilities(&self, peer_id: &PeerId, caps: CapabilityBundle) {
        if let Some(nb) = self.inner.lock().await.get_mut(peer_id) {
            nb.capabilities = Some(caps);
        }
    }

    /// Record the peer's dedup Bloom filter from the first-contact exchange.
    pub async fn set_peer_bloom(&self, peer_id: PeerId, bloom: BloomFilter) {
        if let Some(nb) = self.inner.lock().await.get_mut(&peer_id) {
            nb.peer_bloom = Some(bloom);
        }
    }

    /// The peer's dedup Bloom filter, if exchanged.
    pub async fn peer_bloom(&self, peer_id: &PeerId) -> Option<BloomFilter> {
        self.inner
            .lock()
            .await
            .get(peer_id)
            .and_then(|n| n.peer_bloom.clone())
    }

    /// The transports that currently provide a live link to `peer` (used by
    /// ROUTE-001 for multi-path selection).
    pub async fn transports_to(&self, peer_id: &PeerId) -> Option<Vec<TransportId>> {
        let map = self.inner.lock().await;
        let nb = map.get(peer_id)?;
        Some(nb.links.iter().map(|l| l.transport.clone()).collect())
    }

    /// Live links to `peer` with quality preserved (ROUT-21) — cheaper than
    /// `get()` for a single-peer transport-selection lookup since it never
    /// clones `peer_bloom`/`capabilities`.
    pub async fn links_to(&self, peer_id: &PeerId) -> Option<Vec<LinkRecord>> {
        let map = self.inner.lock().await;
        map.get(peer_id).map(|n| n.links.clone())
    }

    pub async fn get(&self, peer_id: &PeerId) -> Option<Neighbor> {
        self.inner.lock().await.get(peer_id).cloned()
    }

    pub async fn is_up(&self, peer_id: &PeerId) -> bool {
        self.inner
            .lock()
            .await
            .get(peer_id)
            .map(|n| n.state == NeighborState::LinkedUp)
            .unwrap_or(false)
    }

    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }

    /// Snapshot of all known neighbors (for routing, UI, persistence).
    pub async fn neighbors(&self) -> Vec<Neighbor> {
        self.inner.lock().await.values().cloned().collect()
    }

    /// Lightweight snapshot for the routing hot path (ROUT-20): `peer_id` and
    /// `state` only, cloned while holding the lock. `Neighbor` also carries
    /// `peer_bloom` (~175 KB, `#[derive(Clone)]` `BloomFilter`) and
    /// `capabilities`, which flood-recipient selection never reads — cloning
    /// the full struct there allocated and copied megabytes per decision.
    pub async fn neighbor_summaries(&self) -> Vec<NeighborSummary> {
        self.inner
            .lock()
            .await
            .values()
            .map(|n| NeighborSummary {
                peer_id: n.peer_id,
                state: n.state,
                links: n.links.clone(),
            })
            .collect()
    }

    /// Purge neighbors idle for longer than the table TTL. Returns a
    /// `PeerLost` event per eviction (LinkedDown-only now; hard eviction).
    pub async fn sweep(&self) -> Vec<TopologyEvent> {
        let mut map = self.inner.lock().await;
        let now = Instant::now();
        let mut events = Vec::new();
        // Compare elapsed-since-last-seen rather than building a cutoff
        // instant. `Instant::now() - ttl` overflows and PANICS when process
        // uptime is below the TTL — which is the normal case for an app that
        // autostarts at device boot with the 300 s default. The panic happened
        // inside the spawned discovery loop, so the task died silently and
        // neighbour discovery stayed dead for the whole process lifetime.
        let to_remove: Vec<PeerId> = map
            .iter()
            .filter(|(_, n)| now.duration_since(n.last_seen) >= self.ttl)
            .map(|(id, _)| *id)
            .collect();
        for id in to_remove {
            let via = map
                .get(&id)
                .and_then(|n| n.links.first().map(|l| l.transport.clone()));
            if let Some(nb) = map.remove(&id) {
                events.push(TopologyEvent::PeerLost {
                    peer_id: nb.peer_id,
                    via_transport: via.unwrap_or(TransportId::from("ttl")),
                });
            }
        }
        events
    }

    /// All `(transport, quality)` pairs across neighbors, keyed for routing.
    pub async fn link_map(&self) -> BTreeMap<PeerId, Vec<LinkRecord>> {
        let mut out = BTreeMap::new();
        for (id, nb) in self.inner.lock().await.iter() {
            out.insert(*id, nb.links.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PeerInfo;

    fn peer(id: u8) -> PeerInfo {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerInfo {
            peer_id: PeerId::from_bytes(b),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    #[tokio::test]
    async fn first_seen_emits_peer_discovered() {
        let t = NeighborTable::new(Duration::from_secs(60));
        let ev = t
            .upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        assert!(matches!(ev, Some(TopologyEvent::PeerDiscovered { .. })));
        assert_eq!(t.len().await, 1);
        assert!(t.is_up(&peer(1).peer_id).await);
    }

    #[tokio::test]
    async fn rout20_neighbor_summaries_match_full_snapshot() {
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        t.upsert(&peer(2), &TransportId::from("sim-b"), LinkQuality::Good)
            .await;
        let full = t.neighbors().await;
        let summaries = t.neighbor_summaries().await;
        assert_eq!(summaries.len(), full.len());
        for nb in &full {
            let s = summaries
                .iter()
                .find(|s| s.peer_id == nb.peer_id)
                .expect("summary must contain every full-snapshot peer");
            assert_eq!(s.state, nb.state);
        }
    }

    #[tokio::test]
    async fn rout21_best_transport_picks_best_quality_not_first_or_last() {
        // LinkQuality is ordered Excellent < Good < Fair < Poor (lower =
        // better). Insert a middling link first, then a better one and a
        // worse one, so picking "first" or "last" would both give the wrong
        // answer — only picking by actual quality is correct.
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("mid"), LinkQuality::Good)
            .await;
        t.upsert(&peer(1), &TransportId::from("worst"), LinkQuality::Poor)
            .await;
        t.upsert(&peer(1), &TransportId::from("best"), LinkQuality::Excellent)
            .await;
        let links = t.links_to(&peer(1).peer_id).await.expect("peer must exist");
        assert_eq!(links.len(), 3, "all three transports must be recorded");
        assert_eq!(
            best_transport(&links),
            Some(TransportId::from("best")),
            "must select the Excellent-quality link regardless of insertion order"
        );
    }

    #[tokio::test]
    async fn rout21_best_transport_none_when_no_links() {
        assert_eq!(best_transport(&[]), None);
    }

    #[tokio::test]
    async fn second_transport_adds_path_not_peer_lost() {
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let ev = t
            .upsert(
                &peer(1),
                &TransportId::from("sim-b"),
                LinkQuality::Excellent,
            )
            .await;
        assert!(matches!(ev, Some(TopologyEvent::LinkQualityUpdated { .. })));
        assert_eq!(t.len().await, 1);
        let tx = t.transports_to(&peer(1).peer_id).await.unwrap();
        assert_eq!(tx.len(), 2);
    }

    #[tokio::test]
    async fn quality_update_emits_link_quality_updated() {
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let ev = t
            .upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Poor)
            .await;
        assert!(matches!(ev, Some(TopologyEvent::LinkQualityUpdated { .. })));
    }

    #[tokio::test]
    async fn last_link_down_emits_peer_lost_and_linked_down() {
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let ev = t
            .mark_down(&peer(1).peer_id, &TransportId::from("sim-a"))
            .await;
        assert!(matches!(ev, Some(TopologyEvent::PeerLost { .. })));
        assert!(!t.is_up(&peer(1).peer_id).await);
        assert_eq!(t.len().await, 1);
    }

    #[tokio::test]
    async fn ttl_sweep_evicts_idle_neighbors() {
        let t = NeighborTable::new(Duration::from_millis(10));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        let events = t.sweep().await;
        assert_eq!(events.len(), 1);
        assert_eq!(t.len().await, 0);
    }

    // PM-5: the neighbor table must not grow past MAX_NEIGHBORS.
    #[tokio::test]
    async fn neighbor_cap_evicts_lru_on_overflow() {
        let t = NeighborTable::new(Duration::from_secs(600));
        let mk = |n: usize| {
            let mut b = [0u8; 32];
            let bytes = (n as u32).to_le_bytes();
            b[..4].copy_from_slice(&bytes);
            PeerInfo {
                peer_id: PeerId::from_bytes(b),
                addresses: vec![],
                transport_addresses: vec![],
                last_seen: None,
            }
        };
        // Fill to cap.
        for i in 0..MAX_NEIGHBORS {
            t.upsert(&mk(i), &TransportId::from("sim"), LinkQuality::Good)
                .await;
        }
        assert_eq!(t.len().await, MAX_NEIGHBORS);
        // One more peer triggers eviction of the LRU; table stays at cap.
        t.upsert(
            &mk(MAX_NEIGHBORS + 1),
            &TransportId::from("sim"),
            LinkQuality::Good,
        )
        .await;
        assert_eq!(
            t.len().await,
            MAX_NEIGHBORS,
            "table must not exceed MAX_NEIGHBORS after overflow insert"
        );
    }

    #[tokio::test]
    async fn capabilities_recorded_and_retained() {
        let t = NeighborTable::new(Duration::from_secs(60));
        t.upsert(&peer(1), &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let caps = CapabilityBundle {
            node_id: peer(1).peer_id,
            protocol_version: 1,
            transports: vec!["sim-a".to_string()],
            capabilities: vec!["relay".into(), "gateway".into()],
            bloom_m: 64,
            bloom_k: 3,
            timestamp: 0,
            dp_snapshot: Vec::new(),
        };
        t.set_capabilities(&peer(1).peer_id, caps.clone()).await;
        let nb = t.get(&peer(1).peer_id).await.unwrap();
        assert_eq!(
            nb.capabilities.as_ref().unwrap().capabilities,
            caps.capabilities
        );
    }
}
