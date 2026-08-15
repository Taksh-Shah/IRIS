# DECISIONS.md — Active Decision Log

**Schema version**: 1.0
**Last updated**: 2026-08-15T16:30:00Z

---

## Active Decisions

### DEC-0001: CBOR over Protocol Buffers
**Status**: Approved (ADR-0001)
**Date**: 2026-01-15
**Node**: PROTO-001
**Summary**: CBOR (RFC 8949) chosen for self-describing binary encoding; no schema compilation; DTN ecosystem uses it
**Consequence**: Manual field validation required; slightly larger than protobuf

### DEC-0002: Ed25519 for Identity and Signing
**Status**: Approved (ADR-0002)
**Date**: 2026-01-15
**Node**: CRYPTO-001
**Summary**: Ed25519 chosen over RSA and ECDSA for constant-time 64-byte signatures; fast verification on ARM
**Consequence**: Not FIPS certified; government waivers may be needed

### DEC-0003: Rust for Core Protocol
**Status**: Approved (ADR-0003)
**Date**: 2026-01-20
**Node**: ARCH-001
**Summary**: Rust chosen for memory safety, C-level performance, cross-platform FFI
**Consequence**: Smaller talent pool in India; 2-4 week onboarding needed

### DEC-0004: SQLite for Message Store
**Status**: Approved (ADR-0004)
**Date**: 2026-02-01
**Node**: STORE-001
**Summary**: SQLite (rusqlite) chosen for DTN bundle store; WAL mode, ubiquitous on mobile
**Consequence**: Batch inserts needed for high-throughput scenarios

### DEC-0005: PRoPHET as Primary Routing
**Status**: Approved (ADR-0005)
**Date**: 2026-02-10
**Node**: ROUTE-001
**Summary**: PRoPHET with Spray-and-Wait fallback chosen over Epidemic and MaxProp
**Consequence**: Parameters require tuning for Indian disaster mobility

### DEC-0006: ChaCha20-Poly1305 for Symmetric Encryption
**Status**: Approved (ADR-0006)
**Date**: 2026-02-15
**Node**: CRYPTO-001
**Summary**: ChaCha20-Poly1305 (RFC 8439) chosen over AES-GCM for software performance on ARM without AES-NI
**Consequence**: Not FIPS certified; HKDF-BLAKE3 non-standard construction needs resolution

### DEC-0007: Control Plane Architecture
**Status**: Approved
**Date**: 2026-08-11
**Node**: N/A (meta)
**Summary**: Durable engineering memory system using markdown files in engineering/memory/, addressable records in engineering/memory/records/, OpenCode plugin for compaction resilience
**Consequence**: State must be manually committed; no automatic git commits by agents

### DEC-0008: Flat Envelope Protocol Structure
**Status**: Approved (ADR-0011)
**Date**: 2026-08-11
**Node**: PROTO-001
**Summary**: Single CBOR map with integer keys chosen over BPv7 block-based structure and hybrid approach. P0 uses abbreviated format. Append-only extension mechanism for forward compatibility.
**Consequence**: No native BPv7 interop (requires future gateway); simpler parsing; optimal P0 LoRa fit; adequate extensibility for v1

---

## Pending Decisions (Require Human Approval)

### DEC-P0001: Per-session vs Per-message Ephemeral Key
**Status**: RESOLVED 2026-08-14 (CRYPTO-001 RESEARCH, RES-0015 D1)
**Node**: CRYPTO-001
**Summary**: **Per-message ephemeral X25519 (ECIES-like)** per MESSAGE_ENVELOPE.md encryption_hdr. ADR-0006 "per-session" wording corrected — sessions do not exist in async store-and-forward DTN (recipient offline, relays hold bundles for days, no interactive handshake). Sender-side FS yes; recipient-FS no (documented, see DEC-P0003/Q4).
**Evidence**: RES-0015 §2; ADR-0006 Issue 1 (line 137); MESSAGE_ENVELOPE.md; Signal X3DH spec (async prekey design, recipient-FS via OPK); Noise_XX (requires interactivity — not v1)

