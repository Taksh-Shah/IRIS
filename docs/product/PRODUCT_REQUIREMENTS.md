# Product Requirements

**Status:** Draft — pending stakeholder review  
**Last updated:** 2026-08-11  
**Owner:** Product  
**Version:** 0.3  

---

## 1. Requirement Conventions

**Priority:** P0 (must-have for MVP launch) | P1 (must-have for Alpha) | P2 (Beta) | P3 (GA)  
**Acceptance criteria:** Testable conditions that define "done"  
**Dependencies:** Other requirements that must be complete first  

---

## PRD-001: SOS Beacon

**Priority:** P0  
**Delivery target:** 30-second end-to-end delivery within a 3-hop mesh  

### User Story

As a disaster survivor with limited time and cognitive capacity, I need to send an emergency SOS with a single gesture from a locked screen so that rescuers can locate and assist me without requiring me to navigate a complex UI.

### Description

The SOS beacon is a minimal, authenticated location broadcast. When triggered, it sends a bundle containing:
- Sender's IRIS Node ID
- GPS coordinates (or last known coordinates if GPS unavailable)
- Accuracy estimate (meters)
- Timestamp
- Optional: one-line free-text note (up to 128 characters, pre-composable)

The SOS bundle is:
- Signed with the sender's Ed25519 key
- Transmitted using epidemic routing (maximum delivery probability)
- Given the highest priority (P0) in the bundle store
- TTL: 72 hours

### Acceptance Criteria

- [ ] SOS can be triggered from a locked screen via 3 consecutive power button presses within 2 seconds (Android emergency gesture)
- [ ] SOS sent within 5 seconds of trigger (excluding GPS fix acquisition)
- [ ] SOS received by at least one peer within 30 seconds when a relay is within 2 BLE hops
- [ ] Haptic + visual + audio confirmation shown within 2 seconds of trigger
- [ ] SOS includes location with accuracy ≤ 100m when GPS fix available
- [ ] SOS falls back to cell-ID location (accuracy: 300–3000m) when GPS unavailable
- [ ] SOS falls back to "location unknown" message when no location available (still sent)
- [ ] SOS visible in the map view of receiving nodes within 60 seconds
- [ ] Sending a second SOS within 5 minutes prompts "SOS already sent; send again?" (anti-spam)
- [ ] SOS accessible to TalkBack (Android screen reader) and VoiceOver (iOS)

### Dependencies

- PRD-002 (location acquisition)
- Transport layer: BLE and Wi-Fi Direct must be operational
- Bundle store must handle P0 priority correctly

---

## PRD-002: Location Sharing

**Priority:** P0  
**Accuracy target:** GPS: ≤100m; cell-ID fallback: ≤3km

### User Story

As a disaster survivor and as a rescuer, I need to know where I and other mesh participants are located so that we can coordinate evacuation and rescue.

### Description

Location sharing is opt-in per session. When enabled, the IRIS app broadcasts the user's location as a signed Location bundle at a configurable interval (default: 60 seconds). Location data includes:
- GPS lat/lng (WGS84)
- Horizontal accuracy estimate (meters)
- Altitude (meters, if available)
- Timestamp

Location source priority:
1. Fused GPS (Android location API: GPS + Wi-Fi + sensors)
2. GPS only (if fused provider unavailable)
3. Cell-ID (if GPS unavailable)
4. Last known location with age stamp

### Acceptance Criteria

- [ ] Location toggle is accessible within 2 taps from main screen
- [ ] Location broadcast begins within 10 seconds of toggle on
- [ ] Location accuracy displayed to user (e.g., "±50m")
- [ ] Location source displayed to user (GPS / Cell / Last known)
- [ ] Location history not stored on device beyond current session
- [ ] Location cleared when toggle turned off (no background persistence)
- [ ] Other users' locations displayed on offline map with timestamp
- [ ] Location bundles older than 10 minutes displayed with "possibly outdated" indicator
- [ ] Location sharing stops automatically when battery < 15% (low battery mode)

### Dependencies

- Android: ACCESS_FINE_LOCATION permission
- iOS: CoreLocation with always-on permission
- PRD-004 (offline map for displaying locations)

---

## PRD-003: Emergency Text Messaging

**Priority:** P3 (GA)  
**Note:** P3 because routing works without it; text is a quality-of-life feature for extended disasters

### User Story

As a disaster survivor separated from my family, I need to send and receive text messages through the mesh so that I can coordinate without cellular or internet connectivity.

### Description

Text messaging in IRIS is end-to-end encrypted, store-carry-forward DTN messaging. Messages are:
- Encrypted with recipient's public key (ChaCha20-Poly1305 + X25519 key exchange)
- Signed by sender (Ed25519)
- Routed via PRoPHET (opportunistic delivery)
- TTL: 7 days (default; user-configurable)

Group messaging is supported via a group keypair distributed out-of-band or via a group admin's signed member list.

### Acceptance Criteria

- [ ] 1:1 text message composed and sent within 3 taps
- [ ] Message delivery status: Sent / Relayed / Delivered (with timestamps)
- [ ] Message stored in encrypted SQLite store
- [ ] Message TTL displayed to sender
- [ ] Group messages: create group, add members (by Node ID or QR scan)
- [ ] Group messages: any member can read; non-members see encrypted blob
- [ ] Messages up to 4096 bytes (text) per bundle
- [ ] Larger messages split into fragments automatically
- [ ] Received messages push-notified via Android foreground service notification
- [ ] Offline: all of the above work without internet

### Dependencies

