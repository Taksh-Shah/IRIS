# Record ID Allocation

**Last updated**: 2026-09-08T15:44:00+05:30 (FAIL-0009 and RES-0034 allocated)

## Next Available IDs

| Type | Prefix | Next ID | Last Allocated |
|------|--------|---------|---------------|
| Decision | DEC- | DEC-0011 | DEC-0010 |
 | Research | RES- | RES-0037 | RES-0036 |
| Redteam | RED- | RED-0003 | RED-0002 |
| Experiment | EXP- | EXP-0001 | — |
| Failure | FAIL- | FAIL-0010 | FAIL-0009 |
| Discovery | DISC- | DISC-0014 | DISC-0013 |
| Change | CHG- | CHG-0001 | — |
| Verification | VER- | VER-0001 | — |
| Checkpoint | CHK- | CHK-0009 | CHK-0008 |
| Orchestration | ORCH- | ORCH-0002 | ORCH-0001 |

## Allocated IDs

### Current recovery allocation
- FAIL-0009: Android Internet transport reports unavailable and mesh stays down
- RES-0034: Android Internet/LAN relay recovery research
- RES-0035: INTERNET-ANDROID-001 first compile failure (Rust ownership/Option conversion)
- RES-0036: INTERNET-ANDROID-001 jitter assertion failure
- CHK-0008: INTERNET-ANDROID-001 research recovery checkpoint

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

- RES-0019: BLE-001 Android BLE transport 2024-2026 SOTA — 17 websearch queries, documented evidence levels; verdict RECOMMEND: PROCEED. R1 ADOPT: keep v1 = GATT point-to-point + Extended/Periodic-advertising discovery beacon (Android advertising APIs L1; RES-0007 reconfirmed; MTU: Android 14+ negotiates 517 on first requestMtu per ACL, subsequent requests disregarded → segment ATT payloads ≤512 B, treat MTU as negotiate-late via onMtuChanged); R2/R3 REJECT v1: BLE Mesh 1.1 (no native AOSP mesh API through 2026 — L1 absence; nRF Mesh/ Silabs/ STBLEMesh library = only app path; May/June 2026 Medium "native android.bluetooth.mesh" claim contradicted by official docs AND same author's field report — Gap G3 unresolved L5, do not depend) and PAwR (BLE 5.4 star topology built for Electronic Shelf Labels; phone can sync/receive PAwR train via PeriodicAdvertisingManager but NO Android API to transmit in response slots — Nordic DevZone + Infineon AN + Zephyr embedded-only samples; DEFER to v2 stationary gateway/AP); R4 REJECT v1: BIS/isochronous broadcast = one-way LE-Audio centric only (watchlist); R5 ADOPT as security constraint: app-layer envelope crypto = trust anchor — 2024-25 Android BLE stack CVEs unpatched-by-app (CVE-2024-43770 GATT gatts_process_find_info OOB RCE, CVE-2025-0074 SDP RCE, CVE-2025-48539 acl_arbiter UAF zero-click CVSS 8.0 exploited, CVE-2025-22406 BNEP UAF, CVE-2025-44557 pairing_failed auth bypass, CVE-2025-62235 NimBLE BLERP re-pairing); NCC link-layer relay ≤8 ms defeats GATT response-latency bounding + link-layer encryption → IRIS uses no proximity auth so residual risk = topology distortion only (bounded by SEC-001 replay/dedup + ROUTE-002 reputation); Channel Sounding (Bluetooth 6.0, 0.3-1 m, attack detector) = HARDWARE-gated future node NOT v1; R6 ADOPT as DESIGN ACs: FGS type connectedDevice (API 34+) + FOREGROUND_SERVICE_CONNECTED_DEVICE + POST_NOTIFICATIONS + BLUETOOTH_SCAN/CONNECT, background scan via PendingIntent+filters (Oreo+), batch reportDelayMillis>0 + flushPendingScanResults, ≥5 scan start/stop per 30 s → SCAN_FAILED_APPLICATION_REGISTRATION_FAILED (Android 17 stricter — G4 L5 planning input), runtime capability checks isLePeriodicAdvertisingSupported()/PHY, exponential-backoff scan-restart, single GATT connection per peer + prompt close() (L5 ~32-client ceiling, G2), defensive advert parsing; R7 for BLE-002: iOS background advertising drops local name + service UUIDs to overflow area (Android/iOS advert payloads NOT symmetric), iOS 26 Live Activity restores foreground scanning — iOS node cannot be dependable background advertiser → central-role + connection-first; R8 DLEP = RFC 8175 (prompt's RFC 6841 corrected; RFC 8703 Link Identifier + RFC 8757 Latency Range extensions) router↔modem control plane — NOT applicable to phone BLE (no router/modem split), DEFER as routing-metric interface for gateway hardware; R9 roadmap-only: Mesh 1.1 RPR/DFU/CBP/EPA, Bluetooth HDT 7.5 Mbps (adoption late 2026, new radios both ends), CS secure ranging — do not size v1 around any. Gaps G1-G7: G1 PAwR-phone responder formal confirmation needed at DESIGN (LITERATURE → HARDWARE_TEST); G2 GATT client ceiling L5; G3 mesh-on-Android contradiction unresolved; G4 Android 17 verify at IMPLEMENT vs official release notes; G5 Android 14 requestMtu "Request Not Supported" wedge → re-connect on error path; G6 CS 0.3-1 m accuracy L3-only; G7 HDT details draft-level. Maturity RESEARCHED; Confidence HIGH (Android FGS/permissions/MTU/scan, L1 multiple sources) / MEDIUM (PAwR-phone-impossibility, 2026 mesh-on-Android, corroborated L5) / LOW (Android 17 details, CS/HDT timelines). Sources: developer.android.com, Bluetooth SIG Core 5.4/6.0 resources, RFC 8175/8703/8757, NVD/SentinelOne/CVE-news, Nordic DevZone+SDS guides, Silicon Labs, Zephyr samples docs, Infineon AN, NCC Group advisory, Apple CoreBluetooth docs, novelbits/cloud2gnd/argenox/ProAndroidDev/Medium (L5 cross-checked).