### DEC-P0002: HKDF Construction Standard
**Status**: RESOLVED 2026-08-14 (CRYPTO-001 RESEARCH, RES-0015 D2)
**Node**: CRYPTO-001
**Summary**: **RFC 5869 HKDF-SHA256** (RustCrypto `hkdf` + `sha2`). HKDF-BLAKE3 rejected (non-standard — DISC-0002/FAIL-0003; HMAC-BLAKE3 not IETF construction). BLAKE3 `derive_key`/hash retains non-crypto-only roles (message_id, payload_hash, Bloom). KAT via RFC 5869 Appendix A.
**Evidence**: RES-0015 §3; RFC 5869 (Level 1); Hoang 2025 HKDF pseudorandomness proofs (Level 2); DISC-0002; FAIL-0003

### DEC-P0003: Forward-Secrecy Honesty (v1 scope)
**Status**: RESOLVED 2026-08-14 (CRYPTO-001 RESEARCH, RES-0015 D4) — **documented limitation**, not a feature
**Node**: CRYPTO-001
**Summary**: v1 provides sender-authentication + confidentiality + sender-side FS; **NO forward secrecy against recipient key compromise** (ECIES-like static-recipient construction). No doc may claim recipient-FS. v2: X3DH-style prekeys over DISCO-001 capability exchange + Double Ratchet (BLK-0002). Redteam checkpoint verifies no FS overclaim.
**Evidence**: RES-0015 §5; ADR-0006 Issue 3; DISC-0003; Signal X3DH spec §4; Prekey Pogo (WOOT 2025)

### DEC-P0004: At-rest Row Encryption (STORE-001)
**Status**: RESOLVED 2026-08-14 (CRYPTO-001 RESEARCH, RES-0015 D3)
**Node**: CRYPTO-001 / STORE-001
**Summary**: At-rest row seal in iris-storage with storage key = HKDF-SHA256(node master key, "iris-at-rest-v1"); AEAD ChaCha20-Poly1305 per-row random 12-byte nonce; index columns (message_id/priority/ttl/expiry) plaintext, envelope CBOR + sender/recipient/timestamp sealed; master key in Android Keystore (TEE/StrongBox, non-exportable) / OS keychain (desktop) — provisioning in IDENT-001.
**Evidence**: RES-0015 §4; DEC-0002 line 55; source.android.com Keystore (Level 1); Android Keystore system docs

### DEC-P0005: Device Identity Model & Address Derivation
**Status**: RESOLVED 2026-08-15 (IDENT-001 RESEARCH, RES-0016 D1)
**Node**: IDENT-001
**Summary**: Per-device Ed25519 identity keypair; **sender_id (16 B wire) = first 16 bytes of SHA-256(Ed25519 pubkey)**, full hash = internal PeerId; node UID (e.g. `IRIS_NODE_ID`) is a separate non-authoritative convenience label. Derived from the public key ONLY — never MAC/serial/hardware (Meshtastic NodeNum MAC-derived identity spoofing CVE-2025-53627, CVSS 8.2). did:key-compatible multicodec 0xed derivation preserved as documented view, not a v1 requirement.
**Context**: BPv7 (RFC 9171) and the IRIS flat envelope both assert identity in-band; only a key-derived identifier + verified signature make it trustworthy. libp2p PeerId (multihash of pubkey) and Tor onion v3 (base32 key-derived) are the canonical self-authenticating precedents.
**Alternatives**: MAC/hardware-derived IDs (rejected — CVE-2025-53627); full DID resolution (not needed v1); random per-process IDs (rejected — not persistent/authenticated).
**Consequence**: Identity regenerates only with the keypair; sender_id collision risk is 2^-64; wire field stays 16 bytes (RED-0010 codec revision tracked).
**Evidence**: RES-0016 §1; RFC 9171; CVE-2025-53627; libp2p spec; W3C did:key; Tor spec

