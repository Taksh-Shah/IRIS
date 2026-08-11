# Bluetooth Mesh

## Overview

This document covers the Bluetooth Mesh SIG standard, evaluates its applicability to IRIS, and documents the decision to **not** use the Bluetooth Mesh SIG profile. Instead, IRIS implements mesh-like behavior at the application layer over point-to-point BLE connections.

---

## Bluetooth Mesh SIG Standard

### Architecture

Bluetooth Mesh (BT Mesh) is a networking standard defined by the Bluetooth Special Interest Group (SIG), published in 2017 and updated in 2019 (Mesh Profile 1.0.1) and 2021 (Mesh Profile 1.1). It enables many-to-many communication among Bluetooth devices using a publish-subscribe message model.

```
┌─────────────────────────────────────────────────────────────────┐
│                     Bluetooth Mesh Stack                         │
├─────────────────────────────────────────────────────────────────┤
│  Application Layer    Models (Generic On/Off, Sensor, etc.)     │
├─────────────────────────────────────────────────────────────────┤
│  Foundation Models    Configuration, Health                     │
├─────────────────────────────────────────────────────────────────┤
│  Access Layer         Application key encryption, opcodes       │
├─────────────────────────────────────────────────────────────────┤
│  Upper Transport      Segmentation, reassembly, friend messages │
├─────────────────────────────────────────────────────────────────┤
│  Lower Transport      Segmentation for network PDUs             │
├─────────────────────────────────────────────────────────────────┤
│  Network Layer        Network key encryption, relay, TTL        │
├─────────────────────────────────────────────────────────────────┤
│  Bearer Layer         GATT Bearer (GATT proxy) / Adv Bearer    │
├─────────────────────────────────────────────────────────────────┤
│  BLE Physical Layer   BLE 4.0/5.0                              │
└─────────────────────────────────────────────────────────────────┘
```

### Publish-Subscribe Model

In BT Mesh, nodes publish messages to **group addresses** and subscribe to receive messages from group addresses. This is fundamentally different from unicast networking:

- A light switch publishes to address `0xC000` (a group).
- All light bulbs subscribe to `0xC000`.
- When the switch is pressed, all bulbs receive the message — no routing tables, no per-device addressing needed.

Group addresses can be assigned to geographic areas, roles, or device types. A "floor 3 lights" group and an "all lights" group can coexist.

### Network Keys and Application Keys

BT Mesh uses two key tiers:
- **Network Key (NetKey):** Encrypts the network layer. All devices in the same mesh network share the same NetKey (or a subnet NetKey). Any device with the NetKey can relay messages without reading their content.
- **Application Key (AppKey):** Encrypts the application/access layer. Bound to a NetKey. Devices that share an AppKey can read message content. Light switches and bulbs share an AppKey; thermostats use a different AppKey.

### Provisioning

Before a device can join a BT Mesh network, it must be **provisioned** by a Provisioner. Provisioning:
1. The provisioner discovers the unprovisioned device (beacons on advertising channel 39).
2. Provisioner and device perform an OOB (Out-of-Band) or in-band authentication exchange.
3. Provisioner distributes the NetKey, assigns a Unicast Address to the device.
4. Provisioner configures the device: assigns AppKeys, sets subscriptions and publication addresses.

This process requires a dedicated provisioner role (typically a phone app or commissioning tool) and persistent network state. A new device cannot join by itself — a provisioner must be present.

### Node Roles

| Role             | Description                                                    |
|------------------|----------------------------------------------------------------|
| **Relay Node**   | Retransmits received messages to extend range                  |
| **Proxy Node**   | Bridges GATT clients (phones without Mesh stack) to mesh       |
| **Friend Node**  | Stores messages for Low Power Nodes while they sleep           |
| **Low Power Node**| Polls Friend Node for stored messages; sleeps between polls   |

Most mesh deployments use dedicated hardware that plays specific roles. A typical commercial lighting deployment: smart lights as relay nodes, a gateway device as proxy, battery sensors as low-power nodes.

### Message Format and Constraints

