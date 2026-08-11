# Reference Architecture

## Purpose

This document defines the canonical reference architecture for the IRIS Resilient Communication Fabric. It serves as the authoritative description of how the system is deployed, how nodes interact, and how messages travel through the fabric under various infrastructure conditions.

## Scope

Covers physical node types, network topology patterns, message routing paths, and failure degradation strategies. This is the reference point for all implementation decisions.

## Non-Goals

- Does not specify wire formats (see `protocol/MESSAGE_ENVELOPE.md`)
- Does not specify transport internals (see `transports/`)
- Does not specify routing algorithms (see `routing/`)

---

## Reference Deployment: Ahmedabad → Kutch Earthquake Scenario

A major earthquake has struck Kutch district. Cellular towers are down. Internet backbone links are severed. Power is intermittent.

A person in Ahmedabad needs to send:

> "Help is on the way. ETA approximately 2 hours."

### Node Inventory in This Scenario

| Node | Type | Location | Connectivity |
|------|------|----------|-------------|
| Phone-A | User device | Ahmedabad | BLE, Wi-Fi (local), no Internet |
| Phone-B | Relay | Ahmedabad outskirts | BLE, Wi-Fi |
| Vehicle-C | Mobile relay | En route | BLE, Wi-Fi, satellite terminal |
| Gateway-D | LoRa gateway | Kutch edge | LoRa, BLE, no Internet |
| Phone-E | Recipient | Kutch | BLE, LoRa (via gateway) |

---

## Path A — Internet Survives (Best Case)

```
Phone-A
  │ Wi-Fi
  ▼
Local Gateway (Wi-Fi/Ethernet)
  │ Internet
  ▼
Cloud Relay (if configured)
  │ Internet
  ▼
Kutch Gateway (Internet + BLE)
  │ BLE
  ▼
Phone-E (Recipient)
```

**Characteristics:** Low latency (<5s), high bandwidth, standard operation.

---

## Path B — Mobile Vehicle Relay

```
Phone-A ──BLE──► Phone-B ──BLE──► Vehicle-C
                                      │
                               [Physically moves
                                to Kutch region]
                                      │
                                   BLE/LoRa
                                      ▼
                                  Gateway-D
                                      │ LoRa
                                      ▼
                                  Phone-E
```

**Characteristics:** High latency (hours), limited bandwidth, store-carry-forward.

---

## Path C — Satellite Gateway

```
Phone-A ──BLE──► Phone-B ──Wi-Fi──► Satellite Gateway
                                          │ Satellite
                                          ▼
                                    Kutch Ground Station
                                          │ LoRa/BLE
                                          ▼
                                      Phone-E
```

**Characteristics:** High latency (600ms+), very low bandwidth (Iridium ~2400bps), high cost.

---

## Path D — Store and Wait

```
Phone-A sends message → stored locally
          [WAIT for connectivity]
          ▼
Gateway appears (Internet restored)
          │
          ▼
Message forwarded → Delivered to Phone-E
```

**Characteristics:** Indeterminate latency, reliable eventual delivery.

---

## Path E — Fragmented Network (Vehicle Bridge)

```
Region A (Ahmedabad mesh)    Region B (Kutch mesh)
  Phone-A                        Phone-E
  Phone-B           X            Gateway-D
  Phone-C                        Phone-F

Vehicle-G moves between A and B:
  - Connects to Region A → downloads pending messages
  - Drives to Region B → uploads messages, downloads replies
```

**Characteristics:** Latency = transit time (minutes to hours), high reliability.

---

## Path F — Complete Infrastructure Failure

```
Phone-A stores message with TTL=72h
[No paths available — message waits]
[When ANY path appears, message forwards]
```

**Correct system promise:**
> Communication continues whenever a viable physical path exists. Messages survive indefinitely (up to TTL) waiting for a path.

---

## Node Types

### User Device (Phone/Tablet)

- Primary role: message originator and recipient
- Secondary role: relay for nearby nodes
- Transports: BLE (always), Wi-Fi (when available), cellular (gateway)
- Constraints: battery, OS background restrictions, user must consent to relay

### Dedicated Relay Node

- Role: always-on relay, no UI
- Hardware: Raspberry Pi, laptop, embedded device
- Transports: BLE, Wi-Fi, Ethernet, LoRa (with module)
- Power: mains or solar+battery
- No user: headless operation, remote management

### Mobile Relay (Vehicle)

- Role: store-carry-forward between disconnected regions
- Hardware: phone or dedicated device in vehicle
- Transports: BLE, Wi-Fi, cellular (where available), satellite (if equipped)
- Special: physical movement bridges network partitions

### Internet Gateway

- Role: bridge between mesh and Internet
- Hardware: any device with Internet and local wireless
- Function: accepts messages from mesh → forwards via Internet → returns replies
- Availability: opportunistic (only when Internet available)

### LoRa Gateway

- Role: long-range wireless backbone node
- Hardware: LoRa module + Raspberry Pi or similar
- Function: accepts BLE/Wi-Fi messages → transmits via LoRa → receives LoRa → distributes via BLE/Wi-Fi
- Range: 5–15 km rural

### Satellite Gateway

- Role: last-resort long-range relay
- Hardware: satellite terminal (Iridium GO!, VSAT, Starlink)
- Function: tunnels emergency messages (P0–P2) via satellite
- Constraints: cost per message, low bandwidth, regulatory licensing required

---

## Network Topology Patterns

### Star (Normal Operation)

```
        Gateway (Internet)
       /    |    \
    Phone  Phone  Phone
```

All devices connect through an Internet gateway. Fast, low overhead.

### Mesh (Internet Unavailable)

```
Phone ── Phone ── Phone
  |         |         |
Phone     Phone     Phone
```

Peer-to-peer BLE/Wi-Fi mesh. Epidemic routing. Higher overhead.

### Hybrid (Partial Connectivity)

```
        Gateway (Internet)
       /
    Phone ── Phone ── Phone
                |
              Phone (isolated)
```

Some nodes have gateway access, others rely on mesh relay.

### Partitioned (Disaster)

```
Cluster A          Cluster B
Phone-Phone   X    Phone-Phone
    |               |
Phone           Phone

Vehicle bridges gap periodically
```

Store-carry-forward between partitions.

---

## Capability Matrix by Node Type

| Capability | User Phone | Dedicated Relay | Vehicle | LoRa GW | Satellite GW |
|-----------|-----------|----------------|---------|---------|-------------|
| Originate | ✓ | ✗ | ✗ | ✗ | ✗ |
| Receive | ✓ | ✗ | ✗ | ✗ | ✗ |
| BLE Relay | ✓* | ✓ | ✓ | ✓ | ✗ |
| Wi-Fi Relay | ✓* | ✓ | ✓ | ✓ | ✗ |
| LoRa Relay | ✗ | ✓ | ✓ | ✓ | ✗ |
| Satellite | ✗ | ✗ | ✓ | ✗ | ✓ |
| Internet GW | ✓* | ✓ | ✓ | ✓ | ✓ |
| Always-on | ✗ | ✓ | ✓ | ✓ | ✓ |
| Store-Carry | ✓ | ✓ | ✓ | ✓ | ✓ |

*Subject to OS background restrictions and battery policy.

---

## References

- `architecture/LAYER_MODEL.md` — detailed layer breakdown
- `architecture/NODE_MODEL.md` — node state machine
- `architecture/FAILURE_ARCHITECTURE.md` — failure behavior
- `transports/TRANSPORT_ABSTRACTION.md` — transport layer design
- `routing/STORE_CARRY_FORWARD.md` — carry-forward mechanics
