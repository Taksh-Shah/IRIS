# IRIS Vision — The Communication Fabric

**Document ID:** IRIS-VISION-001  
**Version:** 1.0  
**Status:** Active

---

## 1. The Core Vision

Imagine a thread woven through every device in a disaster zone — every phone, every laptop, every dedicated relay node — forming a fabric that carries messages even when every cable is cut, every tower is down, every satellite link is saturated. This is not a network in the traditional sense. There is no center. There is no operator. There is no single point that, when destroyed, silences everyone.

This is what IRIS is: a **communication fabric**.

A fabric is not a pipe. A pipe carries data from A to B through a fixed channel. A fabric adapts — messages flow around obstructions, accumulate at waypoints, and resume their journey when a path becomes available. A fabric degrades gracefully: fewer threads means slower propagation, not silence.

IRIS is the fabric. Applications — messaging, SOS, location sharing, coordination tools — are woven into it. The fabric exists whether or not any particular application is running.

---

## 2. The Ahmedabad → Kutch Scenario

### 2.1 Context

It is 3:47 AM on a Tuesday in January. A magnitude 7.6 earthquake strikes the Rann of Kutch — the same fault line that killed 20,000 people in the 2001 Bhuj earthquake. The epicenter is 40 km north of Bhuj.

Within 90 seconds:
- Every cell tower within 80 km has lost power or collapsed
- Fiber trunk lines running through the affected corridor are severed
- BSNL's regional switching infrastructure in Bhuj is offline
- Satellite uplinks are overwhelmed with automated distress signals
- Roads are impassable in 60% of the affected area

**The communication problem:** 400,000 people need to communicate. Emergency responders in Ahmedabad (300 km away) need to coordinate. Families separated by rubble need to find each other. Medical teams need to reach the injured. Search and rescue needs GPS coordinates of trapped survivors.

Every conventional channel has failed.

### 2.2 What IRIS Does

#### Hour 0: The Quake Hits

Every IRIS-enabled device in the affected area automatically detects the loss of cellular connectivity. Within 3 seconds, each device:
1. Activates BLE advertising and scanning
2. Scans for Wi-Fi Direct peers
3. Begins broadcasting its presence on the IRIS emergency channel

Devices that were sleeping wake the IRIS background service (via Android/iOS emergency broadcast hooks).

**The mesh forms.** 50,000 devices in the affected area start finding each other via BLE. Groups of people cluster: 6 people in a collapsed building connect via BLE. 20 survivors in an open field form a Wi-Fi Direct mesh. 3 people near a standing vehicle with a vehicle-mounted IRIS node get higher bandwidth.

#### Hour 0 + 4 minutes: The First SOS

A woman trapped under rubble in Bhuj taps the SOS button on her IRIS app. Her phone, now with 40% battery, transmits a P0 SOS message:

```
Priority:    P0 (SOS — mandatory relay, no drop)
Message ID:  a7f3b2c1-9d4e-4f6a-8b3d-1e2f4a5c6d7e
Sender:      [Ed25519 public key hash of her device identity]
Recipient:   EMERGENCY_BROADCAST
Content:     "TRAPPED. Bhuj old city, near Swaminarayan temple. 3 people. Building collapsed."
GPS:         23.2419°N, 69.6669°E
Timestamp:   2026-01-14T03:51:22Z
TTL:         72 hours
Signature:   [Ed25519 signature]
```

This message is 287 bytes. It fits in a single BLE packet.

#### The Message Paths

**Path A: BLE Hop Chain (first 2 km)**

Her phone → [BLE] → phone of her neighbor's son, also trapped but with signal → [BLE] → phone of a man standing 30m outside the rubble → [BLE] → phone of someone who has a clearer view. Each device relays immediately. Each device marks the message as seen (bloom filter update) to prevent re-relay loops.

In 8 seconds, the message has traversed 4 hops and reached the edge of the immediate collapse zone.

**Path B: Wi-Fi Direct Bridge (2–8 km)**