- Maximum Access Layer payload: **11 bytes** (unsegmented) or **384 bytes** (segmented, maximum)
- Maximum network layer message: **29 bytes** per PDU
- Segmentation: messages > 11 bytes are segmented into multiple PDUs at the lower transport layer
- Relay TTL: default 3–5 hops. Maximum 127.
- Message cache size: devices maintain a cache of recently seen messages to prevent relay loops. Cache size varies by implementation (typically 32–128 entries).

---

## Why IRIS Does NOT Use Bluetooth Mesh SIG Profile

After evaluation, the IRIS team decided against the BT Mesh SIG profile. The decision is documented here for future reference.

### Reason 1: Provisioning Is Incompatible with Disaster Response

BT Mesh requires every device to be provisioned before joining the network. In disaster scenarios:

- Survivors may have any Android or iOS phone.
- There is no pre-established infrastructure to provision devices.
- There is no provisioner available in field conditions.
- Provisioning requires a stable identity assignment process with a persistent Provisioner.

IRIS requires **zero-configuration joining.** A device that downloads the IRIS app and opens it must immediately be able to communicate with nearby IRIS nodes — with no provisioning step, no pre-registration, and no administrator involvement. BT Mesh fundamentally cannot support this requirement.

### Reason 2: Fragmented Phone Support

As of 2024, mainstream BLE Mesh support on Android and iOS is limited:

**Android:**
- The Android Bluetooth stack (Fluoride/Gabeldorsche) does not natively implement the BT Mesh profile.
- Third-party libraries (nRF Mesh by Nordic Semiconductor, SIG Mesh SDK) exist but require Bluetooth LE background operation and have known stability issues on Android 12+.
- The Android Mesh stack requires `bluetooth-central` and `bluetooth-peripheral` background modes that are aggressively restricted on modern Android.
- Tested: nRF Mesh SDK on Pixel 7 (Android 14) showed 15–30% provisioning failure rate in field tests due to BLE background scan throttling.

**iOS:**
- Apple provides no public BT Mesh API.
- Third-party implementations use CoreBluetooth directly but cannot leverage the iOS BLE Mesh acceleration (used internally by HomeKit).
- Background operation is severely limited — iOS kills background BLE Mesh nodes within minutes.
- For IRIS, which must run on both Android and iOS with feature parity, this is a showstopper.

**Conclusion:** A cross-platform BT Mesh implementation that works reliably on both Android (background scanning) and iOS (background advertising) does not exist as a viable library today.

### Reason 3: Designed for IoT, Not Ad-Hoc Human Communication

BT Mesh was designed for smart building automation: fixed devices with known roles, stable network topology, messages small enough for sensor data. IRIS requirements are fundamentally different:

| Requirement         | BT Mesh Design              | IRIS Requirement               |
|---------------------|-----------------------------|--------------------------------|
| Network topology    | Fixed, preconfigured        | Fully dynamic, ad-hoc          |
| Message size        | 11–384 bytes                | Up to 64 KB                    |
| Node discovery      | Provisioner assigns address | Self-assigned cryptographic ID |
| Routing             | Flood + relay               | DTN store-carry-forward        |
| Node lifetime       | Permanent (IoT device)      | Ephemeral (human in the field) |
| Security model      | Shared NetKey (symmetric)   | Per-pair asymmetric E2EE       |

### Reason 4: Complex Key Management Without Infrastructure

BT Mesh key management requires:
- NetKey distribution to all provisioned nodes.
- AppKey binding and distribution.
- Key refresh procedures when a node is compromised.
- Key revocation when a device leaves.

In a disaster network without infrastructure, there is no mechanism to:
- Securely distribute new NetKeys to all nodes.
- Detect and revoke compromised nodes.
- Handle key refresh across a partitioned network.

IRIS uses self-sovereign cryptographic identity (Ed25519 keypairs generated on device) that requires no key distribution infrastructure. Each message is encrypted to the recipient's public key. No shared secret is required.

### Reason 5: Relay Flooding Does Not Scale for Emergency Traffic

BT Mesh relay flooding: every relay node rebroadcasts every message it receives (within TTL). With N relay nodes and TTL=5, a single message generates O(N × TTL) retransmissions. In a disaster scenario with 200 nodes:

