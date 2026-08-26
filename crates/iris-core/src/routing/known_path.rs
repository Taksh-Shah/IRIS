//! Algorithm 2: Known-Path Routing — BASELINE_ROUTING.md §Algorithm 2.
//!
//! Routing table: `destination → next_hop` with hop count and quality; entries
//! expire after 10 minutes of no confirmation. Next-hop reachability is
//! verified against the neighbor table; unreachable next-hops invalidate the
//! entry and trigger re-discovery.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::discovery::neighbor_table::NeighborTable;
use crate::message::{LinkQuality, PeerId};
use crate::routing::{ForwardingAlgorithm, ForwardingDecision};

/// A single routing-table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    pub destination: PeerId,
    pub next_hop: PeerId,
    pub transport: String,
    pub hop_count: u8,
    pub quality: LinkQuality,
    pub last_confirmed: Instant,
}

/// Destination → next-hop routing table with 10-minute entry expiry.
#[derive(Debug, Default)]
pub struct RoutingTable {
    entries: HashMap<PeerId, RouteEntry>,
}

impl RoutingTable {
    pub fn new(_expiry: Duration) -> Self {
        RoutingTable::default()
    }

    /// Best (sole) route for a destination. None if absent.
    pub fn best_route(&self, destination: &PeerId) -> Option<RouteEntry> {
        self.entries.get(destination).cloned()
    }

    /// Insert or refresh a route. `hop_count` is hops from this node.
    pub fn upsert(
        &mut self,
        destination: PeerId,
        next_hop: PeerId,
        transport: String,
        hop_count: u8,
        quality: LinkQuality,
    ) {
        self.entries.insert(
            destination,
            RouteEntry {
                destination,
                next_hop,
                transport,
                hop_count,
                quality,
                last_confirmed: Instant::now(),
            },
        );
    }

    /// Remove a route (e.g. next hop became unreachable).
    pub fn invalidate(&mut self, destination: &PeerId, next_hop: &PeerId) {
        if self
            .entries
            .get(destination)
            .map(|e| &e.next_hop == next_hop)
            .unwrap_or(false)
        {
            self.entries.remove(destination);
        }
    }

    /// Drop entries not confirmed within `expiry`.
    pub fn prune_expired(&mut self, expiry: Duration) {
        // Use elapsed comparison, not a cutoff instant — see SYS-1 /
        // neighbor_table.rs:236 for why Instant::now() - duration panics on boot.
        let now = Instant::now();
        self.entries.retain(|_, e| now.duration_since(e.last_confirmed) < expiry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Look up a known path and validate the next hop is still reachable.
pub async fn try_known_path(
    recipient: PeerId,
    routing_table: &mut RoutingTable,
    neighbor_table: &NeighborTable,
) -> Option<ForwardingDecision> {
    let entry = routing_table.best_route(&recipient)?;
    // Next hop must still be a live neighbor, else invalidate + miss.
    if !neighbor_table.is_up(&entry.next_hop).await {
        routing_table.invalidate(&recipient, &entry.next_hop);
        return None;
    }
    Some(ForwardingDecision::Forward {
        next_hop: entry.next_hop,
        algorithm: ForwardingAlgorithm::KnownPath,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PeerInfo;
    use crate::transport::TransportId;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn peer_info(id: u8) -> PeerInfo {
        PeerInfo {
            peer_id: pid(id),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    #[tokio::test]
    async fn known_path_hit_with_reachable_hop() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        let neighbors = NeighborTable::new(Duration::from_secs(60));
        neighbors
            .upsert(&peer_info(4), &TransportId::from("sim"), LinkQuality::Good)
            .await;
        let decision = try_known_path(pid(9), &mut table, &neighbors).await;
        assert!(matches!(
            decision,
            Some(ForwardingDecision::Forward {
                algorithm: ForwardingAlgorithm::KnownPath,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn unreachable_next_hop_invalidates() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        let neighbors = NeighborTable::new(Duration::from_secs(60));
        // pid(4) not present in the neighbor table.
        let decision = try_known_path(pid(9), &mut table, &neighbors).await;
        assert!(decision.is_none());
        assert!(
            table.best_route(&pid(9)).is_none(),
            "entry must be invalidated"
        );
    }

    #[tokio::test]
    async fn unknown_destination_misses() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        let neighbors = NeighborTable::new(Duration::from_secs(60));
        assert!(try_known_path(pid(2), &mut table, &neighbors)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn prune_expired_entries() {
        let mut table = RoutingTable::new(Duration::from_millis(10));
        table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        std::thread::sleep(Duration::from_millis(30));
        table.prune_expired(Duration::from_millis(10));
        assert!(table.is_empty());
    }
}