- RES-0020: WIFIAWARE-001 Android Wi-Fi Aware (NAN) transport 2024-2026 SOTA — 8 websearch evidence passes, primary sources, no AI citations; verdict PROCEED. R1 ADOPT: v1 = publish/subscribe discovery + NDP IPv6 socket data path; NAN added API 26, alive (not deprecated) through API 36/Android 16 + forward to Android 17 (new architecture doc); runtime FEATURE_WIFI_AWARE + isAvailable() + ACTION_WIFI_AWARE_STATE_CHANGED (OEM firmware-gated). R2 ADOPT constraint: app-layer envelope = trust anchor; NDP open at L2 (SecurityConfig/PSK/Aware Pairing API 34+ = optional hardening, never identity boundary) — NCS cipher suites SK/PK/PASN API 30/33/34, WFA spec 4.0. R3 ADOPT: 6 GHz/Wi-Fi 6E hardware+region-gated (Gap G-WA-3), design on 2.4/5 GHz floor, runtime cap check (parallel 6E ~1.8 Gbps close-range evidence). R4 ADOPT: reuse INTERNET-001 1 MiB TCP framing on the NDP network interface (kernel MTU, Gap G-WA-1); retain matrix 20-100 Mbps practical / ~300 Mbps peak. R5 ADOPT: production discovery requires active process via FGS (connectedDevice API 34+) — "survives without FGS" (old doc) is dated under Android 12+/15/16 background limits; Suspend/Resume API 34+ (HAL-gated) as power lever. R6 ADOPT: ranging WifiRttManager.startRanging + addWifiAwarePeer(PeerHandle|Mac) + geofenced discovery (API 31+ setMin/setMaxDistanceMm + setRangingEnabled) = optional discovery metadata; 802.11mc FTM / 802.11az NTB (API 35+); NEARBY_WIFI_DEVICES neverForLocation OR ACCESS_FINE_LOCATION. R7 ADOPT: coexistence (single-radio) = runtime-availability constraint (Aware may be unavailable w/ Wi-Fi Direct/SoftAP/tethering; NAN channel selection firmware-driven; coex channel-avoidance Android 12+) → degrade to BLE/Wi-Fi Direct (BLE-001 pattern). R8 ADOPT: NAN MAC layer privacy-hardened (NMI/NDP randomized, ~30 min interval, factory MAC never; IdentityChangedListener) — no NAN app-layer CVE wave (Gap G-WA-2; kernel iwlwifi CVE-2024-49857 secured-NDP-ranging cipher = OS-patch-gated); envelope boundary + no unauth triggers (SEC-001/IDENT-001). R9 ADOPT (CONFLICT-1): Apple now ships WiFiAware framework (iOS 26+, iPhone 12+) — correct WIFI_AWARE.md + TRANSPORT_ABSTRACTION "iOS NO" at DESIGN; BLE-002 stays v1 iOS path; Android↔Apple NDP interop immature (L5). Gaps G-WA-1/2/3 recorded.
- RES-0021: WIFIDIRECT-001 Android Wi-Fi Direct (P2P) transport 2024-2026 SOTA — 8 websearch evidence passes, primary sources (developer.android.com/source.android.com/Wi-Fi Alliance/w1.fi/AOSP wpa_supplicant/osv.dev/NVD), no AI citations; verdict PROCEED. Q1 ADOPT: classic WifiP2pManager (discoverPeers/discoverServices/createGroup/connect/removeGroup) fully supported + actively extended through API 36/Android 16 (USD-based discovery, R2, DIR API 36; CONNECTION_REQUEST_DEFER API 37) — no deprecation; DPP = infra-only NOT P2P; coex channel-avoidance (COEX_RESTRICTION_WIFI_DIRECT, WifiAvailableChannel API 34). Q2 ADOPT w/ condition: group default WPA2-Personal (PSK/AES-CCMP); WPA3-SAE now spec-supported on R2-capable devices (WifiP2pGroup SECURITY_TYPE_WPA3_SAE/COMPATIBILITY API 36, PCC modes; WPA3 v3.5 PMF-required-in-Only-mode) → floor WPA2, SAE capability-gated later phase; OWE infra-only. Q3 ADOPT w/ caveat: WFA mandates 1:1, one-to-many optional — "1 GO + 8 clients" = vendor/HAL ceiling NOT spec guarantee (G-WD-1); STA+P2P concurrency optional/HAL-dependent → N-client admission + GO-side connection table. Q4 ADOPT: DNS-SD (Bonjour) discovery preferred; framework P2P find window = 120 s (DISCOVER_TIMEOUT_S) — doc "30-s" = app re-arm cadence (G-WD-7); discovery best-effort on OEM builds → BLE control plane durable trigger. Q5 ADOPT: createGroup() autonomous GO persists until removeGroup; persistent groups reinvokable (p2p_invite); setGroupOperatingBand = API 29 (correct doc "API 30+"), AUTO/2/5/6 GHz, band-constrained GO creation fails; client IP IPv4 DHCP or new IPv6-link-local; MAC randomization tied to persistent-group presence; GO default intent 6. Q6 DEFER: WPS deprecated client-mode API 28 + WPS-PIN attack target (CVE-2021-0326); NFC OOB in spec §3.1.2.7 not in Android API; modern path = R2 pairing bootstrapping API 36 (WifiP2pPairingBootstrappingConfig incl. OUT_OF_BAND over BLE) → IRIS skip WPS, push passphrase over authenticated BLE control plane, PBC legacy fallback only. Q7 ADOPT: wpa_supplicant full P2P surface (p2p_group_add [persistent] [freq], p2p_connect, p2p_service_add bonjour, p2p_invite, p2p_get_passphrase, group iface p2p-wlan0-N); IP/DHCP NOT automatic parity — IRIS Linux nodes own GO static + dnsmasq/udhcpd glue; Android GO IP 192.168.49.1 practice-stable but verify AOSP (G-WD-2). Q8 ADOPT w/ GATE (OS patch floor): no new unauthenticated P2P RCE 2024-2026 but class real — w1.fi 2026-1 (mgmt frames mem corruption pre-v2.12), 2026-3 (SAE NULL-deref), kernel Wi-Fi driver stream (CVE-2024-26895/27053/47712/47724/56539/46755, 2025-40321, 2026-31780/46069), CVE-2021-0326 RCE fixed SPL 2021-02 → AC: Android SPL ≥ 2021-02, wpa_supplicant ≥ 2.12, kernel w/ 2024-26 fixes; WPS-PIN prohibited. Gaps G-WD-1..8. DESIGN decisions D-1..D-7 (band policy / group security+passphrase origin / discovery re-arm / Linux IP glue / client capacity / patch floor / concurrency posture).

