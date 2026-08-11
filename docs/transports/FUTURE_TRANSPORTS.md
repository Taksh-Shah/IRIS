# Future Transports Research Agenda

## Overview

This document tracks transport technologies that are not yet implemented in IRIS but
warrant active research. For each candidate, we assess: technical feasibility,
regulatory posture, hardware availability, and implementation timeline.

The core architecture principle: new transports are added via the `TransportAdapter`
trait without modifying routing or protocol layers. This document informs the roadmap
but does not commit to timelines.

---

## UWB (Ultra-Wideband)

### Standard
IEEE 802.15.4z (2020). Pulse-based radio at 6–9 GHz, channel bandwidths of 500 MHz–1 GHz.

### Capabilities
- Ranging accuracy: ±10 cm (vs ±3m for BLE, ±30m for GPS in urban)
- Theoretical throughput: ~100 Mbps (not used for ranging; separate data channel)
- Range: 10–50 meters (LOS), 5–15 meters (NLOS through walls)
- Power: ~50–100 mA during active ranging

### Hardware Availability

| Device | UWB Chip | API |
|--------|----------|-----|
| iPhone 11+ | Apple U1 | Core Nearby Interactions (iOS 14+) |
| iPhone 13+ | Apple U1 | Core Nearby Interactions, Accessory ranging |
| Pixel 6, 7, 8 | Qorvo DW3700 | Android UWB API (Android 12+) |
| Samsung S21 Ultra+ | NXP SR100T | Android UWB API |
| Samsung S22+ | NXP SR100T | Android UWB API |

India-relevant devices: Samsung Galaxy (S21+/A-series UWB), Pixel (limited availability).
Apple UWB is iOS-only. Penetration rate of UWB-capable Android phones in India: <5% (2025).

### Android UWB API

```kotlin
// Requires: com.google.android.uwb (Android 12+)
// IMPORTANT: API is restricted — requires partnership with Google for production use
// Available without restriction only in developer mode / testing

val uwbManager = getSystemService(UwbManager::class.java)
val sessionScope = uwbManager.controleeSessionScope()

val partnerParameters = UwbRangingParams.Builder()
    .setSessionId(sessionId)
    .setUwbAddress(UwbAddress(localAddress))
    .setRangingParameters(
        UwbRangingParams.Builder()
            .setSessionId(sessionId)
            .setSessionKeyInfo(sessionKey)
            .build()
    )
    .build()

// Ranging updates arrive as Flow<RangingResult>
sessionScope.prepareSession(partnerParameters).collect { result ->
    when (result) {
        is RangingResult.RangingResultPosition -> {
            val distance = result.position.distance?.value
            // Use distance for routing quality metric
        }
        is RangingResult.RangingResultPeerDisconnected -> { /* handle */ }
    }
}
```

### IRIS Use Case for UWB

Not primarily for data transfer — for **precision routing** and **trust establishment**:
1. UWB ranging confirms physical proximity (anti-spoofing for BLE)
2. Two-party key ceremony: "point your phone at mine to confirm identity"
3. Future: directional antenna on LoRa nodes, UWB for aiming assistance

**Assessment**: Implement when UWB-capable device penetration exceeds 20% in India.
Estimated timeline: 2027–2028.

---

## Thread

### Standard
IEEE 802.15.4g mesh networking protocol. IPv6-based (6LoWPAN). Designed for smart home IoT.
Operates at 2.4 GHz, 250 kbps PHY rate, ~40 kbps effective.

### Matter Protocol

Thread is the primary transport for Matter (formerly Project CHIP), backed by
Apple, Google, Amazon, Samsung. Matter devices (smart home) run over Thread.

### Why Thread Is NOT Suitable for IRIS (Today)

1. **Not available on phones**: Thread radios are in smart home hubs, not smartphones.
   Thread border routers (Apple HomePod, Google Nest Hub) bridge Thread to IP.
2. **Range**: 10–30 meters per hop, designed for home/building not disaster area.
3. **Throughput**: 40 kbps is below BLE for IRIS use cases.
4. **Ecosystem**: requires Thread Border Router infrastructure.

### Potential IRIS Use Case