- 200 nodes × 5 TTL hops = 1000 total transmissions per message.
- BLE advertisement channel capacity: ~600 packets/second (3 channels, 200 packets/sec each).
- At 10 messages/second across the network: 10,000 transmissions/second required, exceeding channel capacity by 16×.

IRIS uses selective forwarding based on routing table state, dramatically reducing forwarding overhead compared to flood-based relay.

---

## IRIS Application-Layer Mesh Design

Instead of BT Mesh, IRIS implements mesh-like behavior at the application layer over standard BLE point-to-point connections.

```
                    Application-Layer Mesh (IRIS Routing)
                    ─────────────────────────────────────
    Node A ──BLE─── Node B ──BLE─── Node C ──BLE─── Node D
           GATT pair       GATT pair       GATT pair
    
    A wants to message D:
    1. A stores message.
    2. A connects to B (best available neighbor by routing score).
    3. A forwards message to B via GATT write.
    4. B stores message, connects to C when B discovers C.
    5. C connects to D and delivers.
    6. D sends delivery ACK back along reverse path.
```

### Comparison: BT Mesh vs IRIS Application-Layer Mesh

| Property              | BT Mesh SIG           | IRIS Application Mesh        |
|-----------------------|-----------------------|------------------------------|
| Discovery             | Unprovisioned beacons | IRIS BLE advertising         |
| Joining               | Provisioner required  | Auto (any IRIS device)       |
| Addressing            | 16-bit unicast + group| 32-byte public key           |
| Routing               | Flood with relay TTL  | DTN routing (PRoPHET/SCF)    |
| Message size          | 384 bytes max         | Up to 64 KB (fragmented)     |
| Encryption            | NetKey + AppKey       | Per-pair E2EE (ChaCha20)     |
| Background (Android)  | Broken in practice    | Foreground service           |
| Background (iOS)      | Unusable              | Limited (bluetooth modes)    |
| Provisioning overhead | Required              | None                         |
| Key management        | Centralized           | Decentralized (per-device)   |
| Flood overhead        | O(N × TTL)            | O(routing_table_size)        |

### Benefits of Application-Layer Approach

1. **Full routing control:** IRIS can implement DTN routing algorithms (PRoPHET, spray-and-wait) that are impossible in BT Mesh.
2. **Dynamic topology:** No provisioning step. Any device can join and leave at any time.
3. **Platform independence:** Works on both Android and iOS using standard CoreBluetooth and Android BLE APIs.
4. **Message size:** No 384-byte limit. IRIS handles its own fragmentation and reassembly.
5. **Security model:** End-to-end encryption per message pair; no shared network key.
6. **Store-carry-forward:** Messages survive connectivity gaps. BT Mesh has no persistence model.
7. **Testability:** The application-layer mesh can be fully simulated in unit tests without BLE hardware.

---

## Future Consideration: BT Mesh for IoT Peripheral Devices

While IRIS phones use application-layer mesh, BT Mesh remains potentially valuable for **dedicated peripheral hardware:**

- Fixed relay nodes (solar-powered, mounted on buildings) could use BT Mesh for inter-device relay.
- LoRa gateway devices could expose a BT Mesh interface for nearby IoT sensors.
- These are separate IoT device deployments, not the primary IRIS phone mesh.

If IRIS deploys fixed infrastructure hardware in future phases, BT Mesh SIG profile should be reconsidered for that specific use case, with proper provisioner infrastructure in place.

---

## Decision Record

**Decision:** IRIS does not use Bluetooth Mesh SIG profile.  
**Date:** Project inception.  
**Alternatives considered:** BT Mesh SIG, nRF Mesh SDK, custom BLE advertising-based mesh, application-layer mesh over GATT connections.  
**Decision:** Application-layer mesh over point-to-point BLE GATT connections.  
**Rationale:** Provisioning incompatibility, poor phone platform support (especially iOS), design mismatch with DTN requirements, message size limits, and key management complexity.  
**Revisit trigger:** If Apple opens BT Mesh APIs on iOS, or if a reliable cross-platform BT Mesh library emerges that handles all background operation requirements.
