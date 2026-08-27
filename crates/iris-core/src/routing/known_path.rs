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

/// Maximum number of destinations tracked simultaneously (ROUT-13 capacity bound).
const MAX_ENTRIES: usize = 1_000;

/// Destination → next-hop routing table with 10-minute entry expiry.
#[derive(Debug)]
pub struct RoutingTable {
    entries: HashMap<PeerId, RouteEntry>,
    expiry: Duration,
}

impl Default for RoutingTable {
    fn default() -> Self {
        RoutingTable {
            entries: HashMap::new(),
            expiry: Duration::from_secs(600),
        }
    }
}

impl RoutingTable {
    pub fn new(expiry: Duration) -> Self {
        RoutingTable {
            entries: HashMap::new(),
            expiry,
        }
    }

    /// Best (sole) route for a destination. None if absent.
    pub fn best_route(&self, destination: &PeerId) -> Option<RouteEntry> {
        self.entries.get(destination).cloned()
    }

    /// Insert or refresh a route. `hop_count` is hops from this node.
    ///
    /// ROUT-12: only overwrites an existing entry when the new route is strictly
    /// better (shorter hop count, or equal hops with better quality) or when the
    /// entry is refreshed via the same next hop or has expired.
    /// ROUT-13: evicts the oldest entry when at capacity to bound RAM.
    pub fn upsert(
        &mut self,
        destination: PeerId,
        next_hop: PeerId,
        transport: String,
        hop_count: u8,
        quality: LinkQuality,
    ) {
        let now = Instant::now();
        if let Some(existing) = self.entries.get(&destination) {
            let expired = now.duration_since(existing.last_confirmed) >= self.expiry;
            let refresh = existing.next_hop == next_hop;
            // LinkQuality is ordered Excellent < Good < Fair < Poor (lower = better).
            let better = hop_count < existing.hop_count
                || (hop_count == existing.hop_count && quality < existing.quality);
            if !expired && !refresh && !better {
                return;
            }
        } else if self.entries.len() >= MAX_ENTRIES {
            // At capacity: evict the entry with the oldest last_confirmed.
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_confirmed)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&oldest_key);
            }
        }
        self.entries.insert(
            destination,
            RouteEntry {
                destination,
                next_hop,
                transport,
                hop_count,
                quality,
                last_confirmed: now,
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

    /// Drop entries not confirmed within the table's expiry.
    pub fn prune_expired(&mut self, expiry: Duration) {
        // Use elapsed comparison, not a cutoff instant — see SYS-1 /
        // neighbor_table.rs:236 for why Instant::now() - duration panics on boot.
        let now = Instant::now();
        self.entries.retain(|_, e| now.duration_since(e.last_confirmed) < expiry);
    }

    /// Drop entries not confirmed within the configured expiry.
    pub fn prune(&mut self) {
        self.prune_expired(self.expiry);
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
    // ROUT-21: select a live transport to the next hop. A `LinkedUp`
    // neighbor with (for whatever reason) no usable links is treated the
    // same as "not up" — invalidate and miss, rather than returning a
    // decision with nothing to send over.
    let links = neighbor_table.links_to(&entry.next_hop).await?;
    let transport = match crate::discovery::neighbor_table::best_transport(&links) {
        Some(t) => t,
        None => {
            routing_table.invalidate(&recipient, &entry.next_hop);
            return None;
        }
    };
    Some(ForwardingDecision::Forward {
        next_hop: entry.next_hop,
        transport,
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

    /// Like `pid` but distinct for every value in `0..MAX_ENTRIES` (`pid`'s
    /// single byte wraps every 256 and collides for capacity-bound tests).
    fn pid_wide(id: usize) -> PeerId {
        let mut b = [0u8; 32];
        b[..8].copy_from_slice(&(id as u64).to_le_bytes());
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

    #[test]
    fn rout12_better_route_replaces_worse() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        // Install a 5-hop route.
        table.upsert(pid(9), pid(4), "sim".into(), 5, LinkQuality::Poor);
        // 3-hop route is strictly better — must win.
        table.upsert(pid(9), pid(5), "sim".into(), 3, LinkQuality::Good);
        let e = table.best_route(&pid(9)).unwrap();
        assert_eq!(e.hop_count, 3, "shorter hop route must replace longer");
        assert_eq!(e.next_hop, pid(5));
    }

    #[test]
    fn rout12_worse_route_does_not_replace_better() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        // 9-hop Poor route must not overwrite the 2-hop Good one.
        table.upsert(pid(9), pid(99), "lora".into(), 9, LinkQuality::Poor);
        let e = table.best_route(&pid(9)).unwrap();
        assert_eq!(e.hop_count, 2, "better existing route must survive");
        assert_eq!(e.next_hop, pid(4));
    }

    #[test]
    fn rout12_same_nexthop_is_a_refresh() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        // Same next_hop — refresh is always accepted.
        table.upsert(pid(9), pid(4), "ble".into(), 5, LinkQuality::Poor);
        let e = table.best_route(&pid(9)).unwrap();
        assert_eq!(e.next_hop, pid(4), "same next_hop refresh must be accepted");
    }

    #[test]
    fn rout13_capacity_bound_evicts_oldest() {
        let mut table = RoutingTable::new(Duration::from_secs(600));
        for i in 0..MAX_ENTRIES {
            table.upsert(pid_wide(i), pid(1), "sim".into(), 1, LinkQuality::Good);
        }
        assert_eq!(table.len(), MAX_ENTRIES);
        // One more insertion must evict an old entry, not grow past MAX.
        table.upsert(pid_wide(MAX_ENTRIES + 1), pid(2), "sim".into(), 1, LinkQuality::Good);
        assert!(
            table.len() <= MAX_ENTRIES,
            "table must not exceed MAX_ENTRIES"
        );
    }
}