Edge nodes (Raspberry Pi + Thread radio) could mesh with smart home devices to
extend IRIS reach into buildings during disaster:

```
IRIS Mesh (BLE/Wi-Fi) → IRIS Edge Node → Thread Border Router → Thread Devices
```

**Assessment**: Low priority. Investigate if smart home infrastructure survives
disasters better than expected. Research phase only; no implementation planned.

---

## 5G Sidelink (ProSe / D2D)

### Standard
3GPP Release 12 introduced ProSe (Proximity Services) / D2D (Device-to-Device).
Release 16 extended sidelink for V2X (vehicle-to-everything).
Release 17+ adds sidelink relay for coverage extension.

### Capability
- Direct UE-to-UE communication without base station
- Uses licensed 5G NR spectrum
- Range: 50–500 meters
- Throughput: up to 100 Mbps (5G NR sidelink)
- Latency: 1–10 ms

### Why Unavailable (2025)

1. **Carrier must enable it**: ProSe sidelink requires network operator to activate the feature.
   No Indian operator has activated it for consumer devices.
2. **No app API**: Android and iOS do not expose sidelink to applications.
3. **Hardware**: most consumer phones have the sidelink capability in modem firmware
   but carriers disable it via configuration.
4. **Regulatory**: licensed spectrum D2D requires operator/regulator approval.

### When It Becomes Available

3GPP Rel-17 sidelink relay is being standardized for public safety (3GPP TS 23.304).
NDRF/disaster management use case is explicitly part of the standardization work.
Timeline to India availability: 2027–2029, likely via BSNL/TRAI public safety initiative.

**Assessment**: Monitor 3GPP and TRAI for public safety ProSe regulation. When Android
exposes an API, implementing a `SidelinkTransport` should be straightforward given
IRIS's transport adapter architecture.

---

## NFC (Near Field Communication)

### Standard
ISO 14443, ISO 15693, NFC Forum standards. 13.56 MHz, 4 cm range, 106/212/424 kbps.

### NDEF (NFC Data Exchange Format)

Standard message format for NFC:
```
NDEF Record:
  TNF (Type Name Format): 0x01 (Well Known)
  Type: "T" (text) or "U" (URI) or custom
  Payload: up to ~32 KB per record
```

### Android NFC

```kotlin
// Fully available, no special permission beyond NFC
val nfcAdapter = NfcAdapter.getDefaultAdapter(context)

// Peer-to-peer (Android Beam was deprecated in Android 10)
// Current approach: NFC tag emulation (HCE — Host Card Emulation)
// or reader/writer mode for IRIS bundle exchange

// Write IRIS bundle to NFC tag
val ndefMessage = NdefMessage(
    NdefRecord.createApplicationRecord("io.iris.node"),
    NdefRecord(
        NdefRecord.TNF_MIME_MEDIA,
        "application/iris-bundle".toByteArray(),
        ByteArray(0),
        irisBundle.toBytes().take(32000).toByteArray() // 32KB NFC limit
    )
)
```

### iOS NFC (Core NFC)

```swift
// iOS 11+: NFC Reader only (cannot write or emulate)
// iOS 13+: Read/write to NFC tags via NFCNDEFReaderSession
// iOS CANNOT: emulate NFC card (only NFC tags can be emulated via HCE — Android only)

let session = NFCNDEFReaderSession(delegate: self, queue: nil, invalidateAfterFirstRead: false)
session.begin()

func readerSession(_ session: NFCNDEFReaderSession, didDetectNDEFs messages: [NFCNDEFMessage]) {
    for message in messages {
        for record in message.records {
            if record.typeNameFormat == .media &&
               String(data: record.type, encoding: .utf8) == "application/iris-bundle" {
                IrisBundleImporter.import(record.payload)
            }
        }
    }
}
```

### IRIS NFC Use Case

Very limited utility for mesh communication (4 cm range). Primary use case:

1. **Key exchange tap**: two phones tap to exchange public keys (trust establishment)
2. **Emergency token**: NFC tag pre-programmed with P0 SOS message; any NFC reader
   (including non-IRIS phone using NFC reader app) can trigger emergency relay
3. **Medical wristband**: patient info encoded in NFC tag; first responder taps with
   any NFC reader