At the edge zone, a group of survivors has formed a Wi-Fi Direct mesh — higher throughput, longer range than BLE. The message bridges from BLE to Wi-Fi Direct and propagates through the survivor cluster. Wi-Fi Direct allows simultaneous relay to 8+ peers. In 22 seconds, the message reaches a IRIS edge node — a Raspberry Pi with a long-range Wi-Fi antenna, deployed 6 months earlier by the Bhuj municipal government at the civil hospital.

**Path C: LoRa Long Haul (8–80 km)**

The edge node at Bhuj Civil Hospital has a LoRa module (915 MHz, 20 dBm). It reformats the P0 SOS for LoRa transmission: fragmented into 2 × 127-byte LoRa packets, sent on the emergency frequency. A LoRa repeater on top of a hill 35 km away — solar-powered, battery-backed, placed by IRIS volunteer deployers — receives the message and re-transmits. Signal propagates another 40 km via two more repeaters on the flat salt pans of the Rann.

**Path D: Carry-Forward by Vehicle (parallel path)**

Meanwhile, a truck driver leaving Bhuj has IRIS running on his phone. He doesn't know he's carrying messages. As he drives the 300 km to Ahmedabad on NH27, his phone stores 47 messages — all P0 and P1 priority, all en route to reachable recipients. Every time he passes a town with cellular (Radhanpur, Patan, Mehsana), his phone bridges the mesh to internet and delivers queued messages. He is a physical data mule, unconsciously participating in message delivery.

**Path E: Satellite Gateway (critical infrastructure sites)**

NDRF's forward base at Anjar has a satellite terminal with IRIS gateway software. The gateway bridges the local BLE/LoRa mesh to a satellite uplink and relays high-priority messages to NDRF coordination centers in Ahmedabad and New Delhi. The channel is narrow (64 kbps) but P0 messages are tiny — 100 SOS messages fit in a single second of satellite bandwidth.

**Path F: Internet Bridge (partial connectivity zones)**

In Ahmedabad, 300 km away, IRIS nodes with internet connectivity receive messages via the satellite/truck relay paths and immediately re-broadcast via internet (WebSocket relay). NDRF coordination teams in Ahmedabad see the Bhuj SOS on their IRIS dashboard within 12 minutes of it being sent — despite no direct path existing between the two locations at the moment of transmission.

### 2.3 Outcome

In the first 4 hours:
- 847 SOS messages successfully delivered to emergency coordinators
- 12,000 P2 location messages exchanged between survivors and families
- 3 search and rescue teams deployed to GPS coordinates provided via IRIS
- 0 P0 messages lost due to protocol failures (some lost due to device battery death)

This is the scenario IRIS is designed for. Every design decision traces back to making this scenario work.

---

## 3. What "Communication Fabric" Means vs. "Chat App"

| Dimension | Chat App | Communication Fabric |
|-----------|----------|---------------------|
| **Infrastructure dependency** | Requires servers, internet | Zero infrastructure required |
| **Network topology** | Client-server (star) | Mesh (any topology) |
| **Failure mode** | Server down = app down | Degrades gracefully |
| **Transport** | TCP/IP over internet | BLE + Wi-Fi Direct + LoRa + Satellite + internet |
| **Message routing** | Fixed path through servers | Multi-path opportunistic |
| **Latency guarantee** | Low latency assumed | Tolerates hours-to-days delay |
| **Storage** | Server stores messages | Every device is a message store |
| **Identity** | Account + password | Cryptographic key pair |
| **Priority** | No concept | P0–P7 with hard guarantees for P0 |
| **Emergency** | No special handling | Emergency broadcast overrides everything |
| **Extensibility** | App-level features | Protocol-level — any app can use the fabric |

A chat app is an application. A communication fabric is infrastructure on which many applications run. IRIS is the fabric. The SOS button, the group chat, the location beacon, the emergency broadcast — these are all applications using the same underlying fabric.

---

## 4. Message Delivery Path Examples (Paths A–F Reference)

### Path A: Direct BLE Delivery
```
Sender → [BLE, ≤50m] → Recipient
Latency: <1 second
Reliability: High if in range
Throughput: ~250 kbps effective
Use case: Two people in same room
```

