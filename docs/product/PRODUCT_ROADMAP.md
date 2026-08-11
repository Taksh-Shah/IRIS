# Product Roadmap

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Product  

---

## Roadmap Overview

| Milestone | Target | Key Deliverable |
|-----------|--------|----------------|
| MVP | Month 6 | SOS + location + 1:1 text on Android (BLE + Wi-Fi Direct) |
| Alpha | Month 12 | Group messaging, LoRa gateway, iOS app, edge server |
| Beta | Month 18 | Satellite fallback, authority broadcast, ML routing experiments |
| Pilot | Month 24 | Ahmedabad field trial with NDRF partnership |
| GA | Month 30 | App Store + Play Store, enterprise API, STQC certification |

All dates are from project start (Month 0 = Q1 2026).

---

## Milestone 1: MVP (Month 6)

### Goal

A working disaster communication tool for Android that a first responder or disaster survivor can use without training, without internet, and without an account.

### Deliverables

**Android app (public beta on Play Store internal test track):**
- SOS beacon (PRD-001): 1-tap from lock screen (3-press power), location + text note
- Location sharing (PRD-002): GPS with cell-ID fallback, toggle on/off
- 1:1 text messaging (PRD-003): end-to-end encrypted, TTL-based
- Basic offline map (PRD-004): district-level OpenStreetMap tiles for 10 high-risk Indian districts
- Relay mode (PRD-006): Android foreground service, BLE + Wi-Fi Direct

**Rust core library (`iris-core`):**
- DTN bundle format (CBOR-based, IRIS compact format)
- PRoPHET routing with Spray-and-Wait fallback
- Ed25519 identity, ChaCha20-Poly1305 message encryption
- SQLite bundle store (WAL mode)
- BLE transport (discovery + data transfer)
- Wi-Fi Direct transport

**Test infrastructure:**
- ONE simulator scenarios for routing validation
- Android integration test suite (Espresso)
- 10-node physical test lab (Android devices in office)

### Key Dependencies

- Android BLE 5.x support (target API 31+)
- Wi-Fi Direct API (Android 4.0+; targeting Android 10+)
- SQLite via rusqlite (Rust crate)
- OpenStreetMap tile data pipeline

### Exit Criteria

- SOS delivered within 30 seconds in 3-hop BLE mesh (lab test)
- Zero critical security vulnerabilities in internal security review
- Battery drain ≤ 8%/hour in relay mode (tested on Redmi 12)
- App installable by non-technical user in ≤ 5 minutes
- Internal team dogfood: 20-person test at IRIS office

---

## Milestone 2: Alpha (Month 12)

### Goal

Extend reach (LoRa), platform (iOS), and capacity (group messaging, edge server) to enable a realistic disaster simulation with 50+ participants.

### Deliverables

**iOS app (TestFlight alpha):**
- SOS, location, 1:1 text (feature parity with Android MVP)
- Core Bluetooth background relay mode
- iOS-specific UX (system SOS integration, Widget)

**LoRa gateway (RPi-based):**
- LoRa transport driver (SX1262 via SPI to RPi)
- IRIS gateway software (`iris-gateway` binary, Linux/RPi)
- WPC-compliant India band (865 MHz) configuration
- Bridge: LoRa ↔ BLE/Wi-Fi for reach extension

**Group messaging:**
- Group creation, member management
- Group keypair distribution (admin-signed member list)
- Group message encryption (sender-to-group-key)

**Edge server (`iris-edge`):**
- Bundle caching for disconnected nodes
- REST API for gateway and app connection
- TLS 1.3, authenticated connections
- Deploy on: IRIS-operated cloud (India region, AWS Mumbai or Azure India)

**Authority broadcast (basic):**
- NDRF and NDMA authority keys embedded in app
- Signed broadcast reception and display (P1 priority)

### Key Dependencies

- LoRa hardware: SX1262 module on RPi (REYAX RYLR890 or equivalent)
- iOS: Core Bluetooth background mode entitlement (Apple Developer Program)
- Edge server infrastructure: VPS in India (₹5,000/month)

### Exit Criteria

- 50-node simulation in ONE simulator: PRoPHET delivery ratio ≥ 80% at 24h
- LoRa gateway tested at 2km range (open field, Mumbai suburbs)
- iOS app: SOS delivered via BLE in background within 60 seconds
- Edge server: 99% uptime over 30-day test period
- Alpha test: 20 external testers (NDRF volunteers, disaster management students)

---

## Milestone 3: Beta (Month 18)

### Goal

Add satellite fallback and authority broadcast to make IRIS viable for scenarios where terrestrial mesh density is insufficient. Begin ML routing experiments.

