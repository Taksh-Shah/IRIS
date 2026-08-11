# IRIS Glossary

**Document ID:** IRIS-GLOSS-001  
**Version:** 1.0

Terms are organized alphabetically within sections. All IRIS-specific definitions are normative.

---

## A

**ACK (Acknowledgement)**  
A control message confirming that a data message was received. IRIS supports two ACK modes: hop-by-hop (custody ACK, from the next relay) and end-to-end (delivery ACK, from the final recipient). See: `ACKNOWLEDGEMENTS.md`.

**Advertising (BLE)**  
The process by which a BLE device broadcasts its presence and capabilities in unconnected state. IRIS uses advertising for peer discovery and emergency SOS beacon. IRIS advertising packets use custom service UUIDs to identify IRIS nodes.

**Anti-entropy**  
A protocol technique for synchronizing state between nodes by comparing summaries (bloom filters, checksums) and exchanging only the differences. IRIS uses anti-entropy during the sync phase of a node contact.

**APDU (Application Protocol Data Unit)**  
The data unit at the application layer of a protocol stack. In IRIS, the application PDU is the fully formed message envelope including headers and encrypted payload.

---

## B

**Backoff**  
A delay strategy that increases wait time between retries to prevent network overload. IRIS uses exponential backoff with jitter for all retransmission logic.

**Battery Policy**  
The set of rules governing how IRIS adapts its behavior based on device battery level. See NODE_MODEL.md. High-priority messages (P0–P2) override battery policy. Low-priority messages (P5–P7) are suppressed when battery < 15%.

**BLE (Bluetooth Low Energy)**  
A short-range wireless radio standard (2.4 GHz ISM band) designed for low-power operation. BLE is IRIS's primary mobile-to-mobile transport. BLE 5.x supports range up to 200m in open air and data rates from 125 kbps (Coded PHY) to 2 Mbps (2M PHY). IRIS uses a mix of advertising (for discovery) and GATT connections (for data transfer).

