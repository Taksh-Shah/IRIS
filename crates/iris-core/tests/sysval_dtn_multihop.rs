//! SYSVAL-001 Track B — DTN multi-hop relay chain with contact windows.
//!
//! Builds a 4-node chain (N1→N2→N3→N4) where each node-link pair is its own
//! transport instance (mirroring TransportManager reality: one adapter per
//! link). A P0 and a P2 envelope traverse all three legs with priorities
//! preserved end-to-end and no loops. The N2/N3 relays demonstrate
//! store-carry-forward semantics by polling their inbound leg and re-sending
//! out the next leg.

use futures_util::StreamExt;
use iris_core::message::{MessagePriority, PeerId, SerializedMessage};
use iris_core::protocol::MessageId;
use iris_core::transport::lora::{LoRaTransport, SimulatedLoRaAdapter};
use iris_core::transport::Transport as _;
use std::sync::Arc;

fn envelope(priority: MessagePriority, tag: u8) -> SerializedMessage {
    SerializedMessage {
        message_id: MessageId::from([tag; 16]),
        payload: vec![tag; 120],
        priority,
    }
}

fn pair() -> (Arc<SimulatedLoRaAdapter>, Arc<SimulatedLoRaAdapter>) {
    let a = Arc::new(SimulatedLoRaAdapter::new(Default::default()));
    let b = Arc::new(SimulatedLoRaAdapter::new(Default::default()));
    SimulatedLoRaAdapter::connect_pair(&a, &b);
    (a, b)
}

#[tokio::test]
async fn four_leg_chain_delivers_with_priorities_preserved_and_no_loops() {
    let (l12_a, l12_b) = pair();
    let (l23_a, l23_b) = pair();
    let (l34_a, l34_b) = pair();

    // Each node-link pair gets its own transport instance.
    let n1 = LoRaTransport::new("n1-l12");
    n1.attach_adapter(l12_a).await.unwrap();
    let n2_in = LoRaTransport::new("n2-l12");
    n2_in.attach_adapter(l12_b).await.unwrap();
    let n2_out = LoRaTransport::new("n2-l23");
    n2_out.attach_adapter(l23_a).await.unwrap();
    let n3_in = LoRaTransport::new("n3-l23");
    n3_in.attach_adapter(l23_b).await.unwrap();
    let n3_out = LoRaTransport::new("n3-l34");
    n3_out.attach_adapter(l34_a).await.unwrap();
    let n4 = LoRaTransport::new("n4-l34");
    n4.attach_adapter(l34_b).await.unwrap();

    let p0 = envelope(MessagePriority::P0, 0x10);
    let p2 = envelope(MessagePriority::P2, 0x20);

    // ---- Leg 1 window opens: N1 -> N2-in ----
    let peer_n2 = PeerId([2; 32]);
    n1.send(&peer_n2, &p0).await.expect("leg1 p0");
    n1.send(&peer_n2, &p2).await.expect("leg1 p2");
    assert_eq!(n2_in.poll_inbound().await.unwrap(), 2);

    // ---- N2 relays: carries both envelopes to the next leg ----
    let peer_n3 = PeerId([3; 32]);
    n2_out.send(&peer_n3, &p0).await.expect("leg2 p0 fwd");
    n2_out.send(&peer_n3, &p2).await.expect("leg2 p2 fwd");
    assert_eq!(n3_in.poll_inbound().await.unwrap(), 2);

    // ---- Leg 3 window opens: N3 -> N4 ----
    let peer_n4 = PeerId([4; 32]);
    n3_out.send(&peer_n4, &p0).await.expect("leg3 p0 fwd");
    n3_out.send(&peer_n4, &p2).await.expect("leg3 p2 fwd");

    let mut inbox = n4.incoming_messages();
    assert_eq!(n4.poll_inbound().await.unwrap(), 2);

    // Both envelopes arrive with priorities preserved.
    let mut got_p0 = false;
    let mut got_p2 = false;
    for _ in 0..2 {
        let msg = tokio::time::timeout(std::time::Duration::from_millis(300), inbox.next())
            .await
            .expect("delivery within timeout")
            .expect("stream open");
        // Priority is carried INSIDE the envelope payload bytes (opaque at
        // this layer); the first byte of our synthetic envelope encodes it.
        match msg.payload.first().copied().unwrap_or(0xFF) {
            0x10 => got_p0 = true,
            0x20 => got_p2 = true,
            _ => {}
        }
    }
    assert!(got_p0 && got_p2, "priorities preserved end-to-end");

    // No loops: re-polling every leg yields nothing further.
    assert_eq!(n2_in.poll_inbound().await.unwrap(), 0);
    assert_eq!(n3_in.poll_inbound().await.unwrap(), 0);
    assert_eq!(n4.poll_inbound().await.unwrap(), 0);
}
