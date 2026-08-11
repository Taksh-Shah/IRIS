# Graph Intelligence — Network Topology Analytics

**Component:** `iris-core/graph` (Rust, petgraph), `iris-sim/graph` (Python, NetworkX)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

IRIS maintains a dynamic network graph representing the known topology of the mesh. This graph drives:

- Identification of critical relay nodes (bridge detection)
- Community detection for geographically clustered deployments
- Partition prediction — detecting when a network segment is about to become isolated
- Routing hints for direct-delivery optimization

The graph is built from contact history, capability beacons, and routing table state. It is updated incrementally as new contact events arrive; full recomputation is avoided except at startup.

---

## 2. Graph Data Model

### 2.1 Nodes

Each node in the network graph represents an IRIS device:

```rust
pub struct GraphNode {
    pub node_id: NodeId,          // pseudonymized for privacy
    pub node_type: NodeType,      // Mobile, EdgeServer, FixedRelay, Vehicle
    pub last_seen: Timestamp,
    pub transports: TransportSet, // BLE, WiFiDirect, LoRa, Satellite
    pub battery_level: Option<f32>,
    pub community_id: Option<u32>, // assigned by Louvain
}
```

### 2.2 Edges

Edges represent observed contacts:

```rust
pub struct GraphEdge {
    pub contact_count: u32,       // number of contacts observed
    pub last_contact: Timestamp,
    pub avg_duration_s: f32,
    pub prophet_dp: f32,          // PRoPHET delivery probability
    pub weight: f32,              // derived: prophet_dp × recency_factor
}
```

Edge weight decays with time using the same aging factor as PRoPHET:

```
weight(t) = prophet_dp × γ^(t - t_last_contact)
```

where `γ = 0.98` (configurable).

### 2.3 Graph Representation (Rust petgraph)

```rust
use petgraph::graph::{Graph, NodeIndex, EdgeIndex};
use petgraph::Undirected;

pub type MeshGraph = Graph<GraphNode, GraphEdge, Undirected>;
```

Maximum graph size: **10,000 nodes, 100,000 edges**. At this scale, petgraph uses approximately 20 MB on Raspberry Pi Zero 2W.

---

## 3. Memory Budget

| Scale | Nodes | Edges | petgraph RAM | NetworkX RAM |
|---|---|---|---|---|
| Small | 10 | 45 | ~0.2 MB | ~5 MB |
| Medium | 100 | 2,000 | ~2 MB | ~15 MB |
| Large | 1,000 | 50,000 | ~20 MB | ~150 MB |
| Disaster | 10,000 | 500,000 | ~200 MB | not recommended |

For Pi Zero 2W (512 MB RAM, 50 MB budget for graph), the maximum supported graph is **1,000 nodes**. Above this threshold, distant nodes are pruned by last-seen age (oldest first) to stay within the memory envelope.

---

## 4. Graph Algorithms

### 4.1 Betweenness Centrality

Betweenness centrality identifies nodes that lie on the most shortest paths — these are critical relay candidates. A node with high betweenness that goes offline is likely to cause routing degradation.

**Definition:**

```
BC(v) = Σ_{s≠v≠t} σ(s,t|v) / σ(s,t)
```

where `σ(s,t)` is the number of shortest paths from `s` to `t` and `σ(s,t|v)` is the number passing through `v`.

**Complexity:** O(VE) for unweighted graphs (Brandes algorithm). For a 1,000-node graph this is ~10M operations, taking ~500 ms on Pi Zero.

**Scheduling:** Betweenness centrality is recomputed every 5 minutes, not on every contact event. Intermediate contact events update edge weights only.

```rust
use petgraph::algo::betweenness_centrality;

pub fn identify_critical_relays(
    graph: &MeshGraph,
    threshold: f32,
) -> Vec<NodeIndex> {
    let bc = betweenness_centrality(graph);
    bc.iter()
        .enumerate()
        .filter(|(_, &score)| score > threshold)
        .map(|(idx, _)| NodeIndex::new(idx))
        .collect()
}
```

**Threshold:** nodes with BC > 0.1 (normalized) are flagged as critical. The coordinator UI highlights these nodes.

### 4.2 Bridge Detection

A bridge is an edge whose removal disconnects the graph. Bridges represent single points of failure in the mesh.

**Algorithm:** Tarjan's bridge-finding algorithm, O(V + E).

```rust
use petgraph::algo::find_bridges;

pub fn detect_network_bridges(graph: &MeshGraph) -> Vec<EdgeIndex> {
    find_bridges(graph)
}
```

**Use cases:**
- Alert coordinator when a critical bridge node's battery is low
- PRoPHET routing avoids routing through bridge nodes for non-urgent traffic (preserves capacity for P0)
- Network partition prediction: if a bridge node hasn't been seen for >5 minutes, a partition is likely