**Bloom Filter**  
A probabilistic data structure used to test set membership. In IRIS, each node maintains a bloom filter of message IDs it has seen. During node contact, bloom filters are exchanged to determine which messages the other node needs. Bloom filters allow efficient deduplication without sending full message ID lists. False positives (reporting a message as seen when it wasn't) are possible but acceptable — they cause slight under-delivery; false negatives are impossible.

**BPv7 (Bundle Protocol version 7)**  
IETF RFC 9171. A delay-tolerant networking protocol defining the structure of "bundles" (message containers) for DTN communication. IRIS's message format is inspired by BPv7 but not strictly compliant. Key BPv7 concepts used: bundle blocks, custody transfer, administrative records, endpoint identifiers.

**Bundle**  
In DTN terminology, a self-contained unit of data that includes routing headers, security metadata, and payload. A bundle can survive network partitions by being stored and carried until delivery becomes possible. IRIS messages are logically bundles.

**Bundle Protocol**  
See BPv7.

---

## C

**CBOR (Concise Binary Object Representation)**  
IETF RFC 8949. A binary data serialization format designed to be compact and suitable for constrained environments. IRIS uses CBOR for the wire format of all protocol messages. CBOR is self-describing (unlike Protobuf, no schema required to decode), supports all JSON types, and adds binary blobs — critical for cryptographic material.

**Carry-Forward**  
The act of physically transporting stored messages (in device storage) through a network partition. When a node with stored messages moves to a location where those messages can be delivered, it "carries them forward." Synonymous with message mule.

**ChaCha20-Poly1305**  
An authenticated encryption algorithm (AEAD) combining the ChaCha20 stream cipher with the Poly1305 message authentication code. IRIS uses ChaCha20-Poly1305 for all message payload encryption. Properties: fast in software (no AES hardware required), constant-time, no IV reuse vulnerability under normal IRIS usage (unique nonces per message).

**Clock Skew**  
The difference in wall-clock time between two devices. In disconnected scenarios, clocks can drift significantly. IRIS's TTL mechanism accounts for clock skew by using a tolerance window (see TTL.md).

**Coded PHY (BLE)**  
A BLE 5.0 physical layer mode that uses forward error correction to extend range at the cost of data rate. IRIS uses Coded PHY (S=8, 125 kbps) for emergency SOS beacons to maximize range.

**Contact Opportunity**  
A period during which two nodes are within communication range of each other and can exchange messages. Contact opportunities are ephemeral and unpredictable in ad hoc mesh scenarios.

**Convergence Layer**  
In DTN/BPv7 terminology, the layer that adapts a specific transport protocol to the Bundle Protocol. In IRIS, convergence layers exist for BLE, Wi-Fi Direct, LoRa, and Internet. Each convergence layer translates between IRIS messages and the native framing of its transport.

**Cryptographic Identity**  
In IRIS, a node's identity is derived from its Ed25519 public key. The node ID is the first 32 bytes of the SHA-256 hash of the public key. There are no usernames, email addresses, or phone numbers at the protocol level.

**Custody**  
In DTN terminology, accepting custody of a message means taking responsibility for ensuring the message is eventually delivered. A custody relay stores the message persistently and issues a custody ACK to the sender, releasing the sender from responsibility. IRIS implements custody transfer as an optional mode for high-reliability scenarios.

**Custody Transfer**  
The protocol operation by which responsibility for message delivery passes from one node to another. After a custody transfer, the original holder may delete its copy.

---

## D

**Data Mule**  
A mobile node (person, vehicle, animal) that physically carries data through a network partition. The node stores messages in memory and delivers them when it reaches a connected area. Also called a "message mule" or "carry-forward node." See UC-04 (truck driver scenario).

**Deduplication**  
The process of identifying and discarding duplicate copies of the same message. Essential in epidemic routing where messages are forwarded to many nodes. IRIS uses bloom filters + a seen-message LRU cache for deduplication. See DEDUPLICATION.md.

**Delay-Tolerant Networking (DTN)**  
A networking paradigm designed for environments where end-to-end connectivity may not be continuously available. DTN systems store messages and forward them when connectivity becomes available, tolerating delays from seconds to days. IRIS is fundamentally a DTN system.

**Delivery Policy**  
A specification attached to a message indicating how it should be delivered. See DELIVERY_POLICIES.md. Policies include: best-effort, confirmed delivery, priority delivery, emergency broadcast.

**Disruption-Tolerant Networking**  
Synonymous with Delay-Tolerant Networking (DTN) in IRIS context.

---

## E

**ECC (Elliptic Curve Cryptography)**  
A family of public-key cryptography algorithms based on the mathematics of elliptic curves. IRIS uses two ECC algorithms: Ed25519 (digital signatures) and X25519 (key agreement). Both use the Curve25519 curve and provide ~128 bits of security.

**Ed25519**  
An elliptic curve digital signature algorithm using the Edwards-curve Digital Signature Algorithm (EdDSA) on Curve25519. IRIS uses Ed25519 for message signing (authentication, integrity, non-repudiation). Key size: 32 bytes (private), 32 bytes (public). Signature size: 64 bytes.

**Edge Node**  
A dedicated IRIS relay/gateway device deployed at a fixed location. Typically a Raspberry Pi or similar single-board computer. Edge nodes provide persistent storage, long-range radio interfaces (LoRa), and internet gateway capability. See EDGE_ARCHITECTURE.md.

**Endpoint Identifier (EID)**  
In DTN/BPv7, the address of a node or application on the DTN network. In IRIS, EIDs are derived from Ed25519 public key hashes. Format: `iris://[base58-encoded-key-hash]`.

**Epidemic Routing**  
A DTN routing algorithm where a message is forwarded to every node encountered. Named for how an epidemic spreads through a population. Provides high delivery probability but consumes significant bandwidth and storage. IRIS uses epidemic routing for P0 SOS messages (delivery probability overrides efficiency).

---

## F

**Fabric**  
IRIS's metaphor for its communication network. A fabric, unlike a pipe or circuit, has no single path — messages can travel any route through the fabric and the fabric continues to function when threads are cut. The IRIS fabric is the set of all IRIS nodes and their transient connections.

**Forwarding**  
The act of relaying a received message toward its destination. In IRIS, any relay node that has not seen a message will forward it according to the routing algorithm for that message's priority.

**Fragment**  
A subdivision of a message payload created to fit within the MTU of a transport layer. BLE MTU is typically 512 bytes; LoRa packets are 51–255 bytes. Large IRIS messages are fragmented before transmission and reassembled at the destination. See FRAGMENTATION.md.

**Fragmentation**  
The process of splitting a message payload into fragments. Fragmentation occurs at the convergence layer for each transport. Fragments carry: message ID, fragment index, total fragment count, and fragment payload.

---

## G

**Gateway**  
A node that bridges between two different network segments or transport types. IRIS gateways bridge: local BLE/Wi-Fi mesh to internet; local mesh to LoRa; local mesh to satellite. See GATEWAY_ARCHITECTURE.md.

**GATT (Generic Attribute Profile)**  
The BLE protocol for structured data exchange between connected devices. IRIS uses custom GATT services and characteristics for message exchange over BLE connections.

**Group**  
A logical grouping of IRIS nodes identified by a group ID (derived from a shared key or administrative assignment). Group messages are encrypted to all group members. See ADDRESSING.md.

---

## H

**Hop**  
A single relay step: the transmission of a message from one node to an adjacent node. Hop count measures how many relay steps a message has traversed.

**Hop Count**  
The number of relay hops a message has taken since origination. Used in TTL enforcement and routing decisions. Messages exceeding max_hops are dropped.

**Hop-by-Hop ACK**  
An acknowledgement sent from each relay node to the previous node, confirming receipt. Contrasted with end-to-end ACK which only the final recipient sends. See ACKNOWLEDGEMENTS.md.

---

## I

**IRIS (Intelligent Resilient Infrastructure for Survival)**  
This project. A multi-transport, heterogeneous, disruption-tolerant communication fabric for emergency and disaster communication.

---

## L

**Latency**  
The time from message origination to message receipt. In IRIS, latency is highly variable: milliseconds (direct BLE connection) to hours or days (store-carry-forward through partitioned network). Design targets are per-priority: P0 ≤ 90 seconds in typical disaster mesh; P4 may be hours.

**LoRa (Long Range)**  
A physical layer radio technology using chirp spread spectrum modulation. Proprietary to Semtech. LoRa operates in unlicensed sub-GHz bands (865–867 MHz in India). Key properties: range 2–30 km, data rate 250 bps–5.5 kbps, excellent link budget (+10 dB over FSK). IRIS uses LoRa for long-range, low-throughput transport.

**LoRaWAN**  
A MAC layer protocol and network architecture built on LoRa physical layer. LoRaWAN uses a star topology with centralized network servers. IRIS does NOT use LoRaWAN architecture — IRIS uses LoRa (physical layer) in a peer-to-peer/mesh architecture without centralized network servers.

**LRU Cache (Least Recently Used)**  
A cache eviction strategy that removes the least recently accessed items first. IRIS uses LRU caches for the seen-message deduplication cache.

---

## M

**MANET (Mobile Ad hoc Network)**  
A network of mobile nodes that communicate directly with each other without fixed infrastructure. All IRIS mobile nodes form a MANET when infrastructure is unavailable.

**Max Hops**  
The maximum number of relay hops a message is allowed to take. Prevents infinite circulation. Default values: P0 SOS: 20 hops, P4 normal: 10 hops, P7 bulk: 5 hops.

**Mesh**  
A network topology where nodes are interconnected and messages can travel multiple paths. Contrasted with star (hub-and-spoke) topology. IRIS forms a mesh among nearby nodes.

**Message Engine**  
The IRIS subsystem responsible for message storage, deduplication, TTL management, priority enforcement, and forwarding decisions. See LAYER_MODEL.md Layer 4.

**Message ID**  
A universally unique identifier for a message. In IRIS, the message ID is a 128-bit UUID v4 (random). Message IDs are used for deduplication and ACK correlation.

**Message Mule**  
See Data Mule.

**MTU (Maximum Transmission Unit)**  
The maximum size of a data packet at a given network layer. BLE effective MTU: ~512 bytes. LoRa: 51–255 bytes (spreading factor dependent). Wi-Fi: 1500 bytes (Ethernet standard). IRIS message fragmentation is driven by transport MTU limits.

**Multicast**  
Sending a message to a subset of nodes (a group) rather than all nodes (broadcast) or one node (unicast). IRIS group messages are encrypted multicast.

**Multipath**  
The property of routing messages across multiple simultaneous paths to increase delivery probability. IRIS supports multipath for high-priority messages when multiple transports are available.

---

## N

**NAN (Neighbor Awareness Networking)**  
See Wi-Fi Aware.

**Network Partition**  
A network state where the graph is split into disconnected subgraphs — nodes in different partitions cannot communicate. DTN is designed to operate through partitions by storing messages until paths reconnect.

**Node**  
Any IRIS-capable device participating in the mesh. Nodes can be: originators (message senders), relays (message forwarders), gateways (transport bridges), or edge nodes (fixed infrastructure). See NODE_MODEL.md.

**Node ID**  
The unique identifier of a node. In IRIS: first 32 bytes of SHA-256(Ed25519_public_key). Stable across sessions, derived from cryptographic identity.

**Noise Protocol Framework**  
A framework for building secure channel protocols using Diffie-Hellman key exchange. IRIS uses the Noise_XX pattern for mutual authentication and key establishment between nodes. Noise provides: forward secrecy, mutual authentication, encrypted channels.

---

## O

**Opportunistic Routing**  
Routing that makes forwarding decisions based on currently available contacts rather than pre-computed paths. All IRIS routing is opportunistic — the network topology is too dynamic and disconnected for traditional routing tables.

**Originator**  
The node that created a message. The originator signs the message with its Ed25519 private key. Relay nodes forward without modifying the signature.

---

## P

**P0–P7 (Priority Levels)**  
IRIS message priority levels. P0 is highest priority (SOS), P7 is lowest (bulk data). See PRIORITY_MODEL.md.

| Level | Name | Examples |
|-------|------|---------|
| P0 | SOS Emergency | SOS beacon, building collapse alert |
| P1 | Medical Emergency | Injury report, medical coordination |
| P2 | Location/Safety | GPS sharing, safety check-in |
| P3 | Emergency Text | Urgent coordination, evacuation orders |
| P4 | Normal Text | Regular messages |
| P5 | Image | Photos |
| P6 | Audio | Voice messages |
| P7 | Video/Bulk | Large files |

**Partition**  
See Network Partition.

**Payload**  
The data content of a message, excluding protocol headers. In IRIS, the payload is always encrypted. The payload type (text, GPS, image, etc.) is indicated in the message header.

**PRoPHET (Probabilistic Routing Protocol using History of Encounters and Transitivity)**  
A DTN routing algorithm that uses historical encounter statistics to predict future contact opportunities and makes forwarding decisions based on delivery probability estimates. IRIS uses PRoPHET-inspired metrics for P3–P4 message routing when full epidemic routing is too expensive.

**Protocol Version**  
A numeric identifier for the version of the IRIS protocol. Version is carried in every message envelope for negotiation and compatibility. Current version: 1.

---

## R

**Reassembly**  
The process of reconstructing a complete message from received fragments. See REASSEMBLY.md.

**Relay**  
A node that forwards messages it has received. Any IRIS node that is not the final recipient acts as a relay for messages it can forward. Relaying is automatic and transparent.

**Routing**  
The process of deciding which nodes to forward a message to. IRIS routing is priority-dependent: P0 = epidemic (forward to all), P1-P2 = spray-and-wait, P3-P4 = PRoPHET, P5-P7 = opportunistic with limits.

---

## S

**SAR (Store-And-Relay)**  
See Store-Carry-Forward.

**Satellite Gateway**  
An IRIS gateway that bridges the local mesh to a satellite communication link. Used for transcontinental or remote area communication when LoRa range is insufficient.

**SCF (Store-Carry-Forward)**  
See Store-Carry-Forward.

**Seen-Message Cache**  
A data structure (LRU cache backed by a bloom filter) that tracks which message IDs a node has already processed. Used to prevent duplicate forwarding.

**Signature**  
A 64-byte Ed25519 digital signature over the message header + payload hash. Computed by the originator. Verified by every recipient and relay node (for integrity; relays cannot verify authenticity without the sender's public key, which may need to be fetched).

**SOS**  
"Save Our Souls" — the international distress signal. In IRIS, SOS is the P0 priority level. SOS messages have: mandatory relay (all nodes must forward), no-drop policy (cannot be removed from storage while TTL is valid), epidemic routing.

**Spray-and-Wait**  
A DTN routing algorithm that forwards a limited number of message copies (the "spray" phase) and then waits for direct delivery to the destination (the "wait" phase). More efficient than full epidemic routing; used for P1-P2 in IRIS.

**Store-Carry-Forward (DTN)**  
The fundamental DTN operational mode: nodes store messages in persistent storage, carry them as they move, and forward them when they encounter nodes closer to the destination or the destination itself.

**Store-and-Forward**  
A network communication model where intermediate nodes store complete messages before forwarding them. All IRIS relay nodes are store-and-forward nodes.

**Sync Protocol**  
The IRIS protocol for synchronizing message stores between two nodes that have come into contact. Uses bloom filter exchange to identify missing messages efficiently. See SYNCHRONIZATION.md.

---

## T

**Temporal Graph**  
A graph representation of the network that includes time as a dimension — edges exist only during specific time intervals (contact opportunities). IRIS routing decisions are informed by temporal graph analysis. See TEMPORAL_GRAPH.md.

**Transport**  
A specific communication technology used to carry IRIS messages between adjacent nodes. IRIS supports multiple transports: BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite, Internet. The TransportManager abstraction allows IRIS to use any available transport.

**TransportManager**  
The IRIS subsystem that manages all transport implementations, selects the optimal transport for each message, and handles transport failures transparently.

**TTL (Time-To-Live)**  
A value embedded in a message specifying when the message expires. IRIS supports two TTL types: hop-count TTL (max relay hops) and time-based TTL (absolute expiration timestamp). When TTL expires, the message is dropped and not forwarded. See TTL.md.

---

## V

**Vector Clock**  
A data structure for tracking causal ordering of events in a distributed system. Each node maintains a counter for every known node; messages carry the sender's vector clock value for causality tracking. Used in IRIS synchronization to resolve state ordering.

---

## W

**Wi-Fi Aware (NAN — Neighbor Awareness Networking)**  
An IEEE 802.11 standard (Android 8+) enabling devices to discover nearby devices and communicate without an access point. Wi-Fi Aware supports cluster formation, service discovery, and data transfer with lower latency than Wi-Fi Direct setup. IRIS uses Wi-Fi Aware for Android-to-Android high-throughput local communication.

**Wi-Fi Direct (P2P)**  
A Wi-Fi standard enabling device-to-device communication without a traditional access point. One device acts as a "Group Owner" (soft AP); others connect to it. IRIS uses Wi-Fi Direct for high-throughput local mesh (up to 250 Mbps). Higher setup latency than BLE but much higher throughput — used for bulk data (images, voice messages).

---

## X

**X25519**  
An elliptic curve Diffie-Hellman (ECDH) key agreement function on Curve25519. Used in IRIS's Noise Protocol handshake to establish shared secrets for session encryption. X25519 provides forward secrecy: session keys are ephemeral and not derived from long-term identity keys.

---

## Z

**Zombie Message**  
A message whose TTL has expired but which has not yet been removed from all nodes' storage. IRIS implements TTL expiration checks on storage access and periodic garbage collection. Zombie messages are never forwarded even if encountered.