**Assessment**: Implement key exchange tap (P1) and NFC tag SOS beacon (P2). Not a
mesh transport — a trust and bootstrap tool only.

---

## TVWS (TV White Space)

### Overview

Unused UHF television broadcast spectrum (470–790 MHz in India, after analog TV
shutoff). Characteristics:
- Range: 5–15 km with modest antennas, 30+ km LOS
- Penetration: excellent (VHF/UHF propagates through buildings and foliage)
- Throughput: 1–20 Mbps (depends on channel bandwidth and modulation)
- Requires geographic database lookup (to avoid interfering with actual TV broadcasts)

### India Regulatory Status

TRAI released consultation paper on TVWS (2018). WPC has not yet allocated TVWS
for unlicensed use. Most devices require type approval and database query to operate.

### Hardware

- Microsoft Research: custom TVWS hardware (discontinued)
- Carlson Wireless: licensed TVWS hardware (enterprise, expensive)
- No consumer devices support TVWS as of 2025

### IRIS Relevance

TVWS is ideal for the IRIS use case (long-range, low-throughput emergency comms)
but is not deployable without hardware and regulatory changes. If TRAI opens TVWS
for disaster response, this becomes the highest-priority future transport.

**Assessment**: Monitor TRAI/WPC. No implementation until regulatory clearance.
Research a simulated TVWS transport for protocol testing.

---

## Ham Radio Digital Modes

### Overview

Amateur radio digital modes enable text/data communication over HF/VHF/UHF ham bands:

| Mode | Bandwidth | Range | Notes |
|------|-----------|-------|-------|
| APRS | 1200 baud, 2m | 100+ km (via digipeater) | Position/text, no E2EE |
| Winlink | 2400 baud, HF | Global | Email over radio, requires RMS relay |
| JS8Call | 25 Hz, HF | 1000+ km | Store-forward, low power |
| WSPR | 200 Hz, HF | Global | Propagation beacon, not comms |
| Vara HF | 2300 Hz, HF | 2000+ km | High-throughput HF mode |

### Why IRIS Cannot Use Ham Radio in Normal Operation

1. **License requirement**: amateur radio operation requires license (India: ASOC/NIAR).
   Consumer devices cannot legally use ham bands.
2. **No encryption**: ham radio regulations prohibit encryption in most countries
   including India (ITU Radio Regulations Article 25.2A). IRIS's E2EE is incompatible.
3. **Commercial use prohibition**: ham bands prohibit commercial communication.

### Exception: Disaster Communication

During declared national emergencies, some relaxation of ham radio regulations may
apply. NDRF has licensed ham radio operators embedded in teams.

**IRIS integration for licensed operators**: a separate `HamRadioGateway` module
(opt-in, requires user to confirm license, disables E2EE on that transport, UI
displays "UNENCRYPTED" prominently).

**Assessment**: Out of scope for general IRIS. Research for integration with India's
ham radio emergency communication network (IARU Region 3 EMCOMM).

---

## Plugin Transport Architecture

All future transports are added without modifying the core routing or protocol engine:

```rust
// TransportAdapter trait — implement this to add any transport
pub trait TransportAdapter: Send + Sync {
    fn transport_id(&self) -> TransportId;
    fn capabilities(&self) -> TransportCapabilities;
    fn is_available(&self) -> bool;
    fn estimated_bandwidth_bps(&self) -> u64;
    fn estimated_latency_ms(&self) -> u32;
    fn max_message_size_bytes(&self) -> usize;
    fn duty_cycle_remaining(&self) -> f32;  // 1.0 = no restriction

    async fn discover_neighbors(&self) -> Vec<NeighborCandidate>;
    async fn connect(&self, candidate: &NeighborCandidate) -> Result<Box<dyn TransportSession>>;
}

pub trait TransportSession: Send + Sync {
    async fn send(&mut self, frame: &TransportFrame) -> Result<()>;
    async fn recv(&mut self) -> Result<TransportFrame>;
    async fn close(&mut self);
}
```

New transports: implement these two traits, register in `TransportRegistry`. No other
changes required. Simulation and testing use `SimulatedTransport<T>` wrapper.
