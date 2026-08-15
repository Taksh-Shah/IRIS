# Record ID Allocation

**Last updated**: 2026-08-15T21:35:00Z

## Next Available IDs

| Type | Prefix | Next ID | Last Allocated |
|------|--------|---------|---------------|
| Decision | DEC- | DEC-0011 | DEC-0010 |
| Research | RES- | RES-0019 | RES-0018 |
| Redteam | RED- | RED-0003 | RED-0002 |
| Experiment | EXP- | EXP-0001 | — |
| Failure | FAIL- | FAIL-0006 | FAIL-0005 |
| Discovery | DISC- | DISC-0014 | DISC-0013 |
| Change | CHG- | CHG-0001 | — |
| Verification | VER- | VER-0001 | — |
| Checkpoint | CHK- | CHK-0005 | CHK-0004 |
| Orchestration | ORCH- | ORCH-0002 | ORCH-0001 |

## Allocated IDs

### Redteam
- RED-0001: Transport Layer Adversarial Review (internet.rs, manager.rs, ble.rs, simulated.rs, mod.rs, message.rs, error.rs)
- RED-0002: ML-001 L3 Shadow-ML Adversarial Review (sim/ml/*, sim/mod.rs shadow path, prophet.rs DP source) — PASS; shadow isolation holds; ML-RT-02/03 fixed in-pass, ML-RT-01/04/05/06/07 → known_limitations
- RES-0001: BPv7 Bundle Structure vs Flat Envelope
- RES-0002: DTN Time and Bundle Identification
- RES-0003: LoRa Payload Constraints for IRIS P0
- RES-0004: STORE-001 Storage Design Validation
- RES-0005: MSG-001 Message Engine Design Validation
- RES-0006: MSG-001 Acceptance/Verification
- RES-0007: BLE-001 Abstractions + Platform Constraints
- RES-0008: MSG-001 Message Engine Best-in-Class (BPv7 RFC 9171 lifecycle, custody, dedup, eviction; C1 → 16-byte UUIDv7)
- RES-0009: BLK-0001 Crypto Architecture (per-message ephemeral X25519 v1, HKDF-SHA256 RFC 5869, scoped forward secrecy)
- RES-0010: MSG-001 Message Engine Implementation Research 2026 (RFC 9713/9758/9171 primary verification, CTAB/CREB + UQEB, Audsley/DRR, double-hash Bloom, RFC 6298 retry bounds, Bundle Age + Android 18h NTP; 11 MSG_DESIGN.md changes)
- RES-0011: ROUTE-002 L2 Opportunistic Routing — RFC 6693 PRoPHET v2 equations (§2.1.2: delta-bounded direct, gamma aging, MAX transitivity — NOT v1 additive), §3.3 defaults (P_enc=0.7/0.5, P_first_thr=0.1, beta=0.9, gamma=0.999, delta=0.01), §5.3.3 timers, §3.6 GRTR/GTMX+/GTHR strategies, no errata; binary SaW optimality + L≈10-15% of M (WDTN'05, ToN'08); MaxProp/PRoPHETv2/ONE methodology; RFC 9171 does not obsolete 6693
- RES-0012: OBS-001 Observability — tracing events/spans direct (no wrapper, interest-cache near-zero disabled cost); OTel mental model but NO OTel SDK/OTLP v1 (messaging semconv still Development; metrics API stable); RFC 9171 §6.1.1 Bundle Status Report reason codes 0-7 map onto DropReason (1=lifetime, 4=storage, 6=no route, 7=no timely contact), §6.2 reports MUST be off by default; 24-event taxonomy (msg.*/route.*/scf.*/gw.*/topo.*) + 21 SimMetrics-vocab metrics; privacy: payload exclusion, 8-byte ID truncation, HMAC-salted hashing, allow-listed attrs, off-by-default export (DPDPA §4(1)(b)/§6(1)); ring-buffer + batch-on-contact (Meshtastic airtime budget, tracing-cache, ION bpstats/bptrace)
- RES-0013: DESKTOP-001 Tauri v2 desktop shell — tauri crate 2.11.5 (2026-07-01), CLI 2.11.4, api 2.11.1; IPC: commands (async = async_runtime tokio spawn), events (JSON, not high-throughput), Channels (ordered/fast — chat stream), tauri::ipc::Response for raw bytes; tauri.conf.json v2 identifier required, devUrl optional (CLI built-in dev server + simple hot-reload over frontendDist), withGlobalTauri for no-npm vanilla frontend; managed State (no Arc needed), async cmd borrow limitation issue #2533 (owned types or Result wrapper), return Result required; Linux webkit2gtk-4.1 deps; Windows WebView2 preinstalled Win10 1803+; testing tauri::test MockRuntime mock_builder/get_ipc_response/assert_ipc_response (feature "test"); background = RunEvent::ExitRequested prevent_exit (default exits on last window close on Win/Linux; macOS stays alive) + tray-icon; alternatives: Electron rejected (size/security), egui/iced/slint native (no web UI, smaller ecosystem), wry direct (rebuild framework)
- RES-0014: ML-001 L3 delivery-probability & gateway prediction SOTA — RFC 6693 PRoPHET v2 remains the IETF standard (Level 1, already in ROUTE-002); ML framing: relay selection as classification (Dudukovich & Papachristou 2018, XGBoost best, PRoPHET > Epidemic); ML-MaxProp (arXiv:2508.20077) XGBoost over {contact freq, buffer occ, hop, age, TTL} beats MaxProp/SaW in ONE; RL-PRoPHET (Wireless Net 31:2909, 2025) learns the DP itself; DTRB MARL (EAAI 2013); gateway/egress = OppNet gateway taxonomy (DTN scheduled vs OppNet random), PodNet/mule offloading, ferry prediction gains only under periodic contacts; learned routing: GNN UAV-DTN (ICCBN'24), graph-attention MARL decentralized lunar DTN (arXiv:2510.20436), DRL uncertain contact plans (2025), RouteNet-TGNN central-controller (JCM 2026); constraints: training offline/central, inference cheap on-node, SIM-001 can test DP/gateway/forwarding prediction same-seed vs L2, never in critical path (P3 #5/#6); experimental design: A/B same-seed vs L2, periodic-vs-random spectrum, held-out leakage guard, shadow-metrics only; gaps: synthetic traces, central-vs-decentral transferability, Level 1 vs Level 2-3 maturity asymmetry
- RES-0015: CRYPTO-001 cryptographic architecture (2026 SOTA + DEC-P0001/P0002 + at-rest + FS). RUSTSEC audit 2026-08-14: no advisory vs x25519-dalek/chacha20poly1305(RustCrypto)/hkdf; curve25519-dalek RUSTSEC-2024-0344 (timing, ≥4.1.3); ed25519-dalek RUSTSEC-2022-0093 (≤1.16, 2.x clean) + MUST use verify_strict (weak keys); libcrux advisory family is Cryspen bindings (NOT our crates) but teaches RFC 7748 §6.1 all-zero shared-secret reject + input validation; ring RUSTSEC-2025-0009/0010 (≥0.17.9, 0.17 maintained). D1 (DEC-P0001): per-message ephemeral X25519 ECIES-like per MESSAGE_ENVELOPE.md — ADR-0006 "per session" corrected; sender-side FS, no recipient-FS (documented). D2 (DEC-P0002): RFC 5869 HKDF-SHA256 via RustCrypto hkdf+sha2; HKDF-BLAKE3 rejected non-standard; BLAKE3 = non-crypto hashing only. D3: at-rest row encryption in iris-storage, storage key HKDF(master,"iris-at-rest-v1"), ChaCha20-Poly1305 per-row random nonce, index columns plaintext/metadata sealed, master key Android Keystore (TEE)/OS keychain. D4: FS honesty — no recipient-FS claims in v1; recipient-FS v2 (X3DH-style prekeys/Double Ratchet, needs BLK-0002). D5 crates: x25519-dalek 2.0.1, ed25519-dalek 2.x (verify_strict only), chacha20poly1305 0.10.1, hkdf 0.12, ring 0.17.9+ alternative. KAT line-up RFC 7748 §5.2/6.1, 8439 §2.8.2, 8032 §7.1, 5869 App A
- RES-0017: EMERG-001 emergency system research (QW1-8). Q1 alert formats: OASIS CAP v1.2 (semantics only), ITU-T X.1303bis (ASN.1 compact binary), ETSI TS 102 900 EU-ALERT V1.4.1 (§5.5 NO UE auth), 3GPP TS 23.041 CB, ATIS-0700041.v002 WEA 3.0 device geo-fencing, FEMA IPAWS, NRTA GY/T 383-2023/385-2023/426-2026 (广电发〔2023〕54号, no D2D mesh standard). Q2 auth: RFC 9804 SPKI keyholder chains (RED-0008 fit), no-UE-auth → WEA spoofing research (CACM 2023 Ed25519+SIM ~68B, ACM 2024 BBV 3.220ms, arXiv 2604.24404), CVE-2025-52464 (Meshtastic keypairs CVSSv4 9.5), Meshtastic sig 64B vs 256B packet. Q3 priority: 802.11e/WMM EDCA 4 ACs; COSPAS-SARSAT 406 burst 45-50s first delayed 50s; Garmin inReach 1-min→10-min; no 15-min SOS standard (implemented ack.rs P0 30s/2x/unlimited-until-TTL). Q4 disaster mode: flood-to-hop-limit (Meshtastic mesh-algo + #4764 duty-cycle-aware CLIENT role), MeshCore dedup+hop, Epistle (queue/rate-limit/battery ~14%/2h), FirstNet vendor; drills: FEMA HSEEP + PrepToolkit comms drills (monthly/quarterly/annual), ISCRAM 2024 community drill → TEST_MODE requirement. Q5 status: RFC 9171 §6.1/6.2 (MUST off by default), RFC 9713 admin record types, RFC 9758, arXiv 2507.17403 (ESA CBOR compressed status). Q6 a11y: WCAG 2.2 SC 1.4.6 7:1 + SC 2.2.4 emergency exception (30s takeover OK); WEA tone 853+960 Hz ≠ docs' 880 Hz (C3); Android FGS/OS channels; iOS in-app only. Q7 legal: BNS 2023 replaced IPC 2024-07-01 (§420→§318, §505→§353(2); §153A unverified) → EMERGENCY_ABUSE stale; SybilGuard/SybilLimit/Newsome 2004/Wikipedia — per-identity limits insufficient. Q8 integration: ContentType::Sos=2/EmergencyAlert=14 + EMERGENCY_BROADCAST recipient; P0 ceiling 84B; EmergencyAlert no-fragment vs ≤2000-char instructions conflict (C1); EMERG-001 node lacks acceptance_criteria (C2); conflicts C1-C6; D6 RUSTSEC: NO new crypto crates. DEC-EMERG-0001..0008 recommendations. Gaps: SEP-0182/CAP-CBOR unresolvable, ECREP/SOTS/RCERCA/TFTmesh unverifiable, BNS §153A, tone governing text, no external validity benchmark (2y/1y/6mo/3mo)
- RES-0016: IDENT-001 identity/key-management/trust/address-derivation. D1 (DEC-P0005): per-device Ed25519 identity; sender_id = first 16 B of SHA-256(pubkey), full hash = PeerId; UID separate non-authoritative; NEVER MAC/hardware-derived (CVE-2025-53627 Meshtastic NodeNum spoofing). D2 (DEC-P0006): per-platform provisioning — Android Keystore TEE/StrongBox (StrongBox only for master/identity: keygen ≈9.2 s, 1 MiB ≈3 s — KeyDroid 2507.07927), iOS Keychain (SE is P-256-only), desktop OS-keychain/file (RED-0001 wiring), Pi TPM sealing (no measured boot); wrapped-DEK pattern; StorageKeySealer default-on; AOSP keystore itself uses HKDF-SHA256 super-keys = corroborates DEC-P0004; Stronghold software vault "not audited" ≠ root of trust. D3 (DEC-P0007): trust = TOFU baseline + signed key advertisement (RED-0005) + Briar BQP-style QR/pairing for verified tier, UI verified/unverified; RED-0008 = SPKI/RFC 9804 keyholder chains, NOT PKIX/X.509. D4 (DEC-P0008): rotation/revocation = signed rotation events, monotonic key-gen counter + valid-until + earliest-seen-wins; KERI null-rotation semantics; full event log = v2. D5 (DEC-P0009): two distinct keypairs, Ed25519 signs X25519 static advertisement (Signal X3DH/Meshtastic 2.8.x XEdDSA precedent; no curve conversion in v1 — IACR 2021/509, LSEG 2511.07548); RED-0011 rides along. D6 crates: no new crypto crates; keyring/android-keystore DESIGN candidates (no advisories); AVOID PKIX family (rustls-webpki RUSTSEC-2026-0104/0098/0099, pemfile 2025-0134); watch core2 2026-0105/thin-vec 2026-0103. Gaps: "QEMU/PLD" operator clarification; Meshtastic 2.8.x merge status; Stronghold post-2.1.0 audit unconfirmed; no formal proof of signed-key-binding in disconnected mesh
- RES-0018: SEC-001 security-hardening research (2024-2026 SOTA, 12 evidence passes, RFC/standard + academic primary; no AI citations): Q1 DoS — RFC 9171 §6.9 CL MUST rate-limit (L1); ADOPT srTCM token bucket per (sender,class) silent-drop P0/P1-exempt (RFC 2697/2698/4115) + per-sender storage quota + priority pool + TTL eviction; Meshtastic CVE regression suite (CVE-2024-47065 traceroute amplification, 2025-21608/24798 want_response crash, 2025-24797 parser, 2025-55293 amplification, 2025-52464/2026-42566 key hygiene); REJECT per-message PoW (Hristozov AISEC battery, Springer MTU, duty-cycle), DEFER identity-mint PoW (KeyChallenge/SyDeLP)+VDF+RFC 7859 to PROTO-001 v2 (no-new-crypto held). Q2 replay — dedup = content-dedup not anti-replay; cross-reboot hole (WP-2 deferred Bloom persistence, mod.rs:24) → layered = dedup + freshness window τ + per-sender high-water (ts,seq) (RFC 9171 §4.2.7, RFC 4303 §3.4.3 concept, RFC 7181 §23.2/RFC 7183 DTN-time-substituted, RFC 6479) + persistence promoted WP-2→SEC-001; collision bounds NIST SP 800-107; fragments re-enter dedup. Q3 Sybil — per-identity limits necessary-not-sufficient (Douceur 2002); REJECT SybilGuard/SybilLimit on relays (connected-graph); ADOPT Ostra NSDI 2008 link-credit + Briar BQP verified-pairing; key CVEs → IDENT regression. Q4 spam — ADOPT reputation = local watchdog + positive-only verified second-hand (CORE) + iTrust audits as routing weight only (Watchdog/Pathrater, iTrust TPDS 2014, ITRM, Rep-AODV 2024); receiver-side Bayesian scoring only; ADOPT RED-0002 ACL-1 key-anchored per-class send allowlist over emergency/authority.rs (CAP v1.2 DSig, TS 102 900 §5.5, CACM 2023 WEA Ed25519). Q5 — blackhole = replication + Ack-eviction (Epidemic Oracle ISCC 2025) + reputation weight; REJECT RTT/GPS secure-position; DEFER SAND to v2. Q6 — CONFLICTS C1-C6 (DISC-0013 doc-vs-code drift) feed DESIGN C-pattern reconciliation; gaps G1-G7 (BBPATs→G1, no DTN flooding standard G2, no GPS-free SPS G3, τ→EXP-SEC-001 G6); DEC-SEC-0001..N formalization at DESIGN

### Decisions
- DEC-0001: CBOR over Protocol Buffers (ADR-0001)
- DEC-0002: Ed25519 for Identity (ADR-0002)
- DEC-0003: Rust for Core (ADR-0003)
- DEC-0004: SQLite for Store (ADR-0004)
- DEC-0005: PRoPHET Routing (ADR-0005)
- DEC-0006: ChaCha20-Poly1305 (ADR-0006)
- DEC-0007: Control Plane Architecture
- DEC-0009: Operator authorization — unblock all gates & human gates (BLK-0001
  CRYPTO-001/IDENT-001, BLK-0005 transports, LEGAL-001, EMERG-001/SEC-001/
  TEST-001/ANDROID-001/IOS-001/PILOT-001); standing, effective 2026-08-14;
  quality process (research/redteam/verifier) retained
- DEC-0010: Operator ratification — RED-0001 CRITICAL tracked to IDENT-001;
  security control 'no production deployment with DevCryptoProvider/no identity/no sealer';
  CRYPTO-001 proceeds to VERIFY -> ACCEPT -> COMPLETE (2026-08-15)
- DEC-P0001: Per-session vs Per-message Key (PENDING — to resolve in CRYPTO-001 RESEARCH)
- DEC-P0002: HKDF Construction Standard (PENDING — to resolve in CRYPTO-001 RESEARCH)

### Failures
- FAIL-0005: CAP-CBOR / "SEP-0182" adoption — no standardized CAP-in-CBOR exists (no RFC/IETF-draft/industry spec found 2026-08-15); CAP is XML, X.1303bis is ASN.1/PER; unresolvable reference recorded as gap, NOT fabricated → IRIS designs minimal CAP-inspired CBOR subset (DEC-EMERG-0001)
- FAIL-0001: P0 SOS ignores LoRa duty cycle
- FAIL-0002: Claiming IRIS engaged legal counsel
- FAIL-0003: Ed25519 uses BLAKE3 internally
- FAIL-0004: HKDF-BLAKE3 key derivation — non-standard (HMAC-BLAKE3 not IETF); rejected DEC-P0002 2026-08-14 → RFC 5869 HKDF-SHA256; BLAKE3 native derive_key if ever needed; never homemade HMAC HKDF

### Discoveries
- DISC-0011: Cell-Broadcast alert systems (EU-Alert/WEA) provide NO UE authentication — ETSI TS 102 900 §5.5; WEA spoofing research (CACM 2023, ACM 2024, arXiv 2604.24404) → IRIS's offline preloaded-root + SPKI Ed25519 authority chain is the verified mitigation pattern (DEC-EMERG-0002)
- DISC-0012: BNS 2023 (Act 45 of 2023) replaced IPC effective 2024-07-01 (IPC §420→BNS §318; IPC §505→BNS §353(2); §153A mapping unverified) → EMERGENCY_ABUSE.md IPC references are stale; migrate to BNS before EMERG-001 VERIFY (DEC-EMERG-0007, LEGAL-001 hook)
- DISC-0001: Ed25519 SHA-512 internal
- DISC-0002: HKDF-BLAKE3 non-standard
- DISC-0003: No forward secrecy against recipient compromise
- DISC-0004: LoRa duty cycle applies to all messages
- DISC-0005: India LoRa spectrum 865–868 MHz
- DISC-0006: iOS cannot relay Wi-Fi Aware/Direct
- DISC-0007: P0 SOS fits in 255 bytes
- DISC-0008: Measured P0 envelope = 237 B; payload capacity ceiling 84 B (supersedes DISC-0007/MESSAGE_ENVELOPE.md estimates)
- DISC-0009: PROTOCOL_TEST_VECTORS.md timestamp hex arithmetically wrong (0x66B5C000 → 0x66B7FF00)

### Checkpoints
- CHK-0001: Control plane construction complete
- CHK-0002: PROTO-001 research and architecture complete
- CHK-0003: PROTO-001 completion ready
- CHK-0004: STORE-001 accepted; control plane v1 verified

### Orchestration
- ORCH-0001: Phase Implementation Orchestration Plan (CORE_PROTOCOL_IMPLEMENTATION)
