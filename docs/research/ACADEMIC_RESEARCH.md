# Academic Research Reference

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research  

---

## Purpose

This document catalogs academic literature that directly informs IRIS design decisions, protocol choices, and research gaps. For each paper, it states: what IRIS takes from the work, and what IRIS does differently or extends.

This is not an exhaustive literature review. It covers only papers where the influence on IRIS is concrete and traceable to a specific design element.

---

## 1. DTN Routing

### 1.1 PRoPHET — Probabilistic Routing

**Lindgren, A., Doria, A., & Schelén, O. (2004).** "Probabilistic Routing in Intermittently Connected Networks." ACM SIGMOBILE Mobile Computing and Communications Review, 7(3), 19–20. Also presented at MobiHoc WDTN Workshop 2004.

**What it proposes:** A routing algorithm for DTN that uses history of past encounters to estimate delivery probability. Contact history is summarized as a scalar P(a,b) ∈ [0,1] for each destination b known to node a. Three update rules: (1) direct encounter increases P, (2) aging decreases P over time, (3) transitivity propagates probability through intermediaries.

**What IRIS takes from this work:**
- The P(a,b) data structure and the three update rules (encounter, aging, transitivity)
- The forwarding decision rule: forward to node with higher P toward destination
- The parameter definitions (P_init, γ, β) and their semantics
- The evaluation methodology: delivery ratio, overhead ratio, latency CDF

**What IRIS does differently:**
- IRIS stores contact history in SQLite (WAL mode) rather than in-memory, enabling persistence across node restarts
- IRIS caps contact history at 30 days (configurable) to bound storage
- IRIS combines PRoPHET with Spray-and-Wait for cold-start nodes (no contact history)
- IRIS computes P over LoRa, BLE, and Wi-Fi contacts separately; combines via weighted average

---

### 1.2 Spray-and-Wait

**Spyropoulos, T., Psounis, K., & Raghavendra, C. S. (2005).** "Spray and Wait: An Efficient Routing Scheme for Intermittently Connected Mobile Networks." Proceedings of the 2005 ACM SIGCOMM Workshop on Delay-Tolerant Networking (WDTN), 252–259.