- RES-0022: ANDROID-001 Android Platform Integration SOTA — 8 websearch evidence passes (2026 bias, L1-L5, no AI citations); verdict PROCEED. Q1 ADOPT: UniFFI foreign traits (#[uniffi::export(foreign)] proc-macro; callback interfaces soft-deprecated) + async trait methods over FFI = Kotlin-adapter injection bridge (BleAdapter sync 10-op / WifiAwareAdapter 12-op + WifiDirectAdapter 20-op async → Kotlin suspend); trap issue #2576 async_runtime attr ineffective on exported-trait impls → explicit tokio Handle; constraints return-Result + by-value (#2263). Q2 ADOPT: UniFFI 0.31.x + cargo-ndk 4.1.2 (MSRV 1.86, NDK r26 floor pinned, ABI {arm64-v8a, armeabi-v7a, x86_64}) + rust-android-gradle 0.9.6 (Gradle 8.x) | Mullvad 0.10.1 (Gradle 9+); generated Kotlin committed. Q3 ADOPT: Kotlin 2.2.x + Compose + Hilt; FGS connectedDevice (API 34+) + PendingIntent BLE scanning + WorkManager + OEM battery-kill matrix; floors targetSdk 34/minSdk 26. Q4 ADOPT: Android 13+ Keystore Ed25519/X25519 hardware-backed (KeyMint v2 TEE); StrongBox excludes Ed25519 (CTS 399856239) → identity TEE Ed25519 (RED-0005), session keys in-engine. Q5 ADOPT + carry-forward: BLE/Wi-Fi Aware (API 37)/Wi-Fi Direct (API 36/37) all supported; CRITICAL Android 17 ACCESS_LOCAL_NETWORK (targetSdk 37+) gates the ENTIRE IRIS data plane → v1 stays 34, SDK-37 cliff documented; BluetoothSocket.read() -1. Gaps G-AND-1..6. Verdict PROCEED → DESIGN (AC-1..n).

- RES-0023: TEST-001 Test Infrastructure SOTA — allocated iter 123 (DISCOVER
  COMPLETE); research 2026 best-in-class Rust/Android test infra over Gaps
  G-1..G-8 (nextest parallel runner, cargo-audit/dery supply-chain gates,
  kani/loom formal+concurrency, cargo-mutants mutation, PROTOCOL_CONFORMANCE
  interop fixtures, CI workflow, golden-vector corpus, bench harness) → verdict
  → DESIGN AC-1..n (iter ~125).

- RES-0024: BLE-002 iOS CoreBluetooth SOTA — allocated iter 133 (RESEARCH
  COMPLETE); 14 websearch + 4 primary-source fetches (Apple docs/security/
  forums incl. Apple engineers, Nordic Academy, NVD, Stack Overflow, Punch
  Through 2026); verdict **CONDITIONAL — PROCEED**. RQ-1 CoreBluetooth bg
  scan/advertise: no relaxation through iOS 26 (iOS 18.0 ad regression fixed
  18.1; iOS 26 HID/LE-Custom-Device UIBackgroundModes = AccessorySetupKit
  accessory-only, Live Activity = screen-on scan stops at screen sleep);
  RQ-2 no PendingIntent analog, overflow-area iOS-iOS-only, user force-quit
  kills relaunch; RQ-3 ~8-connection practical, maximumWriteValueLength =
  negotiated payload (cap 512, no request API), iOS 16.x 20-B MTU regression
  guard; RQ-4 verified CVEs CVE-2023-42941/2024-23241/2024-44124/2024-44191
  = OS-patchable only; prompt CVEs CVE-2023-28412 (Snap One), CVE-2024-44270
  (macOS), CVE-2021-31714 (unverifiable) = misattributed, excluded;
  RQ-5 codified ad asymmetry → connect-to-identify + stable IRIS service UUID,
  `supports_background_ios=false` held (ble.rs:287/test :1075); 7 DESIGN
  inputs (stable UUID, 28-B budget, foreground-always + Live Activity framing,
  512-B segmenter + 23-B fallback, state-restoration hardening, CVE posture,
  iOS patch floor). DESIGN iter ~134.

- RES-0025: IOS-001 iOS Platform Integration SOTA — allocated iter ~142
  (RESEARCH COMPLETE 2026-08-18); 9 websearch + primary-source fetches
  (uniffi-rs docs/changelog/issues, Apple CryptoKit/Keychain/Platform
  Security/BGTask/ActivityKit docs + WWDC26, rustc book + cross-rs/osxcross,
  actions/runner-images + setup-xcode); verdict **PROCEED**. RQ-1 ADOPT-WITH-
  CONDITION: Swift foreign traits (#[uniffi::export(foreign)]/rust,foreign)
  + async trait methods over FFI = Kotlin proof transfers (L1); traps: #2576
  async_runtime-on-trait-exports open on 0.31.x (fixed only in 0.32.0 #2899);
  #2929 async foreign-trait Swift bindings fail under Swift 6 strict
  concurrency `.v6` (open; workaround = Swift-5 mode or post-codegen
  Sendable patch); generated protocols require Sendable impls.
  RQ-2 ADOPT-WITH-CONDITION: SE = ECDSA P-256 only (confirmed through
  current iOS, L1); identity = CryptoKit Ed25519 app-layer key persisted as
  Keychain generic password, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
  NO biometric flags (background-relay); SE ECDSA rescue DEFERRED.
  RQ-3 ADOPT-WITH-CONDITION: macOS-runner-gated (macos-15 + Xcode 16.4,
  iOS 18 SDK floor per App Store 2025-04-24; iOS 17 SDK absent from current
  images); Linux iOS cross-build via cross-toolchains/osxcross = licensed-SDK
  friction, rejected; UniFFI 0.31 flow = generate --library (IOS.md UDL cmd
  STALE); macOS-host unit tests w/ IOS-CoreBluetooth-Mock, physical iPhone
  still BLK-0005. RQ-4 ADOPT-WITH-CONDITION: thin Swift SessionRecovery
  coordinator warranted (launch-reason classification, re-arm in
  willRestoreState, re-submit BGTasks every launch, force-quit no-op).
  RQ-5 ADOPT-WITH-CONDITION: Live Activities floor = iOS 16.1 (not 16.2,
  corrected); current through iOS 26/27, 8h+4h=12h limit, sandboxed;
  screen-on scan continuation stops at sleep (RES-0024 carry);
  LiveCommunicationKit = voice/video not BLE. 7 DESIGN inputs +
  EXTERNAL-FACTS (IOS.md Xcode floor, UniFFI flow, SE section, BGTask
  wording; SWIFT_LAYER re-arm; iOS 16.2→16.1). DESIGN iter ~143.

- RES-0026: PILOT-001 Field Pilot Deployment SOTA — allocated iter ~152
  (RESEARCH COMPLETE 2026-08-19); 12 websearch + 4 primary-source passes
  (Meshtastic official docs/blog + firmware source, arXiv 2603.10153 DTN
  trial, Briar/SecureJoin, NDMA DMEx Guidelines Oct 2024 + Mock-Exercises
  page + PIB Suraksha Chakra 2025, WPC G.S.R. 853(E) 2021 gazette + TEC ER,
  IT Rules SSMI 50-lakh, Play internal/closed testing + TestFlight, FEMA
  HSEEP; L1-L5 evidence-leveled, no AI citations); verdict **PROCEED**.
  RQ-1 ADOPT-WITH-CONDITION (managed-flood 100+ node precedent + Helene
  gateway-uplink field precedent; conditions = relay-cadence scaling rule,
  gateway-scoped uplink/downlink, fixed NCT-of-N/partition/mule test battery,
  LoRa excluded GAP-004); RQ-2 ADOPT-WITH-CONDITION (Briar/SecureJoin QR
  mutual-scan bootstrap + IDENT-001 verified tier + EMERG-001 out-of-band
  authority provisioning; batch ceremony, no-recovery model, decommission);
  RQ-3 ADOPT-WITH-CONDITION (OBS-001 ↔ NDMA DMEx 4-phase mapping; thresholds
  + Wilson-CI sample plan as DESIGN ACs; EXP-002/003/005 before/after legs);
  RQ-4 ADOPT codification (WPC G.S.R. 853(E) 2021 = 865-868 MHz SRD 25 mW
  e.r.p./≤1% duty supersedes 865-867 LEGAL-001 carry; SSMI 50-lakh
  non-applicable at pilot scale; NDRF MoU authorization + STQC GA; open
  counsel Qs recorded, no legal opinion); RQ-5 ADOPT-WITH-CONDITION (Play
  **internal** track ≤100 testers primary — DISCOVER 'closed track' REFINED;
  closed-track 12-testers×14-days for later production; TestFlight external
  + 90-day refresh; Play App Signing upload-key custody); RQ-6 ADOPT
  (EMERG-001 drill mode = NDMA Mock Exercise standard; DRILL-chain + TEST-only
  SOS + observers/debrief + safety annex). 9 DESIGN inputs D-1..D-9 (topology,
  relay-cadence rule, test battery, ProvisioningFlow, trust model, KPI plan,
  exercise schedule, distribution runbook, REG-NOTES) + external-facts
  reconciliation + gaps G-P1..G-P9. DESIGN iter ~154.

- RES-0027: LORA-001 LoRa transport SOTA — allocated iter ~161 (RESEARCH COMPLETE 2026-08-19); websearch + primary-source passes (WPC G.S.R. 853(E) 2021 Table-I 865-868 25 mW e.r.p./1% duty L1-confirmed via thc.nic.in/G25977.pdf; LoRaWAN IN865 30 dBm = network-plan only; AN1200.13 sensitivity; BLE-GATT/USB-SLIP bridge; DutyCycleTracker; Meshtastic override NOT followed); verdict PROCEED; DESIGN inputs D-1..D-7 -> LORA_001_DESIGN.md (iter ~162).
 | Research | RES-0028 | done | SAT-001 satellite gateway transport research (Iridium SBD/AT-modem target, envelope verdict, cost guard, security, India regulatory) | 2026-08-22 |

### Research additions
- RES-0029: WIFIDIRECT-001 Mobly readiness/observation failure research — allocated 2026-09-06; Android Wi-Fi Direct lifecycle, Mobly asynchronous RPC guidance, and fresh two-phone logcat evidence; verdict ADOPT-WITH-CONDITION; harness must poll one message ID through the cycle budget rather than create new IDs while the application socket is attaching.
- RES-0033: ANDROID-PAIR-001 trusted-peer QR/manual pairing and alias UX — allocated 2026-09-08; official CameraX + bundled ML Kit evidence, hostile-input validation, signed-advertisement/SAS design; verdict PROCEED-WITH-CONDITIONS.

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