- PRD-001 (SOS — establishes the transport and bundle stack)
- Cryptographic layer: X25519 key exchange, ChaCha20-Poly1305 encryption
- PRoPHET routing operational

---

## PRD-004: Offline Map with Evacuation Routes

**Priority:** P1  
**Tile source:** OpenStreetMap (pre-cached, India-specific)

### User Story

As a disaster survivor, I need to see my location and nearby evacuation routes on a map that works without internet so that I can navigate to safety even when cell towers are down.

### Description

IRIS includes a pre-cached offline map covering the user's district (downloaded before disaster, or bundled for high-risk districts). The map displays:
- User's current location (blue dot)
- Other IRIS nodes' shared locations (with timestamps)
- Pre-loaded evacuation routes (polylines from NDMA/state DMA data)
- Shelter locations (pre-loaded point-of-interest layer)
- Flood zones / restricted areas (if provided by authority)

Map tiles are from OpenStreetMap (Mapbox Vector Tiles format), stored locally. Map can be updated via Wi-Fi/cellular pre-disaster.

### Acceptance Criteria

- [ ] Map renders within 3 seconds of opening on a mid-range Android device (Snapdragon 680)
- [ ] Map functions fully offline (no tile requests to internet while offline)
- [ ] Evacuation routes pre-loaded for at least the user's current district
- [ ] District-level map tile package ≤ 500 MB for densely mapped areas
- [ ] User can download maps for additional districts while online
- [ ] Other users' locations appear on map within 30 seconds of location bundle receipt
- [ ] Shelter locations searchable by name
- [ ] Map rotates to compass heading (if device has magnetometer)
- [ ] Map accessible in high-contrast mode (accessibility requirement)

### Dependencies

- PRD-002 (location sharing — to display own and others' locations)
- Pre-cached tile data pipeline (build-time or pre-disaster download)

---

## PRD-005: Authority Broadcast Reception

**Priority:** P1  
**Source:** Verified government / emergency management entities

### User Story

As a disaster survivor, I need to receive official emergency instructions from NDRF, SDRF, and local authorities through the mesh network so that I receive accurate information even when cellular and internet are unavailable.

### Description

Authority broadcasts are signed bundles from pre-provisioned authority keypairs. The authority's public key is embedded in the IRIS app at build time (for NDRF, NDMA, and optionally state SDRFs). Broadcasts include:
- Authority name and role
- Message text (up to 2048 characters)
- Validity period (start + end timestamp)
- Geographic scope (if applicable: lat/lng bounding box)
- Priority: P0 (full-screen takeover) or P1 (notification banner)

P0 authority broadcasts trigger a full-screen display that cannot be dismissed for 30 seconds.

### Acceptance Criteria

- [ ] Verified authority broadcast (valid Ed25519 signature from known authority key) displayed with "VERIFIED OFFICIAL MESSAGE" indicator
- [ ] Unverified broadcast (unknown key) displayed with "UNVERIFIED — source unknown" warning
- [ ] P0 broadcast triggers full-screen takeover with audio alert
- [ ] P0 broadcast cannot be dismissed for 30 seconds (except by second authority broadcast marking it resolved)
- [ ] Authority broadcasts forwarded by all relay nodes (epidemic routing, highest priority)
- [ ] Authority public keys updateable via a signed key update bundle from NDMA master key
- [ ] Broadcast history accessible in app (last 10 broadcasts with timestamps)
- [ ] Broadcast text readable by TalkBack / VoiceOver

### Dependencies

- Key provisioning: NDMA/NDRF authority keys embedded at build time
- Bundle signing and verification (cryptographic layer)
- PRD-006 (relay operation) for propagation

---

## PRD-006: Relay Operation in Background

**Priority:** P0  
**Platform constraint:** Android background execution limits; iOS background execution limits

### User Story

As a disaster survivor who is not actively using my phone, I need my phone to continue relaying IRIS bundles for others in the mesh so that the network continues to function even when I am focused on survival tasks.

### Description

IRIS relay operation must continue when the app is in the background (screen off, user not interacting). This is the primary technical challenge for mobile mesh networking.

**Android:** IRIS runs as a foreground service (persistent notification) with wake locks for BLE scanning. Wi-Fi Direct requires the GO to maintain the group; IRIS attempts to maintain Wi-Fi Direct group in foreground service. LoRa relay (via USB OTG) runs in the same foreground service.

**iOS:** iOS background execution is heavily restricted. IRIS uses:
- Core Bluetooth background mode for BLE relay
- Background App Refresh for periodic check-in (unreliable; OS may delay)
- Significant Location Change for wake on location change (if location access granted)
- Push notification wake (requires internet; not useful in offline scenarios)

iOS relay is best-effort; IRIS iOS app will relay while screen is on and during Core Bluetooth background events (triggered by nearby BLE advertisements).

### Acceptance Criteria

- [ ] Android: IRIS relay continues for ≥8 hours with foreground service active and screen off
- [ ] Android: IRIS foreground service survives MIUI/ColorOS/OneUI aggressive doze for ≥4 hours without user whitelisting IRIS
- [ ] Android: foreground service notification is clear about purpose ("IRIS: relaying emergency messages")
- [ ] Android: battery drain in relay mode ≤ 8% per hour on a 4000 mAh device
- [ ] iOS: IRIS relays BLE bundles in response to Core Bluetooth background events
- [ ] iOS: user is shown clear explanation of why Location Always permission improves relay reliability
- [ ] All platforms: relay operation does not transmit user's own messages without user action

### Dependencies

- Android foreground service implementation
- iOS Core Bluetooth background mode
- BLE transport operational

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial draft — 6 requirements |