### DEC-P0006: Key Provisioning Strategy (per-platform)
**Status**: RESOLVED 2026-08-15 (IDENT-001 RESEARCH, RES-0016 D2)
**Node**: IDENT-001 / ANDROID-001 / IOS-001 / RED-0001
**Summary**: Master key + Ed25519 identity live in the strongest available platform store: **Android Keystore TEE** (StrongBox optional for master — StrongBox keygen ≈9.2 s/1 MiB ≈3 s, KeyDroid arXiv:2507.07927 — NEVER per-message); **iOS Keychain** (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` — Secure Enclave is NIST P-256-only, Ed25519/X25519 cannot live there); **desktop** OS keychain or protected key file (RED-0001 wiring deliverable); **Pi** external TPM (SLB9670/9672) sealing only, no measured boot (documented weaker posture). App keys: wrapped-DEK pattern (master non-exportable wraps storage DEK). `StorageKeySealer::from_master_key` becomes default-on at-rest sealing once provisioned. AOSP keystore's own HKDF-SHA256 super-key design independently corroborates DEC-P0004.
**Context**: "Where does the master storage key live?" was the open provisioning question deferred from CRYPTO-001 RES-0015.
**Alternatives**: Stronghold software vault (rejected as root of trust — official docs say "not yet audited for security"; software-only); static key file everywhere (rejected — no hardware protection on mobile).
**Consequence**: Platform-specific provisioning code needed (IDENT-001 core + platform adapters); per-platform key availability semantics differ (Android Keystore best, Pi weakest).
**Evidence**: RES-0016 §2; source.android.com Keystore; AOSP source; Apple Keychain docs; KeyDroid (Level 2); Infineon TPM docs; IOTA Stronghold docs

### DEC-P0007: Trust Model (TOFU + signed advertisement + pairing tier)
**Status**: RESOLVED 2026-08-15 (IDENT-001 RESEARCH, RES-0016 D3)
**Node**: IDENT-001
**Summary**: **TOFU baseline** (first-seen key binds; key change → user warning; Meshtastic: "a hard requirement" of decentralized mesh — no PKI). Higher assurance tier via **QR/pairing-code exchange** (Briar BQP style, verified vs unverified UI 3-vs-2-bars). RED-0005 signed key advertisement = **SPKI-style (RFC 9804, re-ratified 2025-06) keyholder cert**: Ed25519 identity signs the X25519 static public key. RED-0008 `auth_cert_chain` = key-anchored chain of authorization (keyholder model), NOT a CA hierarchy, NOT PKIX/X.509.
**Context**: Trust-on-first-use vs pairing codes was an open question; RFC 9804 re-ratification gives the maintained spec line for key-as-subject certs; routing-reputation trust (PRoPHET literature) is explicitly NOT identity trust.
**Alternatives**: CA/certificate hierarchy (rejected — no trusted directory in disaster mesh); no verification at all (rejected — RED-0008 requires chain validation).
**Consequence**: TOFU + QR tier + signed-advertisement chain; UI must distinguish verified/unverified; X.509/PKIX avoided (RUSTSEC pressure: rustls-webpki 2026-0104/0098/0099).
**Evidence**: RES-0016 §3; RFC 9804; Briar spec + Tamarin analysis; Meshtastic docs; ZI-CG arXiv:2601.02254

### DEC-P0008: Rotation & Revocation (v1 scope)
**Status**: RESOLVED 2026-08-15 (IDENT-001 RESEARCH, RES-0016 D4)
**Node**: IDENT-001
**Summary**: Rotation/revocation v1 = **signed rotation events** over the existing KEY_ROTATION path (envelope type 16): monotonic key-generation counter + valid-until timestamp + **earliest-seen-wins on conflict** (Ceramic offline-ordering lesson). Revocation = signed rotation moving identity to a placeholder "revoked" key (**KERI null-rotation semantics**), propagated as a signed advertisement via gossip/SCF. Full KERI-style signed event log with equivocation detection = v2 (BLK-0002).
**Context**: BPSec RFC 9172 §6 explicitly leaves key management out of scope; offline networks cannot authenticate "newest" — monotonic counters + valid-until windows are required.
**Alternatives**: Central revocation authority (rejected — no central directory); timestamp-ordering (rejected — untrusted clocks offline).
**Consequence**: Key rotation without connectivity converges via SCF/gossip; a compromised old key can be rotated out but historical ciphertext remains decryptable by the old key holder (documented — recipient-FS is v2 per DEC-P0003).
**Evidence**: RES-0016 §4; RFC 9172 §6; RFC 9173; Ceramic 2022 analysis; KERI spec; ICN revocation 2026

### DEC-P0009: Identity ↔ X25519 Key Binding (RED-0005)
**Status**: RESOLVED 2026-08-15 (IDENT-001 RESEARCH, RES-0016 D5)
**Node**: IDENT-001
**Summary**: **Two distinct keypairs** (Ed25519 identity signing, X25519 static encryption) bound by the Ed25519 identity signing the X25519 static public key in the key advertisement (the RED-0005 deliverable, SPKI/RFC 9804 keyholder form). **NO XEdDSA/curve conversion in v1** — Signal X3DH/XEdDSA and Meshtastic 2.8.x prove the same-key route is deployed, but LSEG (arXiv:2511.07548) and IACR 2021/509 both carry the "careful analysis" caveat; two-key binding gives clean separation, verify_strict-only single code path, small-order defense (RED-0011) transparently applied to remote X25519 keys.
**Context**: CRYPTO-001 redteam RED-0005 (MEDIUM) required an explicit, documented identity↔key binding rather than implicit trust.
**Alternatives**: XEdDSA single-key signing (rejected v1 — conversion caveats); no binding (rejected — RED-0005); only identity key used for both (rejected — violates key separation).
**Consequence**: Key advertisement is verifiable by any peer holding the Ed25519 identity; encryption keys remain rotatable independently of identity; wire format unchanged (advertisement is application data).
**Evidence**: RES-0016 §5; Signal X3DH/XEdDSA spec; Meshtastic 2.8.x; IACR 2021/509; LSEG arXiv:2511.07548

### DEC-P0010: Emergency Alert Wire Format (DEC-EMERG-0001)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D1)
**Node**: EMERG-001
**Summary**: **Minimal CAP-inspired CBOR subset** on the existing flat envelope (field 13 payload) for `ContentType::EmergencyAlert` (P3, 1 KB budget). OASIS CAP v1.2 = semantic reference only (XML, no wire conformance); ITU-T X.1303bis (ASN.1/PER) rejected as encoding (FAIL-0005). Fields: broadcast_id[16], republish_id, issued_at, expires_at (≤72 h), severity (CRITICAL..MINOR/TEST), certainty, message_type (ALERT/UPDATE/CANCEL/ACCEPT/DRILL/REVOCATION_LIST), area_code (ISO 3166-2), language (BCP 47), headline ≤96, instructions ≤96, authority meta, drill. Authority broadcasts stay wire-priority P3 (cannot be P0 given 255-B/84-B envelope + chain size); severity subfield drives UI/takeover tiers (C1).
**Evidence**: RES-0017 §1/§8; OASIS CAP v1.2; RFC 8949 §4.2; FAIL-0005

### DEC-P0011: Emergency Authority Identity (DEC-EMERG-0002)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D2)
**Node**: EMERG-001
**Summary**: **Reuse IDENT-001 `verify_chain` (RED-0008)** SPKI/RFC 9804 key-anchored chain for emergency authority certificates — no new certificate struct, `KeyAdvertisementV1` is the chain element. Chain depth ≤4 (National→State→District→Local). Preloaded NDMA root in `TrustStore` (is_trusted_root). Offline verification at every relay. Validities 2y/1y/6mo/3mo per governance (IRIS-specific — offline-revocation lag tradeoff, no external benchmark). No PKIX/rustls-webpki. Geo/functional scope + permissions = payload-level authority profile, enforced: area ⊂ authority area, message_type ⊂ functional scope, severity ≤ broadcast_max_severity.
**Evidence**: RES-0017 §2/§8; RFC 9804; RED-0008; CVE-2025-52464; EMERGENCY_GOVERNANCE

### DEC-P0012: SOS Semantics (DEC-EMERG-0003)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D3)
**Node**: EMERG-001
**Summary**: SOS = `ContentType::Sos` (P0), empty-recipient/`EMERGENCY_BROADCAST` broadcast path, **never encrypted** (no recipient key), Ed25519-signed, payload ≤ **84 B** (DISC-0008 ceiling): lat/lon i32 ×1e6, accuracy, location_source (GPS/NETWORK/USER_MANUAL/NONE), timestamp, kind (SOS/CANCEL/TEST), original_message_id, reason code. Encrypted direct SOS to known peers uses the normal message path. Free-text note is a follow-up P4 `Text` referencing the SOS id (C5).
**Evidence**: RES-0017 §3/§8; DISC-0008; EMERGENCY_UX; SOS.md

### DEC-P0013: Cancel & Status Semantics (DEC-EMERG-0004)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D4)
**Node**: EMERG-001
**Summary**: **Status reporting OFF by default** (RFC 9171 §6.2). BroadcastCancel = `EmergencyAlert` body `message_type := CANCEL` (typed admin-record model, RFC 9713); SOS-cancel = `Sos` body `kind := CANCEL` + original_message_id, same-signer-only, ≤60 min window. No per-relay broadcast ACK (anti-probing). SOS delivery ack = application message when direct recipient exists. No new ContentTypes in v1 (→ PROTO-001 v2/RED-0010).
**Evidence**: RES-0017 §5/§8; RFC 9171 §6.1/6.2; RFC 9713; EMERGENCY_ABUSE

### DEC-P0014: Rate Limiting & Sybil Posture (DEC-EMERG-0005)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D5)
**Node**: EMERG-001
**Summary**: **Per-sender** (`sender_id`) SOS rate limit at message-engine layer: 3/hr rolling 60-min window; 4th+ → **downgraded to P3** (not dropped); reset on verified CANCEL; authority broadcasts **never rate-limited**; disaster-mode soft raise to 5/hr (bounded burst). **Sybil caveat documented**: keygen is free ⇒ per-identity caps evadable; damage bounded by no-spoofed-broadcasts + area-cluster triggers + dedup + P0-exempt storage. Registry bounded (LRU 4096) vs memory DoS.
**Evidence**: RES-0017 §7/§8; SybilGuard/SybilLimit; EMERGENCY_ABUSE

### DEC-P0015: UX/A11y + Tone Correction (DEC-EMERG-0006, C3)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D6)
**Node**: EMERG-001
**Summary**: WEA-standard attention tone is **853+960 Hz** (not the docs' 880/660 Hz — C3 corrected in EMERGENCY_UX.md). WCAG 2.2 SC 1.4.6 (7:1 contrast) + SC 2.2.4 emergency-exception (30-s non-dismiss full-screen takeover legit). Platform split: Android via FGS/OS emergency channels when available; iOS in-app overlay only (no public SOS API). This node emits typed events; rendering → platform nodes.
**Evidence**: RES-0017 §6/§8; WCAG 2.2 SC 1.4.6/2.2.4; FCC WEA signal

### DEC-P0016: Legal Refresh BNS (DEC-EMERG-0007, C4)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D7)
**Node**: EMERG-001 / LEGAL-001
**Summary**: **BNS 2023 (Act 45 of 2023) replaced IPC effective 2024-07-01.** EMERGENCY_ABUSE.md migrates IPC §420→BNS §318, §505→BNS §353(2); **IPC §153A→BNS mapping unverified → LEGAL-001 hook** (DISC-0012). Legal consequence text updated before VERIFY.
**Evidence**: RES-0017 §7/§8; DISC-0012; indiacode.nic.in

### DEC-P0017: TEST_MODE Drill Capability (DEC-EMERG-0008)
**Status**: RESOLVED 2026-08-15 (EMERG-001 RESEARCH/DESIGN, RES-0017 D8)
**Node**: EMERG-001
**Summary**: TEST_MODE: `drill: bool` on broadcast + `kind := TEST` on SOS + DRILL-dedicated authority certs (never real). Drill broadcasts → yellow "DRILL" banner, never full-screen takeover / OS channels, isolated audit, cancel-anytime. Aligned with FEMA HSEEP exercise methodology. Drill data separate from real audit.
**Evidence**: RES-0017 §4/§8; FEMA HSEEP; PrepToolkit comms drills; ISCRAM 2024

---

## Superseded Decisions

None yet.

---

## Decision Record Format

New decisions use format:
```
DEC-XXXX: [Title]
Status: PROPOSED | APPROVED | REJECTED | SUPERSEDED
Date: YYYY-MM-DD
Node: XXXX-XXX
Summary: [One line]
Context: [Why needed]
Alternatives: [What was considered]
Consequence: [Trade-offs accepted]
Evidence: [Link to research/record]
```

### DEC-SEC-0001: General per-(sender, class) token-bucket rate limiter
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: srTCM RFC 2697-style token bucket per (sender_short, priority_class); lazy refill; silent drop on overflow; P0/P1 exempt from drops (queue-priority only).
**Context**: Only emergency SOS has rate limiting; RES-0018 R1 - RFC 9171 §6.9 mandates CL-level rate limiting; CVE-2024-47065.
**Alternatives**: fixed global cadence (Meshtastic ~2 s) - rejected (class fidelity); per-message PoW - rejected (battery/duty-cycle/asymmetry).
**Consequence**: Attackers can no longer flood a relay CPU/buffer with a class-mix at high rate; per-identity limits remain Sybil-defeatable (paired with DEC-SEC-0004/0006).
**Evidence**: RES-0018 R1/Q1; RFC 9171 §6.9; RFC 2697/2698; CVE-2024-47065

### DEC-SEC-0002: Per-sender storage quota + priority-reserved pool
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: Per-sender storage quota + P0/P1 priority-reserved pool (generalizes emergency reserved pool); eviction by lifetime/TTL + quota order; P0/P1 never evicted by quota.
**Context**: RES-0018 R2 - buffer floods dominate in DTN; Zekkori Claim-Carry-and-Check + SRED buffer analysis.
**Alternatives**: unbounded store with LRU-only - rejected (quota-aware eviction needed).
**Consequence**: Store-carry-forward buffer abuse bounded per sender.
**Evidence**: RES-0018 R2; Zekkori IJACSA 2021; RFC 9171 lifetime

### DEC-SEC-0003: Cross-reboot replay protection (freshness + high-water + persistence)
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: Inbound freshness window with per-source skew + per-sender monotonic high-water (ts,seq) + persist dedup Bloom+LRU snapshot and high-water marks (batched crash-safe, flash-wear safe); promotes the WP-2 deferred dedup persistence to a security requirement.
**Context**: RES-0018 R4/R5 - replays succeed across reboot today (memory-only dedup, WP-2 deferral in mod.rs:24); RFC 9171 §4.2.7 (time,seq); RFC 4303 window concept; RFC 7181 §23.2.
**Alternatives**: per-link SAs + ESN (RFC 4303 full) - deferred, needs pairwise keys; nonce-cache only - cold-cache restart hole.
**Consequence**: A relay's replay window survives reboot; batched writes avoid flash wear.
**Evidence**: RES-0018 R4/R5/Q2; RFC 9171 §4.2.7; RFC 4303 §3.4.3; RFC 7181 §23.2; RFC 7183

### DEC-SEC-0004: Reputation = local watchdog + verified-second-hand positives + audits; routing weight only
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: security/reputation.rs computes bounded per-peer Bayesian scores from (1) local forwarding observation, (2) second-hand positives from verified peers only (CORE positivity rule - negatives never propagate), (3) iTrust-style probabilistic audits; score = routing weight, never an admission gate.
**Context**: RES-0018 R8/Q4 - DTN reputation without central authority; Marti watchdog; iTrust; ITRM; Rep-AODV; blackhole/gray-hole detection.
**Alternatives**: global reputation consensus / EigenTrust - rejected (DTN sparse, convergence).
**Consequence**: Low-reputation relays remain usable as last-resort route (availability > trust purity - disaster requirement).
**Evidence**: RES-0018 R8/Q4; Marti MobiCom 2000; CORE 2002; iTrust TPDS 2014; ITRM ISIT 2009; Rep-AODV CMC 2024

### DEC-SEC-0005: Receiver-side spam scoring; no relay-side content drops
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: security/spam.rs produces a receiver/UI-visible "likely_spam" annotation (Bayesian-style, ≤512B-class payloads); relay never hard-drops content; scoring classes disjoint from P0-P3.
**Context**: RES-0018 Q4.2 - SMS-scale spam filtering has persistent FP problem (CIKM 2007); relay-side content drops would be a DoS vector + emergency-FP risk.
**Alternatives**: relay-side Bayesian hard-drop - rejected (FP on emergency-adjacent content intolerable; CPU DoS surface).
**Consequence**: Spam control is user-visible, not network-enforced.
**Evidence**: RES-0018 Q4.2; Cormack/Hidalgo/Sanz CIKM 2007

### DEC-SEC-0006: ACL-1 emergency send authorization (key-anchored allowlists)
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: security/acl.rs materializes per-alert-class allowlists (P0 broadcast = authority roles; P1 medical = authority/certified responders; SOS = any verified identity @3/hr); verify chain+sig before relay, silent-drop unverifiable; revocation via IDENT-001 rotation. Closes RED-0002 ACL-1 + RED-0003 signed encryption_intent.
**Context**: RES-0018 R9/Q4.3 - CAP v1.2 DSig, RFC 9804 SPKI chains, ETSI TS 102 900 §5.5 no-UE-auth, CACM 2023 WEA Ed25519 mitigation; EMERG-001 authority code already implements verify_authoritative.
**Alternatives**: capability tokens / badge servers - rejected (online authority or human round-trip, offline-infeasible).
**Consequence**: Emergency-class sends are authority-gated offline-verifiable; who-may-send closed with standards-level precedent.
**Evidence**: RES-0018 R9/Q4.3; CAP v1.2; RFC 9804; ETSI TS 102 900 §5.5; CACM 2023; EMERG-001 authority.rs

### DEC-SEC-0007: Per-message PoW REJECTED; identity-mint PoW DEFERRED to PROTO-001 v2
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: REJECT per-message PoW/client puzzles on the TX path (battery, WPC duty cycle, difficulty calibration, botnet asymmetry). DEFER identity-mint PoW (KeyChallenge keygen-range PoW; SyDeLP adaptive PoW) and VDF to PROTO-001 v2.
**Context**: RES-0018 R3/Q1.4 - Hristozov AISEC "puzzles unsuitable"; measured ECDSA ~4.1s on OpenMote; client-puzzle MTU regime problems; RFC 9171 §6.9 preference for rate limiting.
**Alternatives**: per-message PoW - rejected; capture-based rate limiting - adopted (DEC-SEC-0001).
**Consequence**: DOS_RESISTANCE.md Layer-6 PoW text corrected to REJECT/DEFER verdict.
**Evidence**: RES-0018 R3/Q1.4; Hristozov AISEC arXiv 1911.08134; Springer 2017

### DEC-SEC-0008: SybilGuard/SybilLimit/social-graph REJECTED for relays; verified-pairing via platforms
**Status**: APPROVED 2026-08-15 (SEC-001 DESIGN)
**Node**: SEC-001
**Summary**: REJECT SybilGuard/SybilLimit/social-graph topology defenses on mesh relays (sparse/partitioned DTN has no fast-mixing honesty region). Keep TOFU + SPKI key-anchored identity (IDENT-001); surface verified-pairing (BQP QR pattern) via platform nodes as the offline identity-cost primitive; per-identity limits documented Sybil-defeatable.
**Context**: RES-0018 R6/R7/Q3 - Douceur IPTPS 2002; Ostra NSDI 2008 link-credit model as conceptual budget=f(trusted links); Briar BQP + OTF-005; Meshtastic CVE-2026-42566 key-bound identity, CVE-2025-52464 key hygiene.
**Alternatives**: SybilGuard/Limit - rejected (assumptions fail); CAPTCHA/badge servers - rejected (offline).
**Consequence**: Identity remains key-bound and revocable; identity-cost via verified pairing is platform-node scope; IRIS documents its Sybil boundary honestly.
**Evidence**: RES-0018 R6/R7/Q3; Douceur IPTPS 2002; Ostra NSDI 2008; Briar; CVE-2026-42566; CVE-2025-52464
