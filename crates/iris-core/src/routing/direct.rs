//! Algorithm 1: Direct Delivery — BASELINE_ROUTING.md §Algorithm 1.

use crate::discovery::neighbor_table::{best_transport, NeighborTable};
use crate::message::PeerId;
use crate::routing::{ForwardingAlgorithm, ForwardingDecision};

/// If the destination is a current neighbor with a live, selectable
/// transport, return a direct forwarding decision. O(1) neighbor-table
/// lookup.
///
/// ROUT-21: `is_up` alone (state == `LinkedUp`) does not mean a specific
/// transport is currently usable — this also selects *which* transport, and
/// falls through to `None` (letting KnownPath/Flood/Store take over) rather
/// than returning a decision with no transport to act on if, for some
/// reason, a `LinkedUp` neighbor's `links` is empty.
pub async fn try_direct(
    recipient: PeerId,
    neighbor_table: &NeighborTable,
) -> Option<ForwardingDecision> {
    if !neighbor_table.is_up(&recipient).await {
        return None;
    }
    let links = neighbor_table.links_to(&recipient).await?;
    let transport = best_transport(&links)?;
    Some(ForwardingDecision::Forward {
        next_hop: recipient,
        transport,
        algorithm: ForwardingAlgorithm::Direct,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{LinkQuality, PeerInfo};
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
        table
            .upsert(&peer, &TransportId::from("sim"), LinkQuality::Good)
            .await;
        let decision = try_direct(pid(5), &table).await;
        assert!(matches!(
            decision,
            Some(ForwardingDecision::Forward {
                algorithm: ForwardingAlgorithm::Direct,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn direct_miss_for_unknown() {
        let table = NeighborTable::new(Duration::from_secs(60));
        assert!(try_direct(pid(9), &table).await.is_none());
    }
}