### 4.3 Louvain Community Detection

Community detection groups nodes that contact each other frequently — typically corresponding to geographic clusters (building floors, rescue teams, villages).

**Algorithm:** Louvain method (modularity optimization), typical complexity O(n log n).

```python
# Python simulation / edge server only (not on mobile)
import networkx as nx
from community import best_partition  # python-louvain

def detect_communities(G: nx.Graph) -> dict[int, int]:
    """Returns {node_id: community_id}"""
    partition = best_partition(G, weight='weight', resolution=1.0)
    return partition
```

**Output:** Each node is assigned a community ID. Community IDs are used by Spray-and-Wait to allocate copies — more copies go to nodes in different communities to maximize geographic spread.

**Update frequency:** Recomputed every 10 minutes on edge servers, every 30 minutes on mobile devices.

### 4.4 Connected Components

Quickly determines whether the network is partitioned:

```rust
use petgraph::algo::connected_components;

pub fn is_partitioned(graph: &MeshGraph) -> bool {
    connected_components(graph) > 1
}

pub fn partition_count(graph: &MeshGraph) -> usize {
    connected_components(graph)
}
```

---

## 5. Incremental Graph Updates

Full graph recomputation is expensive. IRIS applies incremental updates for common events:

| Event | Graph operation | Cost |
|---|---|---|
| New contact observed | Insert/update edge | O(1) |
| Node capability beacon received | Update node attributes | O(1) |
| Node not seen for TTL period | Mark node stale (soft delete) | O(1) |
| Contact history pruned | Remove edge | O(degree) |
| Topology sync from peer | Merge subgraph | O(delta edges) |

**Stale node policy:** A node is marked stale if not seen for `2 × expected_inter_contact_time`. It is removed from the graph (hard delete) after 24 hours of no contact.

### 5.1 Graph Merge

When two network partitions reconnect, each node has a partial view of the graph. Graph merge is additive: edges from both views are union-merged, with the higher-confidence edge winning on conflicts.

```rust
pub fn merge_graphs(local: &mut MeshGraph, remote: &MeshGraph) {
    for edge in remote.edge_references() {
        let (a, b) = (edge.source(), edge.target());
        if let Some(local_edge) = local.find_edge(a, b) {
            // Keep higher confidence (more contact events)
            if remote[edge.id()].contact_count > local[local_edge].contact_count {
                local[local_edge] = remote[edge.id()].clone();
            }
        } else {
            local.add_edge(a, b, remote[edge.id()].clone());
        }
    }
}
```

---

## 6. Graph Export and Visualization

### 6.1 Export Format

The graph is exported as GraphML for coordinator visualization tools:

```xml
<graphml>
  <graph id="iris-mesh" edgedefault="undirected">
    <node id="n1">
      <data key="node_type">EdgeServer</data>
      <data key="community_id">0</data>
      <data key="betweenness">0.42</data>
    </node>
    <edge source="n1" target="n2">
      <data key="prophet_dp">0.73</data>
      <data key="last_contact_age_s">142</data>
    </edge>
  </graph>
</graphml>
```

### 6.2 REST API (Edge Server)

```
GET /api/v1/graph/topology        → full graph export (GraphML)
GET /api/v1/graph/critical-nodes  → nodes with BC > threshold
GET /api/v1/graph/bridges         → bridge edges
GET /api/v1/graph/communities     → community assignments
GET /api/v1/graph/partitions      → connected component count
```

---

## 7. Partition Prediction

IRIS tracks the derivative of bridge node contact frequency. If a bridge node's contact rate drops below 20% of its 7-day average, a partition warning is raised:

```
WARN  component=graph event=partition_risk node_id=<pseudonym>
      reason="bridge_node_contact_rate_low"
      current_rate=0.12 baseline_rate=0.87 threshold=0.20
```

The coordinator receives a push notification. The network responds by increasing Spray-and-Wait copy count for cross-partition edges.

---

## 8. Performance Benchmarks

| Algorithm | 100 nodes | 1,000 nodes | Target (Pi Zero) |
|---|---|---|---|
| Betweenness centrality | 8 ms | 480 ms | < 500 ms |
| Bridge detection | 1 ms | 12 ms | < 50 ms |
| Louvain community | 5 ms | 180 ms | < 500 ms |
| Connected components | < 1 ms | 3 ms | < 10 ms |
| Graph merge (100 edges) | < 1 ms | < 1 ms | < 5 ms |

---

## 9. References

- petgraph crate: https://docs.rs/petgraph/
- Brandes betweenness centrality: Brandes, U. (2001). "A faster algorithm for betweenness centrality"
- Louvain algorithm: Blondel et al. (2008). "Fast unfolding of communities in large networks"
- Tarjan bridges: Tarjan, R.E. (1974). "A note on finding the bridges of a graph"
- NetworkX: https://networkx.org/