**What it proposes:** A two-phase routing scheme that limits replication (addressing epidemic's overhead problem) while maintaining good delivery probability. Phase 1 (Spray): distribute L copies to L distinct relays. Phase 2 (Wait): each relay waits for direct contact with destination.

**Spyropoulos, T., Psounis, K., & Raghavendra, C. S. (2006).** "Efficient Routing in Intermittently Connected Mobile Networks: The Multiple-Copy Case." IEEE/ACM Transactions on Networking, 16(1), 77–90.

The 2006 paper introduces Binary Spray-and-Wait: rather than spraying all L copies at once, each forward transfers L/2 copies, recursively halving. This achieves the same delivery ratio with fewer transmissions.

**What IRIS takes from this work:**
- Binary Spray-and-Wait as the cold-start routing algorithm (when PRoPHET history is insufficient)
- The L parameter concept and its relationship to delivery ratio and overhead
- The analytical model for optimal L under random waypoint mobility (used as a starting point before India-specific calibration — see RESEARCH_GAPS.md GAP-001)

**What IRIS does differently:**
- IRIS uses Spray-and-Wait only when PRoPHET history is below a threshold (configurable minimum sample count)
- L is configurable per deployment rather than fixed
- IRIS transitions from Spray-and-Wait to PRoPHET as contact history accumulates

---

### 1.3 MaxProp

**Burgess, J., Gallagher, B., Jensen, D., & Levine, B. N. (2006).** "MaxProp: Routing for Vehicle-Based Disruption-Tolerant Networks." Proceedings of IEEE INFOCOM 2006.

**What it proposes:** A routing algorithm that combines delivery probability (like PRoPHET) with bundle-level scheduling. Nodes maintain a ranked list of bundles to forward based on a utility function that incorporates: hop count, delivery probability, and time-to-live.

**What IRIS takes from this work:**
- The concept of per-bundle scheduling priority (IRIS implements bundle priority tiers: P0 SOS, P1 authority broadcast, P2 location, P3 text)
- The insight that delivery probability + age + hop count should influence forwarding decisions
- Evaluation metrics and comparison methodology

**What IRIS does not adopt:**
- MaxProp's full utility function is computationally expensive for constrained devices
- MaxProp requires per-bundle global state that is difficult to maintain in partitioned networks
- Not used in IRIS v1; documented for future consideration

---

### 1.4 Epidemic Routing

**Vahdat, A., & Becker, D. (2000).** "Epidemic Routing for Partially Connected Ad Hoc Networks." Duke University Technical Report CS-2000-06.

**What it proposes:** Flooding-based DTN routing where every node replicates every bundle to every encountered node. Maximizes delivery probability at unbounded cost.

**What IRIS takes from this work:**
- Used only for P0 SOS bundles where delivery probability must be maximized regardless of overhead
- The summary vector (Bloom filter of known bundle IDs) for efficient anti-entropy synchronization
- The proof that epidemic achieves maximum possible delivery ratio (theoretical upper bound for IRIS comparison)

---

## 2. LoRa Physical Layer and Scalability

### 2.1 LoRa Scalability

**Bor, M., Roedig, U., Voigt, T., & Alonso, J. M. (2016).** "Do LoRa Low-Power Wide-Area Networks Scale?" Proceedings of the 19th ACM International Conference on Modeling, Analysis and Simulation of Wireless and Mobile Systems (MSWiM), 59–67.

**What it proposes:** First systematic analysis of LoRa scalability. Models collision probability as a function of node count, SF distribution, and duty cycle. Key finding: with 1% duty cycle, a single LoRa gateway can support approximately 500 devices with SF7 but fewer with higher SFs due to longer airtime.

**What IRIS takes from this work:**
- Channel occupancy model used in IRIS's LoRa MAC scheduler: estimate airtime per bundle; limit transmission rate to stay within WPC duty cycle constraints
- SF selection algorithm: prefer lower SF when SNR allows (shorter airtime, lower collision probability)
- Gateway capacity estimates for hardware kit sizing

---

### 2.2 LoRa Interference

**Croce, D., Gaillard, M., Gifford, S., Ferré, G., & Bui, T. (2017).** "Impact of LoRa Imperfect Orthogonality: Analysis of LoRa Signals Collision." IEEE Communications Letters, 21(4), 796–799.

**What it proposes:** Demonstrates that LoRa spreading factors are not perfectly orthogonal. Inter-SF interference is higher than Semtech's datasheet implies. Under certain SNR conditions, an SF7 transmission can interfere with an SF8 receiver.

**What IRIS takes from this work:**
- IRIS uses a single SF per deployment zone (rather than mixing SFs) to avoid inter-SF interference
- Link budget calculations include an interference margin not present in Semtech's standard link budget calculator
- Motivates the field measurement planned in EXP-LORA-001

---

## 3. Disaster Communication

### 3.1 Disaster Communications Survey

**Haddow, G., & Haddow, K. S. (2014).** "Disaster Communications in a Changing Media World." Butterworth-Heinemann.

**What it provides:** Comprehensive survey of disaster communication from an emergency management perspective. Key findings:
- First 72 hours are critical; communication infrastructure failure is common in that window
- People prioritize family contact over receiving official information
- Warning message comprehension requires repetition and multiple channels
- Rumor propagation in information vacuums can be as dangerous as the disaster itself

**What IRIS takes from this work:**
- Priority scheme rationale: family location sharing (P2) is as important as authority broadcast (P1)
- Authority broadcast design: IRIS authority messages include hash-verified source identity to combat misinformation
- Multi-channel principle: IRIS operates across BLE, Wi-Fi Direct, and LoRa simultaneously

---

### 3.2 Social Media in Disasters

**Sutton, J., Palen, L., & Shklovski, I. (2008).** "Backchannels on the Front Lines: Emergent Uses of Social Media in the 2007 Southern California Wildfires." Proceedings of the 5th International ISCRAM Conference.

**What it provides:** Field study of Twitter use during the Peppermill wildfire. Key finding: people sought confirmatory information from multiple sources before acting on warnings.

**What IRIS takes from this work:**
- Authority broadcast design: IRIS displays message source and verification status prominently, not just the message text
- UX principle: do not present uncertain information (unverified relay count) as certain

---

## 4. Mesh Networking and Peer-to-Peer Systems

### 4.1 Peer-to-Peer Systems Overview

**Wehrle, K., Götz, M., & Rieche, S. (2005).** "Distributed Hash Tables." In Peer-to-Peer Systems and Applications, 79–93. Springer.

**What IRIS takes from this work:**
- DHT concepts inform IRIS's future distributed routing table design (not implemented in v1)
- Chord and Kademlia ring structures are reference points for evaluating IRIS routing table scalability

---

### 4.2 Ad Hoc Network Routing

**Perkins, C. E., & Royer, E. M. (1999).** "Ad Hoc On-Demand Distance Vector Routing." Proceedings of the 2nd IEEE Workshop on Mobile Computing Systems and Applications (WMCSA), 90–100.

**What IRIS takes from this work:**
- AODV's route request/reply model informs IRIS's topology discovery (not full AODV implementation)
- Route error propagation concept applied to IRIS contact blacklisting

---

## 5. Security and Cryptography

### 5.1 Signal Protocol

**Marlinspike, M., & Perrin, T. (2016).** "The Double Ratchet Algorithm." Signal Foundation Technical Report.

**What it proposes:** A key agreement protocol providing forward secrecy and break-in recovery using a combination of Diffie-Hellman ratchet and symmetric-key ratchet.

**What IRIS takes from this work:**
- Forward secrecy design principle motivates IRIS's session key rotation (not full double ratchet in v1)
- The argument for ephemeral key exchange per session informed ADR-0002
- IRIS v2 roadmap includes evaluating double ratchet for 1:1 messaging

---

### 5.2 Noise Protocol Framework

**Perrin, T. (2018).** "The Noise Protocol Framework." https://noiseprotocol.org/noise.html.

**What IRIS takes from this work:**
- Noise_XX and Noise_IK handshake patterns as reference for IRIS peer authentication protocol
- The concept of "handshake pattern" (static/ephemeral key combinations) informs IRIS transport session establishment

---

## 6. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