### Path B: BLE Hop Chain
```
Sender → [BLE] → Relay1 → [BLE] → Relay2 → [BLE] → Recipient
Max hops: 10 (configurable)
Latency: 1–30 seconds
Range: ~500m with 10 hops, open terrain
Use case: Survivors in collapse zone
```

### Path C: BLE → Wi-Fi Direct Bridge
```
Sender → [BLE] → Bridge Node → [Wi-Fi Direct] → Recipient cluster
Latency: 5–60 seconds
Throughput: Up to 25 Mbps on Wi-Fi Direct link
Use case: Small survivor group connected to larger mesh
```

### Path D: Store-Carry-Forward (Human Mule)
```
Sender → [local mesh] → Carrier Device (stored)
    [physical movement, minutes to hours]
Carrier Device → [local mesh at destination] → Recipient
Latency: Minutes to hours (depends on carrier movement)
Reliability: High (carried in device storage)
Use case: Ahmedabad→Kutch courier, truck driver, rescue worker
```

### Path E: LoRa Long-Range
```
Sender → [BLE/Wi-Fi] → LoRa Gateway → [LoRa, 868/915 MHz] → LoRa Repeater → [LoRa] → LoRa Gateway → [BLE/Wi-Fi] → Recipient
Range per hop: 5–30 km (terrain dependent)
Latency: 10 seconds – 5 minutes
Throughput: 250 bps – 5.5 kbps
Use case: Cross-district communication when all roads blocked
```

### Path F: Satellite Bridge
```
Sender → [local mesh] → Satellite Gateway → [Ku/Ka band uplink] → Satellite → [downlink] → Internet → Recipient
Latency: 600ms – 3 seconds
Throughput: 64 kbps – 10 Mbps (terminal dependent)
Cost: High (satellite bandwidth)
Use case: Critical coordination, government/NGO operations centers
```

---

## 5. Long-Term Global Vision

### Phase 1: India Proof of Concept (0–18 months)
Deploy in disaster-prone India: Gujarat, Odisha, Kerala, Assam, Uttarakhand. Partner with NDMA, state DMAs, Red Cross India. Prove the system in controlled drills and real emergencies.

### Phase 2: South/Southeast Asia Expansion (18–36 months)
Bangladesh (cyclone corridor), Nepal (earthquake zone), Indonesia (volcanic/tsunami zone), Philippines (typhoon corridor). These regions share India's infrastructure gaps and disaster frequency.

### Phase 3: Africa and Latin America (36–60 months)
Sub-Saharan Africa: cellular penetration without fiber backbone. East Africa: disaster + conflict zones. Brazil, Colombia: Amazon basin low-connectivity regions.

### Phase 4: Protocol Standardization
Submit IRIS protocol extensions to IETF DTN Working Group. Seek standardization of emergency priority channel, SOS message format, and gateway bridge protocol.

### Phase 5: OS-Level Integration
Work with Android and iOS to integrate IRIS mesh as an OS-level service (similar to Emergency SOS or Disaster Radio proposals) — automatic activation when emergency detected, independent of any specific app.

### The Ultimate Vision
Every smartphone on Earth is a potential relay node. No person is ever completely cut off from all communication. Emergency messages always find a path. The fabric is the global safety net, woven from the devices we already carry.

---

## 6. Why Now

- **Disaster frequency is increasing.** Climate change is making floods, cyclones, heatwaves, and wildfires more frequent and severe in India and globally.
- **Smartphone penetration has reached critical mass.** India has 750 million smartphone users. The hardware for a mesh exists in nearly every pocket.
- **BLE, Wi-Fi Direct, LoRa are now ubiquitous.** The radio hardware required is standard in modern phones and available cheaply for edge nodes.
- **Open source cryptography is mature.** The security primitives needed (Noise Protocol, ChaCha20, Ed25519) are battle-tested, patent-free, and well-implemented in Rust.
- **DTN research has produced practical algorithms.** Bundle Protocol v7 (RFC 9171), CBOR (RFC 8949), and epidemic routing research have given us the theoretical foundation.

The technology is ready. The need is urgent. IRIS is the integration.