### Deliverables

**Satellite fallback:**
- Iridium SBD integration (9603 modem via UART to RPi gateway)
- P0/P1 bundle forwarding to Iridium SBD when terrestrial mesh fails
- Satellite status indicator in app (shows "Satellite relay active")
- Iridium India service agreement (via Tata Communications)

**Authority broadcast (full — PRD-005):**
- P0 full-screen takeover
- Key update mechanism (signed key rotation from NDMA master key)
- Geographic scope filtering (broadcast only shown if receiver in specified bounding box)

**ML routing experiments:**
- Python training pipeline: XGBoost model for delivery probability prediction
- A/B test infrastructure: 10% of bundles routed via ML model, 90% via PRoPHET
- Feature set: contact duration, time-of-day, transport type, node mobility estimate
- Evaluation: delivery ratio improvement vs PRoPHET baseline

**Desktop app (Tauri, Windows/macOS/Linux):**
- Feature parity with Android app (MVP feature set)
- Useful for: command post, NDRF operations center
- Wi-Fi Direct transport (via OS API); LoRa via USB OTG

### Exit Criteria

- Satellite relay: P0 SOS delivered via Iridium SBD in end-to-end test (India → Iridium → India)
- Authority broadcast: verified P0 broadcast triggers full-screen within 5 seconds of receipt
- ML routing: EXP-ROUTE-005 result showing ≥5% delivery ratio improvement over PRoPHET baseline
- Beta test: 100 external users, 4-week beta period

---

## Milestone 4: Pilot (Month 24)

### Goal

Real-world field trial with institutional partner (NDRF) in Ahmedabad, validating that IRIS functions under real disaster conditions with real users.

### Deliverables

**Ahmedabad field trial:**
- Partner: National Disaster Response Force (NDRF) — 12th Battalion (Gandhinagar)
- Scenario: simulated earthquake response, urban search and rescue
- Participants: 50 NDRF personnel + 20 community volunteers
- Deployment: 5 LoRa gateway nodes (RPi-based), all on Iridium SBD
- Duration: 3-day exercise

**Field trial infrastructure:**
- Pre-deployed gateway nodes at Ahmedabad Fire Station, Kalupur area, 3 other locations
- IRIS edge server in Mumbai cloud region
- Field trial build: audit log, researcher observation mode
- Post-trial: EXP-ROUTE-006 analysis of real contact traces

**NDRF partnership outcomes:**
- Signed MoU for continued engagement
- Evaluation report from NDRF
- NDMA endorsement letter (target)

### Exit Criteria

- Field trial conducted without technical failures preventing exercise completion
- Participant satisfaction: ≥80% rate IRIS "easy to use" on post-exercise survey
- SOS delivery: ≥90% of test SOS messages delivered within 5 minutes in exercise scenario
- NDRF commitment to continued engagement (signed next-phase agreement)

---

## Milestone 5: General Availability (Month 30)

### Goal

Public app store launch, enterprise API, STQC certification, and commercial availability of hardware kits.

### Deliverables

**App store launch:**
- Google Play Store: public release (rating ≥ 4.2)
- Apple App Store: public release
- F-Droid: open source build

**Enterprise API:**
- REST API for enterprise integration (admin console, device management, alert broadcasting)
- Enterprise dashboard (Tauri desktop app)
- SLA-backed support contracts

**Hardware kits:**
- IRIS Gateway Kit v1.0: RPi4 + SX1262 LoRa + Iridium 9603 modem + solar charge controller
- WPC type approval: obtained for kit components
- BIS registration: obtained
- Pricing: ₹45,000–₹60,000 per kit (enterprise); ₹15,000 (NGO pricing)

**STQC certification:**
- Software security assessment by STQC
- Common Criteria EAL2 evaluation (target for government procurement)

**Pricing:**
- App: free (open source)
- Enterprise support: ₹5,000/device/year
- Government contract: negotiated per deployment

---

## Key Dependencies and Risks

| Dependency | Risk | Mitigation |
|-----------|------|-----------|
| NDRF pilot partnership | Approval takes longer than planned | Begin engagement Month 6; MoU target Month 15 |
| Iridium India service agreement | Commercial terms unfavorable | Evaluate OneWeb as alternative Month 16 |
| iOS App Store approval | Apple rejects app for emergency alert category | Engage Apple Developer Relations Month 10 |
| STQC certification | Timeline extends beyond Month 30 | Begin process Month 20; GA date floats if needed |
| WPC type approval for hardware | Regulatory delay | Engage WPC/TEC Month 15 |

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial roadmap |
