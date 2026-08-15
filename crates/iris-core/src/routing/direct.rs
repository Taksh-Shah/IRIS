//! Algorithm 1: Direct Delivery — BASELINE_ROUTING.md §Algorithm 1.

use crate::discovery::neighbor_table::NeighborTable;
use crate::message::PeerId;
use crate::routing::{ForwardingAlgorithm, ForwardingDecision};

/// If the destination is a current neighbor (on any live transport), return a
/// direct forwarding decision. O(1) neighbor-table lookup.
pub async fn try_direct(
    recipient: PeerId,
    neighbor_table: &NeighborTable,
) -> Option<ForwardingDecision> {
    if neighbor_table.is_up(&recipient).await {
        Some(ForwardingDecision::Forward {
            next_hop: recipient,
            algorithm: ForwardingAlgorithm::Direct,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{PeerInfo, LinkQuality};
    use crate::transport::TransportId;
    use std::time::Duration;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    #[tokio::test]
    async fn direct_hit() {
        let table = NeighborTable::new(Duration::from_secs(60));
        let peer = PeerInfo {
            peer_id: pid(5),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        };
        table.upsert(&peer, &TransportId::from("sim"), LinkQuality::Good).await;
        let decision = try_direct(pid(5), &table).await;
        assert!(matches!(
            decision,
            Some(ForwardingDecision::Forward { algorithm: ForwardingAlgorithm::Direct, .. })
        ));
    }

    #[tokio::test]
    async fn direct_miss_for_unknown() {
        let table = NeighborTable::new(Duration::from_secs(60));
        assert!(try_direct(pid(9), &table).await.is_none());
    }
}