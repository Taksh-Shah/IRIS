# DECISIONS.md — Active Decision Log

**Schema version**: 1.0
**Last updated**: 2026-08-18T06:10:00Z

---

## Active Decisions

### DEC-TEST-0001: cargo-nextest primary runner (doctests stay on cargo test --doc)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: cargo-nextest = primary workspace runner (`.config/nextest.toml`: ci profile, JUnit XML, retries, leaky-test detection 100 ms / LEAK-FAIL). **Doctests are NOT run by nextest** (stable-Rust limitation issue #16) → separate `cargo test --doc` step (tokio CI precedent). No MSRV constraint on the tested project.
**Context**: RES-0023 G-1 (nexte.st L1, tokio CI L4).
**Alternatives**: remain on libtest (rejected — no parallel sharding, JUnit, retries, leaky detection).
**Consequence**: AC-1, AC-9.
**Evidence**: https://nexte.st/ (L1), tokio ci.yml (L4), accessed 2026-08-18

### DEC-TEST-0002: cargo-deny + cargo-audit as recurring supply-chain gates
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: cargo-deny 0.20.x = PR policy gate (`deny.toml`: advisories deny + unmaintained=workspace, bans multiple-versions=deny + wildcards=deny, licenses allowlist MIT/Apache-2.0/ISC/Zlib, sources unknown-registry/unknown-git deny) + cargo-audit = fast live-DB scan on PR/schedule (`--deny warnings`, committed `.cargo/audit.toml` suppressions with mandatory expires+reason, SARIF upload). Complementary cadences (BLK-0003).
**Context**: RES-0023 G-2 (cargo-deny book L1, RustSec L1, crates.io L1).
**Alternatives**: one-off `cargo audit` (EMERG-001 precedent) — rejected as non-recurring.
**Consequence**: AC-2, AC-9.
**Evidence**: embarkstudios.github.io/cargo-deny (L1), rustsec.org (L1), accessed 2026-08-18

### DEC-TEST-0003: Kani proofs scoped to pure functions (Linux CI only)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: Kani (bit-precise model checker) proves memory-safety/panic/overflow/assert properties for pure functions — codec decode bounds, fragment assembly arithmetic, quota/rate-limiter arithmetic, PRoPHET score, envelope size invariants. Kani **does NOT model concurrency** (official limitation) and installs Linux/Mac only → Linux CI job, no concurrency claims.
**Context**: RES-0023 G-3 (kani book L1, ASE 2026 L2).
**Alternatives**: full-model formal verification (rejected — Kani concurrency ceiling).
**Consequence**: AC-3, AC-9, gate matrix.
**Evidence**: model-checking.github.io/kani (L1), arXiv 2607.01504 (L2), accessed 2026-08-18

### DEC-TEST-0004: loom leaf structures + tokio-test async behavior
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: loom (`#[cfg(loom)]` replacement types) = leaf concurrent structures only (dedup cache, replay high-water marks, quota counters), soundness limits documented (SeqCst-as-AcqRel, no load-buffering); tokio-test = async behavior suites (message_engine queue/ack/lifecycle, transport::manager registration/start-stop under paused time). **No tool models full transport::manager** → correctness rests on tokio/property/adversarial/fuzz layers.
**Context**: RES-0023 G-3 (loom docs L1, tokio-test L1).
**Alternatives**: transport::manager-wide loom modeling (rejected — intrusive, unsound for SeqCst/load-buffering).
**Consequence**: AC-4.
**Evidence**: docs.rs/loom (L1), docs.rs/tokio-test (L1), accessed 2026-08-18

### DEC-TEST-0005: tarpaulin workspace coverage gate (≥ 80%, security ≥ 95%)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: tarpaulin `--out lcov --fail-under 80` as PR/merge gate on **ubuntu x86_64 only** (ptrace engine); security path stays ≥ 95% (SEC-001 AC-14 baseline). JVM path in separate job.
**Context**: RES-0023 G-4 (tarpaulin README L4).
**Alternatives**: coverage gate on all OSes (rejected — ptrace Linux x86_64 only; llvm engine elsewhere is post-1.0 caveat).
**Consequence**: AC-5, AC-9, gate matrix.
**Evidence**: github.com/xd009642/tarpaulin (L4), accessed 2026-08-18

### DEC-TEST-0006: cargo-mutants nightly trend, NOT hard PR gate v1
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: Mutation sweep = nightly/sharded **trend signal** (survival-rate metric frozen: `killed / (total − unviable − timeout)`, exemptions scoped with rationale); **not** a hard PR gate (hour-scale cost, flakiness sensitivity — G-TI-2). PR-time = changed-file incremental. Requires deterministic suite → nextest retries off in the mutants profile.
**Context**: RES-0023 G-4 (mutants.rs L1/L4, tensogram L4, ThoughtWorks L4/L5).
**Alternatives**: hard mutation-score gate (rejected — economically destructive on 50k-LOC core).
**Consequence**: AC-6, AC-9.
**Evidence**: mutants.rs (L1/L4), tensogram docs (L4), accessed 2026-08-18

### DEC-TEST-0007: PROTOCOL_CONFORMANCE interop fixtures (BLK-0004)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: `tests/protocol_conformance.rs` + `tests/fixtures/`: RFC 9171/RFC 8949-derived CBOR vectors (shared primitive encodings), Meshtastic-style bounded-recovery resync adversarial fixtures (corrupt length byte swallows ≤ 512 B until resync), capture-style PDU fixtures (Wireshark BP pattern); property-based sim harness over existing Sim* coordinators (ns-3 BPv7 pattern).
**Context**: RES-0023 G-5 (RFC 9171 L1, Meshtastic L4, Wireshark L4, Unibo-BP L2, ns-3 L4).
**Alternatives**: no interop fixtures (rejected — BLK-0004 open).
**Consequence**: AC-7.
**Evidence**: rfc-editor.org/rfc/rfc9171 (L1), wiki.wireshark.org/BP (L4), accessed 2026-08-18

### DEC-TEST-0008: Golden-vector corpus (byte-exact both-way)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: `tests/golden_vectors/` committed + byte-exact both-way encode/decode harness; IRIS envelope + framing + CBOR/UUIDv7/timestamp vectors. Catches **DISC-0009-class** errors (PROTOCOL_TEST_VECTORS.md timestamp hex arithmetic) permanently. Corpus provenance: IRIS envelope is flat non-RFC-9171 → vectors generated in-repo from DESIGN spec + round-trip/fuzz cross-check (G-TI-4); maturity DESIGNED→UNIT_VALIDATED.
**Context**: RES-0023 G-7 (RFC 9171 L1, Wireshark L4, in-repo DISC-0009 L4).
**Alternatives**: hand-entered vectors only (rejected — provenance gap; DISC-0009 recurrence).
**Consequence**: AC-8.
**Evidence**: rfc9171 (L1), wiki.wireshark.org/BP (L4), accessed 2026-08-18

### DEC-TEST-0009: GitHub Actions CI workflow (official + tokio reference)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: `.github/workflows/ci.yml`: checkout@v4 → dtolnay/rust-toolchain@stable (+ MSRV cell via matrix `include:`) → Swatinem/rust-cache@v2 per-cell `key:` → taiki-e/install-action@v2 `tool: cargo-nextest` → `cargo nextest run --profile ci` + separate `cargo test --doc`; matrix {ubuntu, macos, windows}-latest × {stable, MSRV}, `fail-fast: false`; lint (fmt+clippy) once on ubuntu; deny/audit jobs; tarpaulin job (ubuntu); mutants nightly job; Windows/macOS = behavioral only (G-TI-1).
**Context**: RES-0023 G-6 (GitHub docs L1, action repos L4, tokio CI L4).
**Alternatives**: single-OS CI (rejected — matrix proves cross-platform as ANDROID-001 legs demand).
**Consequence**: AC-9.
**Evidence**: docs.github.com rust CI (L1), tokio ci.yml (L4), accessed 2026-08-18

### DEC-TEST-0010: JVM job on JDK ≤ 21 (Temurin), ubuntu runner
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: `actions/setup-java` Temurin ≤ 21 (Gradle 8.9 compatibility constraint, G-TI-5) runs `android/app` JUnit5 suites on ubuntu; cargo-ndk/uniffi-bindgen legs stay env-gated (BLK-0005/ANDROID-001 notes).
**Context**: RES-0023 G-6/G-TI-5; TEST-001_DISCOVER §2 (env-gated JVM).
**Alternatives**: JVM on dev host (rejected — no gradle/kotlinc, JDK 25 vs Gradle 8.9 ≤21).
**Consequence**: AC-9, AC-10 (non-blocking).
**Evidence**: Gradle 8.9 compatibility matrix (verify at IMPLEMENT), DISCOVER §2 (L4)

### DEC-TEST-0011: criterion 0.8.x bench harness now, regression/battery gate deferred
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: `benches/` criterion 0.8.x harness NOW (harness=false, criterion_group!/criterion_main!, `std::hint::black_box`) for CPU paths (codec, envelope, crypto, routing score, fragment, store); runs on CI without regression gate; **battery/physical-transport perf DEFERRED to BLK-0005** hardware. `#[bench]` is nightly-only hard error ≥ 1.88 → criterion only stable option. cargo-criterion still experimental → plain `cargo bench`.
**Context**: RES-0023 G-8 (cargo bench L1, criterion docs L1).
**Alternatives**: `#[bench]` nightly (rejected — stable 1.97 toolchain), divan (criterion chosen — ecosystem standard).
**Consequence**: AC-10.
**Evidence**: doc.rust-lang.org/cargo/commands/cargo-bench.html (L1), docs.rs/criterion (L1), accessed 2026-08-18

### DEC-TEST-0012: Baseline preservation (613/0/1, clippy 0, rustfmt clean)
**Status**: APPROVED 2026-08-18 (TEST-001 DESIGN)
**Node**: TEST-001
**Summary**: All infra additions (config/fixtures/harness/workflows) must preserve the existing workspace quality bar: **613/0/1**, clippy `--workspace --all-features --tests` 0, rustfmt clean, proptest feature suites, fuzz 44M+ clean. Mutants requires deterministic (non-flaky) suite (nextest retries off in mutants profile).
**Context**: Existing evidence iter 122-124; ACCEPTANCE_POLICY universal criteria.
**Alternatives**: lower the bar (rejected).
**Consequence**: AC-11.
**Evidence**: TEST-001_DISCOVER.md (iter 123, L4-own)

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

### DEC-BLE-0001: BLE v1 = GATT point-to-point + advertise-parse discovery
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: v1 BLE transport = GATT point-to-point for payload exchange + Extended/Periodic-advertising as a discovery beacon (advertise-parse; connect for bulk). Connectionless channel is for discovery only, never payload. Matches existing scaffold + INTERNET-001 reference pattern.
**Context**: RES-0019 R1 + verdict PROCEED — connectionless discovery + GATT connection is the only universally-available Android model; RES-0007 reconfirmed; no 2026 SOTA forces a pivot.
**Alternatives**: Mesh 1.1 / PAwR / BIS as v1 transport — rejected (see DEC-BLE-0002..0004).
**Consequence**: BLE stays a short-range unicast/broadcast-discovery transport in the IRIS Transport trait shape; LoRa/satellite gateways maintain long-range.
**Evidence**: RES-0019 R1; RES-0007; developer.android.com BLE advertising/scanning docs (L1); scaffold INTERNET-001 (L4)

### DEC-BLE-0002: BLE Mesh 1.1 REJECTED for v1 (v2 candidate)
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: No BLE Mesh in v1. No native AOSP mesh API through 2026 (official docs absence); app-grade mesh requires the nRF Mesh/SIG library. Mesh is a distinct protocol stack with its own security model (Network/App keys, provisioning) — not the IRIS Transport trait shape.
**Context**: RES-0019 R2 — Mesh 1.1 features (RPR, DFU, CBP, Directed Forwarding, Private Beacons) ready for embedded, not Android. May/June 2026 native-mesh-API claim contradicted by official docs + same author's field report (L5, Gap G3).
**Alternatives**: nRF Mesh library now — rejected (provisioning fragility, background limitations, stack commitment for marginal v1 gain).
**Consequence**: Recorded as v2 candidate with trigger (SIG-stack/fixed-cluster adoption); mesh flooding stays in IRIS application layer.
**Evidence**: RES-0019 R2/Q1.4; developer.android.com absence of mesh package (L1); nRF Mesh (L4); Medium 2026 contradiction (L5, G3)

### DEC-BLE-0003: PAwR REJECTED for v1 (v2 candidate)
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: No PAwR (BLE 5.4 Periodic Advertising with Responses) in v1. Star topology built for Electronic Shelf Labels; Android phones can sync/receive a PAwR train but there is NO API to transmit in response slots (Nordic: "not built to be used with smartphones… designed only for ESL"). No relay/multi-hop/throughput limits.
**Context**: RES-0019 R3 — phone-as-PAwR-responder appears impossible (G1); multi-advertiser networks undefined by spec; Android-side = monitor only.
**Alternatives**: use PAwR now — rejected (phone cannot respond).
**Consequence**: v2 candidate for stationary IRIS gateways with embedded controller radios (LoRa/satellite gateway clusters).
**Evidence**: RES-0019 R3/Q1.2; Bluetooth Core 5.4 (L1); Nordic DevZone + Infineon AN + Zephyr samples (L3-5); DevZone phone-role answer

### DEC-BLE-0004: Connectionless isochronous (BIS) REJECTED for v1
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: No BIS broadcast in v1. One-way only (no uplink), requires LE Audio stack + Security Mode 3 key distribution, poor fit for IRIS bidirectional multi-hop exchange. Watchlist only.
**Context**: RES-0019 R4 — BIS/CIS is LE-Audio/Auracast-centric, not a general bidirectional message bus.
**Alternatives**: — 
**Consequence**: Recorded on watchlist; revisit only if an IRIS broadcast-announce feature (v2) demands it.
**Evidence**: RES-0019 R4; Bluetooth SIG isochronous FAQ (L1); novelbits/cloud2gnd (L3)

### DEC-BLE-0005: MTU negotiate-late, <=512 B ATT payloads
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: Android 14+ negotiates ATT MTU 517 on the FIRST requestMtu per ACL and disregards subsequent requests on that ACL. Design payload segmentation <= min(mtu_effective-5, 512); treat MTU as negotiate-late (start at 23, resize on onMtuChanged); defensive re-connect on early requestMtu error (G5 wedge).
**Context**: RES-0019 R1/G5 — Android 14 behavior changes (L1); community wedge report (L5); RES-0007/EMERG-001 512-B ceiling alignment.
**Alternatives**: assume 512 up front — rejected (breaks 23-byte default peers; wedge risk).
**Consequence**: AttSegmenter module in BLE transport; INTERNET-001 framing/backoff pattern transfers.
**Evidence**: RES-0019 R1/G5; developer.android.com BluetoothGatt + Android 14 behavior changes (L1); SO 77602246/77748472 (L5)

### DEC-BLE-0006: App-layer envelope = trust anchor; OS patch-floor documented
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: IRIS message security NEVER depends on BLE pairing/bonding or link-layer encryption. App-layer envelope (CRYPTO-001 sign+encrypt, verify-before-forward, SEC-001 gates) is the trust anchor. Defensive advertise-parse + no unauthenticated-trigger actions. Document required minimum Android security-patch level (BLE stack CVEs are unpatched-by-app).
**Context**: RES-0019 R5 — CVE-2024-43770/2025-0074/2025-48539/2025-22406/2025-44557 in stack cannot be app-patched; NCC link-layer relay ≤8ms defeats latency-bounding + L2-encryption for proximity claims.
**Alternatives**: rely on LESC/pairing — rejected (app-layer is stronger + uniform).
**Consequence**: BLE proximity never a security primitive; relay residual = topology distortion only (SEC-001/ROUTE-002 bounded). Channel Sounding deferred (DEC-BLE-0008).
**Evidence**: RES-0019 R5/Q4/Q5; NVD/Android bulletins (L1); NCC Group advisory (L2/3); MIT 6.5610 BLE report (L3)

### DEC-BLE-0007: DLEP deferred as routing-metric interface (RFC 8175)
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: DLEP is NOT a v1 BLE transport. DLEP = control protocol between a router and an attached radio modem (radio-aware routing), not a phone transport. No router/modem split on Android phones; BLE metrics already app-visible. Deferred as a routing-metric interface for future gateway hardware. Spec = RFC 8175 (+ RFC 8703/8757); RFC 6841 is not the DLEP spec.
**Context**: RES-0019 R8/Q2 — LAN radio-aware routing for LoRa/satellite gateways on Linux routers; BLE duty-cycled mesh has no DLEP home.
**Alternatives**: DLEP now — rejected (no router/modem split; redundant with existing BLE metrics).
**Consequence**: Future LoRa/satellite relay gateways may surface metrics into ROUTE-001/002 via a DLEP-shaped interface; recorded for GW/LORA/SAT nodes.
**Evidence**: RES-0019 R8/Q2; RFC 8175/8703/8757 (L1); Cisco Radio Aware Routing whitepaper (L3)

### DEC-BLE-0008: Channel Sounding deferred to hardware-gated node
**Status**: APPROVED 2026-08-16 (BLE-001 DESIGN)
**Node**: BLE-001
**Summary**: Bluetooth 6.0 Channel Sounding (secure ranging 0.3-1 m, attack detection) is the standards-path mitigation for relay/proximity abuse — record as HARDWARE-gated future node; phones with CS silicon only ~2026-2027. Not v1.
**Context**: RES-0019 R5/R9 — relay cannot be defeated at BLE link layer in software; CS = range-primitive for distance-authorization (future use cases).
**Alternatives**: RTT/distance bounding — already rejected in RES-0018 (no-GPS requirement).
**Consequence**: BLE-001 design does not depend on CS; if IRIS later needs proximity authority, CS becomes a gated node.
**Evidence**: RES-0019 R5/R9; Bluetooth 6.0 CS test suite (L1); NewTec/u-blox (L3); NCC relay advisory (L2/3)

---

### DEC-WA-0001: v1 = publish/subscribe NAN discovery + NDP IPv6 socket data path (INTERNET-001 framing)
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: Wi-Fi Aware v1 = publish/subscribe NAN discovery (signed IRIS beacon carried as service_name + service_specific_info/match_filter; unsolicited+PASSIVE default, solicited+ACTIVE emergency) + NDP (NAN Data Path) IPv6 socket data path reusing INTERNET-001 TCP framing (1 MiB cap, pool + backoff). NDP = kernel-managed network interface → standard sockets, exactly the INTERNET-001 shape.
**Context**: RES-0020 R1/R4 — NAN alive API 26→36 (not deprecated) + Android 17 new architecture; NDP is a network interface (no app-level data API needed beyond sockets).
**Alternatives**: vendor CotS NAN SDK / proprietary TCP stack — rejected (INTERNET-001 framing transfers, one code path).
**Consequence**: WifiAwareAdapter trait + WifiAwareTransport + SimulatedWifiAwareAdapter in crates/iris-core; Android Kotlin adapter under ANDROID-001.
**Evidence**: RES-0020 R1/R4; developer.android.com Wi-Fi Aware/NDP docs (L1); INTERNET-001 verification (27/27)

### DEC-WA-0002: NDP open at L2 in v1; app-layer envelope = trust anchor
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: v1 NDP uses OPEN data path — no setPskPassphrase / NCS cipher-suite / certificates. App-layer envelope crypto (CRYPTO-001 sign+encrypt, verify-before-forward, SEC-001 gates) is the sole trust anchor. NCS SK/PK/PASN (API 30+) + Aware Pairing (API 34+, WFA 4.0) = optional hardening for v2.
**Context**: RES-0020 R2 — NDP security types are optional by spec; app-layer is stronger + uniform across transports (matches DEC-BLE-0006).
**Alternatives**: mandate PSK/NCS from v1 — rejected (key-distribution burden, no uniform gain over app-layer envelope).
**Consequence**: security postured entirely at app layer; optional v2 hardening triggers recorded.
**Evidence**: RES-0020 R2/AP-3; Wi-Fi Alliance NAN/MCL spec 2.0/4.0 (L1); developer.android.com NDP security (L1)

### DEC-WA-0003: 6 GHz / Wi-Fi 6E NAN not a v1 requirement (G-WA-3)
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: 6 GHz / Wi-Fi 6E NAN = opportunity-only. Design floor = 2.4 GHz (ch 6) + 5 GHz (ch 44/149) NAN, license-exempt ISM/U-NII in India. 6E is hardware + region-gated (India U-NII-5..7 band rules vary); enable later when both allow.
**Context**: RES-0020 R3/G-WA-3 — 6E support tiered by chipset + region certification.
**Alternatives**: require 6E for v1 — rejected (excludes most v1 hardware + regulatory risk).
**Consequence**: no 6E dependency; runtime-checked, degradation-defined.
**Evidence**: RES-0020 R3/G-WA-3; Realtek Wi-Fi 6E NAN tiering (L1); WFA 6 GHz certification notes (L1)

### DEC-WA-0004: FGS connectedDevice (API 34+) required for production discovery; Suspend/Resume power lever
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: Production background discovery requires a foreground service of type connectedDevice (API 34+). The dated "API 10+ discovery survives without FGS" claim is corrected — Android 12+ background limits + Android 15/16 FGS runtime-quota tightening make FGS mandatory. Suspend/Resume (API 34+, HAL-gated) = background cadence power lever; fall back to attach/discovery teardown + re-attach where unsupported.
**Context**: RES-0020 R5 — official FGS guidance + API-level changes through Android 16; Android 17 planning input.
**Alternatives**: rely on no-FGS discovery — rejected (Android 12+/15/16 enforcement breaks it).
**Consequence**: FGS declarations + user-visible notification in the Android deployment contract (AC-10).
**Evidence**: RES-0020 R5; developer.android.com foreground-service types + Wi-Fi Aware background (L1)

### DEC-WA-0005: runtime capability gate mandatory — FEATURE_WIFI_AWARE + isAvailable() + ACTION_WIFI_AWARE_STATE_CHANGED
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: Never assume Wi-Fi Aware from API level. Gate = hasSystemFeature(FEATURE_WIFI_AWARE) + WifiAwareManager.isAvailable + ACTION_WIFI_AWARE_STATE_CHANGED receiver; re-attach on true, teardown + degrade to BLE/Wi-Fi Direct on false. Single-radio coexistence (R7) surfaces as availability churn = degrade, not fail.
**Context**: RES-0020 R1/R7 — OEM firmware-gated availability; single-radio WiFi/RF front-end sharing.
**Alternatives**: assume availability on flagship APIs — rejected (OEM variance; state toggles at runtime).
**Consequence**: WifiAwareAdapter exposes is_available + state-change callback; TransportManager handles degrade routing.
**Evidence**: RES-0020 R1/R7; developer.android.com WifiAwareManager + Wi-Fi Aware availability (L1)

### DEC-WA-0006: discovery defaults unsolicited publish + PASSIVE subscribe; solicited + ACTIVE emergency
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: Default discovery = unsolicited publish (beacon) + passive subscribe (listener) for battery. Emergency mode switches to solicited publish + ACTIVE subscribe for faster discovery (WIFI_AWARE.md convention retained; AC-2).
**Context**: WIFI_AWARE.md §Publish/Subscribe; RES-0020 R1 — discovery types are app-chosen on Android.
**Alternatives**: always solicited/ACTIVE — rejected (battery + radio overhead for routine discovery).
**Consequence**: em-level switch logic in WifiAwareTransport; adapter exposes both publish/subscribe types.
**Evidence**: WIFI_AWARE.md (existing design); developer.android.com PublishConfig/SubscribeConfig (L1)

### DEC-WA-0007: peer identity by public-key fingerprint, never NAN MAC/PeerHandle; no unauthenticated triggers
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: NAN MAC/PeerHandle are ephemeral (platform MAC randomization ~30 min, factory MAC never on-wire; IdentityChangedListener events) and NOT trusted identities. IRIS binds peers by public-key fingerprint from the signed advertisement. Discovery + NDP setup produce candidate/connection objects only; every IRIS payload passes envelope verify + engine gate. No peer-triggered state change.
**Context**: RES-0020 R8/G-WA-2 — NAN privacy randomization built-in; Android Wi-Fi stack CVEs OS-patch-gated; SEC-001/IDENT-001 no-unauthenticated-trigger doctrine.
**Alternatives**: map PeerHandle→PeerId persistently — rejected (rotation breaks mapping; candidate-only is safe).
**Consequence**: candidate registry keyed by fingerprint; IdentityChangedListener handled in adapter; AC-6/AC-7 adversarial + security ACs.
**Evidence**: RES-0020 R8; developer.android.com IdentityChangedListener + NAN privacy (L1); SEC-001/IDENT-001

### DEC-WA-0008: CONFLICT-1 — Apple WiFiAware (iOS 26+) recognized in docs; BLE-002 = v1 iOS path
**Status**: APPROVED 2026-08-16 (WIFIAWARE-001 DESIGN)
**Node**: WIFIAWARE-001
**Summary**: Apple introduced WiFiAware framework (iOS 26+, iPhone 12+; 2.4/5/6 GHz, ~15 MB/s measured) — CONFLICT-1 corrected the 'iOS NO' rows in WIFI_AWARE.md + TRANSPORT_ABSTRACTION. IRIS v1 keeps WIFIAWARE-001 Android-only in code (caps supports_background_ios: false); BLE-002 = v1 iOS P2P path; Android↔Apple NDP interop recorded immature (vendors/ecosystem fragmentation) — cross-OS NDP validation deferred to platform nodes (BLK-0005). 
**Context**: RES-0020 R9/CONFLICT-1 — iOS capability changed 2026; Apple patent/API specifics still un-previewed.
**Alternatives**: add iOS WiFiAware to v1 WIFIAWARE — rejected (native Swift adapter = IOS-001 scope; interop unvalidated).
**Consequence**: doc reconciliation done now (AC-12); iOS WiFiAware = roadmap item for IOS-001.
**Evidence**: RES-0020 R9/AP-8; developer.apple.com WiFiAware documentation (L1); Wi-Fi Alliance (L1)

### DEC-WD-0001: Wi-Fi Direct = data plane on BLE control plane; DNS-SD discovery + TCP-over-GO data path
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: v1 shape (RES-0021 Q1/Q4/Q5/Q7 verdict PROCEED): BLE advertises/discovers in background (BLE-001 = always-on control plane); Wi-Fi Direct activates for payloads > 10 KB — DNS-SD (Bonjour) service discovery on a BLE-triggered re-arm window, createGroup/connect persistent GO, data plane = TCP socket over the GO reusing INTERNET-001 framing (4-byte length prefix + message, 1 MiB cap, pool + backoff).
**Context**: RES-0021 Q1/Q4 — WifiP2pManager fully supported + extended (API 36/37, no deprecation); DNS-SD preferred over device-name filtering; framework P2P find window 120 s.
**Alternatives**: device-name-filter discovery — rejected (RES-0021 Q4 DNS-SD preferred); Wi-Fi Direct as always-on discovery layer — rejected (background unreliable, BLE owns discovery).
**Consequence**: WifiDirectTransport + WifiDirectAdapter FFI trait + TCP framing reuse; AC-1/AC-2/AC-4; WIFI_DIRECT.md 4-phase workflow retained.
**Evidence**: RES-0021 Q1/Q4/Q5/Q7; docs/transports/WIFI_DIRECT.md (L1-L5 primary sources)

### DEC-WD-0002: group security = WPA2-Personal (PSK/AES-CCMP) floor; WPA3-SAE capability-gated later phase
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: v1 group security = WPA2-Personal (AES-CCMP). WPA3-SAE is now spec-supported on Wi-Fi Direct R2-capable devices (WifiP2pGroup SECURITY_TYPE_WPA3_SAE/WPA3_COMPATIBILITY API 36, PCC modes) — capability-gated later phase via isWiFiDirectR2Supported/isPccModeSupported. OWE = infra-only, not P2P.
**Context**: RES-0021 Q2 — group default remains WPA2 on most deployments; SAE needs v3.5 PMF requirements.
**Alternatives**: WPA3-SAE v1 default — rejected (device/vendor variance, API 36 floor); WPA2-only forever — rejected (SAE = roadmap hardening).
**Consequence**: AC-6/AC-7 security posture + AC-14 patch floor; isWiFiDirectR2Supported probes added later phase; WIFI_DIRECT.md WPA3 note.
**Evidence**: RES-0021 Q2; developer.android.com WifiP2pGroup security types API 36 (L1)

### DEC-WD-0003: WPS-PIN prohibited; passphrase pushed over authenticated BLE control plane; PBC legacy fallback only
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: WPS deprecated client-mode (API 28); WPS-PIN is a documented attack target (CVE-2021-0326 adjacent unauthenticated P2P RCE requires only an active P2P search). IRIS never uses WPS-PIN; the initiating peer pushes the group passphrase over the authenticated BLE control plane (R2-OOB-inspired); PBC = legacy dev-doc fallback only. NFC OOB (P2P spec §3.1.2.7) not exposed by Android API.
**Context**: RES-0021 Q6/Q8 — R2 pairing bootstrapping API 36 (WifiP2pPairingBootstrappingConfig incl. OUT_OF_BAND over BLE) is the modern path but BLE control plane already exists in IRIS.
**Alternatives**: WPS-PIN auto-config — rejected (attack surface, deprecation); R2 OOB bootstrap in v1 — deferred (API 36 floor; BLE control plane covers the same need).
**Consequence**: passphrase exchange via BLE control-plane message; AC-7; dev-doc sample (PBC) documented as legacy-only; AC-14 patch floor records CVE-2021-0326.
**Evidence**: RES-0021 Q6/Q8; developer.android.com WPS deprecation (L1); NVD CVE-2021-0326 (L5)

### DEC-WD-0004: discovery strategy — 30-s app re-arm cadence vs 120-s single find; DISCOVERY_CHANGED → BLE fallback
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: Framework P2P find window = 120 s (DISCOVER_TIMEOUT_S); WIFI_DIRECT.md's 30 s = app-level re-arm cadence (RES-0021 Q4/G-WD-7 correction). v1 uses a 30-s app-controlled window with explicit stop, or a single 120-s find with stop/restart; WIFI_P2P_DISCOVERY_CHANGED_ACTION stop event returns to BLE control-plane waiting. Best-effort on OEM builds → BLE remains the durable trigger.
**Context**: RES-0021 Q4 — discovery best-effort; battery 80-150 mA scan budget from WIFI_DIRECT.md.
**Alternatives**: continuous discovery — rejected (battery); BLE-only discovery — rejected (WD data plane needs the group anyway).
**Consequence**: AC-5 discovery re-arm + availability refresh test; WIFI_DIRECT.md 30-s wording corrected.
**Evidence**: RES-0021 Q4; developer.android.com discoverServices + DISCOVER_TIMEOUT_S (L1)

### DEC-WD-0005: persistent GO + band policy — setGroupOperatingBand (API 29) 5 GHz preferred / AUTO fallback
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: createGroup() forms an autonomous persistent GO (until removeGroup) reinvokable via p2p_invite-equivalent; setGroupOperatingBand = **API 29** (corrects WIFI_DIRECT.md's "API 30+"), AUTO/2.4/5/6 GHz — v1 uses 5 GHz preferred with AUTO fallback; band-constrained GO creation FAILS → fall back to AUTO / 2.4 GHz / degrade to BLE. MAC randomization is platform-inherited and tied to persistent-group presence — identity never depends on P2P MAC.
**Context**: RES-0021 Q5 — client IPv4 DHCP or IPv6-link-local (GROUP_CLIENT_IP_PROVISIONING_MODE_IPV6_LINK_LOCAL); group presence = state to re-establish, not assumed persistent (G-WD-8).
**Alternatives**: ephemeral GO per transfer — rejected (persistent GO = WIFI_DIRECT.md optimization for frequent peers); hard-coded GO IP — rejected (adapter-supplied go_addr, G-WD-2).
**Consequence**: AC-9 persistent-GO teardown/re-establish; AC-10 band API 29 doc correction; GO intent bias 14/7/3 retained.
**Evidence**: RES-0021 Q5; developer.android.com createGroup/setGroupOperatingBand API 29 (L1)

### DEC-WD-0006: client-capacity model — N-client admission + GO-side connection table; no hard-coded-8; GO intent 14/7/3
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: WFA mandates 1:1 P2P, one-to-many optional — "1 GO + 8 clients" = vendor/HAL ceiling, NOT a spec guarantee (RES-0021 Q3/G-WD-1). IRIS architects an N-client admission model: bounded GO-side connection table (per-client connection/endpoint slots), admitted client queue, no unbounded churn. STA+P2P concurrency is HAL-combination-dependent → capability-probed, never assumed. GO election intent bias (fixed infra 14 / battery 7 / low 3) retained.
**Context**: RES-0021 Q3 — group client count is platform/HAL-dependent.
**Alternatives**: hard-code 8-client table — rejected (vendor ceiling assumptions); unbounded connection table — rejected (capacity).
**Consequence**: AC-9 bounded GO connection table; AC-3 coexistence gate.
**Evidence**: RES-0021 Q3; Wi-Fi Alliance P2P Technical Specification v1.9 (L1)

### DEC-WD-0007: dual-platform adapter — Android WifiP2pManager + Linux wpa_supplicant via one trait; IRIS IP/DHCP glue
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: One WifiDirectAdapter trait surfaces both Android (WifiP2pManager + DNS-SD + FGS/wakelock) and Linux (wpa_supplicant: p2p_group_add/p2p_connect/p2p_service_add bonjour/p2p_invite/p2p_get_passphrase, group iface p2p-wlan0-N). IP/DHCP is NOT automatic parity — IRIS Linux nodes own GO static address + dnsmasq/udhcpd glue; client IPv4 DHCP or IPv6-link-local. GO address is adapter-supplied (requestGroupInfo/requestConnectionInfo), never hard-coded (verify Android GO 192.168.49.1 in AOSP at IMPLEMENT, G-WD-2).
**Context**: RES-0021 Q7 — wpa_supplicant full P2P surface; Android GO IP practice-stable but not contract.
**Alternatives**: Android-only adapter — rejected (DESKTOP-001 Linux nodes need the transport); hard-coded 192.168.49.1 — rejected (G-WD-2).
**Consequence**: AC-11 FFI conformance type-checked (SimulatedWifiDirectAdapter); ANDROID-001 + DESKTOP-001 Linux adapter scope; AC-4 E2E via sim.
**Evidence**: RES-0021 Q7; w1.fi wpa_supplicant P2P docs (L1); AOSP WifiP2pServiceImpl (L2)

### DEC-WD-0008: OS patch floor AC — Android SPL ≥ 2021-02, wpa_supplicant ≥ 2.12; single-radio coexistence = runtime constraint
**Status**: APPROVED 2026-08-17 (WIFIDIRECT-001 DESIGN)
**Node**: WIFIDIRECT-001
**Summary**: Wi-Fi stack CVEs are OS-patch-gated, never app-patchable (RES-0021 Q8): AC-14 records a deployment floor — Android SPL ≥ 2021-02 (CVE-2021-0326 P2P RCE), Linux wpa_supplicant ≥ 2.12 (w1.fi 2026-1 unauth mgmt-frame memory corruption, 2026-3 SAE NULL-deref), kernel with 2024-26 Wi-Fi driver fixes (CVE-2024-26895/27053/47712/47724/56539/46755; CVE-2025-40321; CVE-2026-31780/46069). Single-radio STA+P2P coexistence surfaces as runtime-availability churn → degrade to BLE, never fail (RES-0021 Q1/Q3; WifiAvailableChannel API 34 OP_MODE probes).
**Context**: RES-0021 Q1/Q3/Q8 + G-WD-1/G-WD-6 — OEM/HAL variance; Android 12+ coex channel-avoidance COEX_RESTRICTION_WIFI_DIRECT.
**Alternatives**: rely on WPA2 alone — rejected (L2 is not the trust anchor); assume STA+P2P concurrency — rejected (HAL-dependent).
**Consequence**: AC-3 coexistence gate + AC-14 recorded patch floor; WIFI_DIRECT.md known-issue notes carried.
**Evidence**: RES-0021 Q1/Q3/Q8; developer.android.com WifiAvailableChannel + coex restrictions (L1); osv.dev/NVD (L5)

---

## ANDROID-001 DECISIONS (iter 112 DESIGN, RES-0022)

### DEC-AND-0001: UniFFI foreign traits + async trait methods over FFI = Kotlin-adapter injection bridge
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: Kotlin-implemented platform adapters inject into Rust via UniFFI **foreign traits** — `#[uniffi::export(foreign)]` proc-macro (callback interfaces soft-deprecated, retained for UDL legacy). BleAdapter → sync foreign-trait methods returning `Result`; WifiAwareAdapter (12-op) + WifiDirectAdapter (20-op) → **async trait methods translated to foreign `suspend` fns** via UniFFI's foreign-future/oneshot callback mechanism (Rust piggybacks the Kotlin coroutine runtime). Constraint: methods must return `Result` with a compatible error type and pass params by value (references unsupported, issue #2263) — existing adapter traits already comply.
**Context**: RES-0022 Q1, G-AND-3.
**Alternatives**: hand-written JNI (rejected — no bindings for the 3 async traits, high risk); callback interfaces (soft-deprecated); UniFFI 0.32 (docs.rs, unverified — G-AND-1 pin 0.31.x).
**Consequence**: AC-2/AC-3 (G-AND-3 spike at IMPLEMENT); single FFI error enum; generated Kotlin committed.
**Evidence**: mozilla.github.io/uniffi foreign_traits + async-overview (L1); issue #2263 (L3)

### DEC-AND-0002: Explicit tokio runtime handle injection (issue #2576 workaround)
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: `#[uniffi::export(async_runtime="tokio")]` is **ineffective** on structs implementing exported traits' async methods (trait-method bridging does not route through async-compat). Every async poller thread receives an explicit `tokio::runtime::Handle` at construction; adapter polling awaits run through it. The G-AND-3 spike proves this path against the existing `#[async_trait]` adapter definitions before IMPLEMENT proceeds.
**Context**: RES-0022 Q1 (issue #2576).
**Alternatives**: rely on the attribute (broken); spawn per-call runtimes (rejected — overhead + panics).
**Consequence**: D-2 in ANDROID_DESIGN.md; constructor signature carries the Handle.
**Evidence**: github.com/mozilla/uniffi-rs/issues/2576 (L3/L5)

### DEC-AND-0003: Toolchain pinned — UniFFI 0.31.x + cargo-ndk 4.1.2 + rust-android-gradle (0.9.6 | Mullvad 0.10.1)
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: UniFFI **0.31.x** (bindgen + proc-macro; 0.32 on docs.rs — re-check at upgrade, G-AND-1). cargo-ndk **4.1.2** (MSRV Rust 1.86) producing `libiriscode.so` for **{arm64-v8a (Play 64-bit mandatory), armeabi-v7a, x86_64 (emulator)}** (i686 dropped — G-AND-6). rust-android-gradle **0.9.6** for Gradle 8.x **or Mullvad fork 0.10.1** for Gradle 9+ — the exact Gradle↔plugin pair is pinned at scaffold (G-AND-4). NDK pinned via `ANDROID_NDK_HOME` (r26 floor; SDK r28.2 current — G-AND-5). Generated Kotlin **committed** (uniffi-starter convention).
**Context**: RES-0022 Q2.
**Alternatives**: manual JNI (rejected); rustjni / MatrixDev plugins (younger, not preferred).
**Consequence**: AC-1/AC-2; CI Gradle build evidence.
**Evidence**: crates.io cargo-ndk 4.1.2 (L3); plugins.gradle.org 0.9.6/0.10.1 (L1); uniffi-starter (L4)

### DEC-AND-0004: Kotlin 2.2.x + Compose + Hilt MVVM app shell
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: Kotlin **2.2.x** (2.1+ strong-skipping compiler default) + Compose BOM **≥ 2024.01.00** (ANDROID.md floor) + **Hilt** (official compile-time DI; Koin = KMP alternative) + MVVM/Clean Architecture per KOTLIN_LAYER.md + `collectAsStateWithLifecycle()`. Floors: targetSdk **34** / minSdk **26** (API 26 ≈ 95%+ device coverage). FGS **connectedDevice** (API 34+) + PendingIntent BLE scanning + WorkManager 15-min cadence + OEM battery-kill UX matrix.
**Context**: RES-0022 Q3; ANDROID.md; KOTLIN_LAYER.md.
**Alternatives**: Koin (KMP route, not needed v1); raw FGS types (wrong category).
**Consequence**: AC-6/AC-7.
**Evidence**: kotlinlang.org whatsnew22 (L1); developer.android.com Compose stability (L1)

### DEC-AND-0005: Identity = Keystore TEE Ed25519 (no ECDSA P-256 conversion)
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: ED25519 hardware-backed in Android Keystore **since Android 13** (KeyMint v2 HAL adds Curve25519 signing+agreement; keystore2 in Rust). Identity = **Keystore TEE Ed25519** (API 33 floor for hardware-backed; runtime software-key fallback on older/no-TEE devices per G-AND-2) + X25519 static advertisement — aligned IDENT-001 RED-0005 (Ed25519 signs X25519). Session/onion keys stay in Rust engine (ephemeral, no HW anchor). StrongBox subset **excludes Ed25519** (RSA/AES/P-256/HMAC-SHA256/3DES) → StrongBox reserved only for optional AES DEK / P-256 rescue path (not v1). ECDSA P-256 conversion **rejected** (hw-sign precedent orders ED25519 first).
**Context**: RES-0022 Q4; IDENT-001 RES-0016-D2/RED-0005; ANDROID.md §Security (dated claim corrected).
**Alternatives**: ECDSA P-256 conversion (rejected); StrongBox Ed25519 (rejected — CTS 399856239).
**Consequence**: AC-8; ANDROID.md §Security amendment at DOCUMENT.
**Evidence**: source.android.com keystore + issuetracker 356158095 (L1); issuetracker 399856239 (L1/L5); github.com/reitowo/hw-sign (L4)

### DEC-AND-0006: Android 17 ACCESS_LOCAL_NETWORK (LNP) cliff — v1 stays targetSdk 34
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: Android 17 introduces **`ACCESS_LOCAL_NETWORK`** (runtime, NEARBY_DEVICES group, no re-prompt if sibling granted) mandatory for targetSdk 37+; gates **outgoing/incoming TCP, UDP unicast/multicast/broadcast → the entire IRIS data plane** (TCP-over-GO, NDP IPv6 sockets, NAN UDP) + mDNS/DNS-SD. v1 **targetSdk 34** (implicit INTERNET grant, temporary bridge). SDK-37 cliff documented; any bump must declare + runtime-request LNP; `NsdManager`-mediated IPs exempt. Android 17 `BluetoothSocket.read()` **-1** on close (targetSdk 37) — Kotlin read loops check -1.
**Context**: RES-0022 Q5.
**Alternatives**: bump to 37 now (rejected — LNP plumbing + no transport benefit for v1).
**Consequence**: AC-9; ANDROID.md §permissions amendment.
**Evidence**: developer.android.com behavior-changes-17 + local-network-permission (L1); flutter/flutter#184859 (L4)

### DEC-AND-0007: Adapter lifecycle absorbs NEW-WA-RT-108..112 + WIFIDIRECT RT-010 in AdapterLifecycle.kt
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: Per-call FFI timeouts (RT-110, seconds-scale worst case), idempotent start/subscribe (RT-111 + RT-010 — no new DiscoverySession per subscribe, ensure_started not latched), ring-buffer outbox (RT-112 — bounded, per-destination eviction per wifi_direct pattern), verified-peer reuse key (RT-108 — candidate→VERIFIED at envelope resolution), closed-NDP prune + **multi-subscriber availability stream** (RT-109 — SharedFlow).
**Context**: WIFIAWARE-001/WIFIDIRECT-001 SECURITY_REVIEW carry-forwards.
**Alternatives**: adapter calls without timeouts (rejected — RT-110); one-shot availability (rejected — RT-109).
**Consequence**: AC-5; lifecycle unit tests.
**Evidence**: WIFIAWARE-001_VERIFICATION.md known_limitations (L4)

### DEC-AND-0008: crates/iris-android mirrors DesktopEngine (IrisEngine) with committed generated Kotlin
**Status**: APPROVED 2026-08-17 (ANDROID-001 DESIGN)
**Node**: ANDROID-001
**Summary**: `crates/iris-android` = UniFFI binding crate; `IrisEngine` mirrors `DesktopEngine` (engine_handle.rs): TransportManager + registers BLE/Wi-Fi Aware/Wi-Fi Direct transports (injected adapters) + MessageEngine over MemoryStorage (Android metadata-only Room per KOTLIN_LAYER.md; Postgres stays desktop/server) + auto inbox forwarder → process_incoming + `subscribe_inbox` broadcast Channel exposed to Kotlin. Protocol/routing/telemetry/storage all in iris-core; Kotlin is a thin adapter. ADR-0003 crates/iris-android plan honored (JNI label updated to UniFFI).
**Context**: DESKTOP-001 reference pattern; iter-110 evidence.
**Alternatives**: hand JNI wrapper around engine (rejected — DesktopEngine pattern proven).
**Consequence**: AC-4; Kotlin unit round-trip test.
**Evidence**: crates/iris-desktop/engine_handle.rs (L4)

---

## BLE-002 DECISIONS (iter 134 DESIGN, RES-0024)

### DEC-BLE-002-0001: Stable IRIS service UUID for iOS advertising — never per-session
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: iOS advertises ONE stable IRIS service UUID (never per-session): a background/foreground scanner needs the UUID in its `scanForPeripherals(withServices:)` filter (background requires a service-UUID filter); overflow-area hash matching requires a stable scanned-for UUID. The GATT service UUID is the iOS discovery anchor; the Android discovery beacon payload (service data) stays Android-only.
**Context**: RES-0024 RQ-5/DI-1 — iOS `startAdvertising(_:)` supports only local name + service UUIDs; no service data ever.
**Alternatives**: per-session/dynamic UUIDs — rejected (scanner filter cannot follow; overflow-area hash matching impossible); advertise service data — impossible on iOS (API-level).
**Consequence**: AC-3 connect-to-identify flow; `AdvertisementData.service_uuid` always `Some(IRIS_SERVICE_UUID)` on iOS adapter; AC-10 doc contract.
**Evidence**: RES-0024 RQ-5/DI-1; Apple CoreBluetooth `startAdvertising` docs (L1)

### DEC-BLE-002-0002: iOS discovery = connect-to-identify (GATT identification characteristic)
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: iOS discovery flow: foreground scan (IRIS service-UUID filter) → `connect_gatt` → read the identification characteristic → parse `DiscoveryBeacon`/capabilities → candidate peers (then normal envelope security). Shared `DiscoveryBeacon` build/parse core (`ble_advert.rs`) serves BOTH carriers: Android reads ad-carried service data; iOS reads the same bytes from the identification characteristic after connect. One parser, two carriers.
**Context**: RES-0024 RQ-5/DI-1 — codified ad asymmetry: no service data on iOS; Android can skip connect via ad data, iOS cannot.
**Alternatives**: try to pack beacon into iOS ads — impossible (API); scanner-side heuristics (name/RSSI) — rejected (unauthenticated, unreliable).
**Consequence**: AC-3; SimulatedBleAdapter gains `inject_identify_read`; AC-6 adversarial parse covers both carriers.
**Evidence**: RES-0024 RQ-5/DI-1; Apple `CBAdvertisementData` docs (L1)

### DEC-BLE-002-0003: `supports_background_ios` stays false; foreground-always + iOS 26 Live Activity screen-on framing
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: No Android-`PendingIntent` analog exists on iOS — no background-scan API, system relaunches for connection events only, user force-quit disables relaunch. `supports_background_ios: false` stays pinned (ble.rs:287/test :1075). iOS 26 Live Activity can keep a scan alive while the screen is on / Lock Screen visible (stops at screen sleep) — adopted as foreground-adjacent UX framing, NOT an OS background relaxation. Symmetric discovery requires foreground (or screen-on iOS).
**Context**: RES-0024 RQ-1/RQ-2/DI-3 — no relaxation through iOS 26; iOS 26 accessory background modes not applicable (DEC-BLE-002-0007).
**Alternatives**: claim background support — rejected (no API, would be fake); rely on iOS 26 accessory modes — rejected (AccessorySetupKit accessory-only).
**Consequence**: AC-10 background-contract documentation; iOS = limited relay node when backgrounded (RES-0019 R7).
**Evidence**: RES-0024 RQ-1/RQ-2/DI-3; Apple CoreBluetooth background execution docs (L1); Apple Developer Forums Live Activity reports (L5)

### DEC-BLE-002-0004: MTU = `maximumWriteValueLength` — negotiate-late 23→≤512 with 20-B degraded guard
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: iOS has NO MTU-request API — ATT MTU exchanges negotiated by the stack (up to 517); the app queries negotiated payload via `peripheral.maximumWriteValueLength(for:)` which is ATT payload (MTU−3) capped at 512. `BleAdapter::set_mtu` on iOS = no-op request returning the negotiated value. Segmenter: start 23-B default, resize on returned value, cap min(negotiated−3, 512). iOS 16.0/16.0.1 regression (185→77→20-B, fixed 16.1) → treat 20-B payload as degraded-but-functional, never loop.
**Context**: RES-0024 RQ-3/DI-5; RES-0007/EMERG-001 512-B ceiling; BLE-001 AC-2 negotiate-late precedent (Android `onMtuChanged`).
**Alternatives**: requestMTU-style path — impossible on iOS (no API); hard-code 512 — rejected (23-B peers would break).
**Consequence**: AC-2; `ble_att.rs` accepts externally-reported negotiated payload; AC-10 doc records 20-B guard.
**Evidence**: RES-0024 RQ-3/DI-5; Apple `maximumWriteValueLength` docs (L1); Stack Overflow iOS 16 MTU regression (L5)

### DEC-BLE-002-0005: State-restoration hardening — re-start in willRestoreState, system-kill vs user-force-quit, relaunch-loop debounce
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: State restoration does NOT auto-resume scanning/advertising — the app MUST re-call `startScan`/`startAdvertising` in `willRestoreState`; restoration identifiers stable across launches ("IrisCentralManager"/"IrisPeripheralManager"); both `bluetooth-central` AND `bluetooth-peripheral` UIBackgroundModes required for manager restoration. Relaunch classification: system-kill relaunch allowed; user force-quit = never relaunched. Guard relaunch-crash loops (restoration-ID mismatch / restart-while-scanning) with debounce/timeout; never initialize BLE state outside a valid launch path.
**Context**: RES-0024 RQ-4/DI-6 — restoration works for scanned/connected peripherals or scanner with identifiers; iOS 26 accessory categories restore pairing state without app code (not IRIS).
**Alternatives**: assume auto-resume — rejected (docs confirm re-start is app responsibility); no launch-path guard — rejected (crash loops).
**Consequence**: AC-8 lifecycle + AC-10 doc contract; iOS adapter restoration seam under IOS-001.
**Evidence**: RES-0024 DI-6; Apple `willRestoreState` + restoration docs (L1)

### DEC-BLE-002-0006: CVE posture — verified iOS set = OS-patchable only; misattributed IDs excluded; iOS patch floor documented
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: Verified iOS Bluetooth-stack CVEs (CVE-2023-42941, CVE-2024-23241, CVE-2024-44124, CVE-2024-44191) are OS-patchable only — no in-app mitigation exists; app-layer AEAD envelope (CRYPTO-001) + verify-before-forward + no-unauthenticated-trigger = the only in-app controls (defense-in-depth). Misattributed IDs EXCLUDED from the threat model: CVE-2023-28412 (Snap One OvrC, not iOS), CVE-2024-44270 (macOS sandbox escape, not iOS Bluetooth), CVE-2021-31714 (not in NVD). Deployment doc records a minimum iOS security-patch floor.
**Context**: RES-0024 RQ-4/DI-7 — prompt-CVE contradiction resolution; BLE-001 DEC-BLE-0006 precedent (Android app-layer = trust anchor).
**Alternatives**: list misattributed CVEs as IRIS-addressable — rejected (false threat model); rely on BLE link-layer — rejected (DEC-BLE-0006).
**Consequence**: AC-7 security + AC-10 patch-floor documentation; AC-15 SECURITY_REVIEW threat model uses verified set only.
**Evidence**: RES-0024 RQ-4/DI-7; Apple security releases CVE pages (L1); NVD (L1)

### DEC-BLE-002-0007: iOS 26 accessory background modes REJECTED — AccessorySetupKit accessory-only, not IRIS
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: iOS 26's new `bluetooth-peripheral` background-mode categories ("Common Bluetooth HIDs" / "Bluetooth LE Custom Devices") are tied to AccessorySetupKit accessory pairing — IRIS is a peer-to-peer mesh app, not an accessory-pairing app, and does not qualify. These are NOT a general background relaxation. Roadmap: re-evaluate only if IRIS ships a paired-accessory form factor.
**Context**: RES-0024 RQ-1/RQ-2 — the only "new" iOS 26 BLE background surface; Live Activity = screen-on scan only (DEC-BLE-002-0003).
**Alternatives**: claim accessory category in Info.plist — rejected (App Store review failure risk, wrong semantics).
**Consequence**: AC-10 doc records iOS 26 accessory modes as not-for-IRIS; no code path depends on them.
**Evidence**: RES-0024 RQ-1/RQ-2; Apple iOS 26 Info.plist key docs (L1)

### DEC-BLE-002-0008: Cross-platform interop — Android ad-carried vs iOS connect-to-read share one DiscoveryBeacon parse core
**Status**: APPROVED 2026-08-18 (BLE-002 DESIGN)
**Node**: BLE-002
**Summary**: Cross-platform discovery vectors: Android advertises the beacon in service data (scanner skips connect); iOS advertises UUID only and serves the same beacon bytes from the identification characteristic (scanner connects). The shared `DiscoveryBeacon` build/parse core (`ble_advert.rs`) is the single source of truth for both carriers. Overlapping reliance on the iOS overflow area is NOT designed (iOS-internal, reverse-engineered format, G4). Backgrounded-iOS-hidden-from-Android asymmetry is accepted (RES-0019 R7); foreground Android/Linux/Windows gateways carry relay-heavy work.
**Context**: RES-0024 RQ-5/G4 — codified ad asymmetry; WIFI_AWARE-001 DEC-WA-0008 CONFLICT-1 (BLE-002 = v1 iOS path).
**Alternatives**: duplicate parse logic per platform — rejected (drift risk); design on overflow-area interop — rejected (G4, iOS-internal).
**Consequence**: AC-3/AC-6 both-carrier coverage; AC-12 doc reconciliation (BLE.md + TRANSPORT_ABSTRACTION).
**Evidence**: RES-0024 RQ-5/G4; BLE-001 `ble_advert.rs` (L4); RES-0019 R7 (L1/L5)

### DEC-BLE-002-0015: `BleAdapter::gatt_read` REQUIRED — connect-to-identify needs a GATT read op
**Status**: APPROVED 2026-08-18 (BLE-002 IMPLEMENT, iter ~135)
**Node**: BLE-002
**Summary**: The BLE-001 `BleAdapter` trait had no read operation; the iOS connect-to-identify discovery path (DEC-BLE-002-0002) could not be expressed. RESOLVED: add `fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError>` to the trait (device-scoped read = `peripheral.readValue(for:)` on iOS; `BluetoothGatt.readCharacteristic` on Android) and wire it into the existing `discover_peers` connect-to-read branch — payload from `IRIS_IDENTIFY_CHARACTERISTIC` parse via the shared `DiscoveryBeacon` core. `SimulatedBleAdapter` SERVES the characteristic: `gatt_read` of `IRIS_IDENTIFY_CHARACTERISTIC` returns its `identify_data`, any other characteristic returns `Err(DeviceNotFound)` (no `supports_identify_read` flag — SECURITY_REVIEW iter ~137 correction). Android `BleBridge` (iris-android) returns `DeviceNotFound` until the Android FFI exposes a read op (ANDROID-001 follow-up); Android discovery skips connect via ad-carried data, so the Android↔iOS identify-read vector is a recorded known_limitation carried into IOS-001.
**Context**: BLE-002 IMPLEMENT — AC-3 required both-carrier round-trip via `SimulatedBleAdapter`; design BLE_002_DESIGN.md §4 named `inject_identify_read`; implementation review showed serving reads on the sim is simpler (no mutable injection state, matches real GATT server semantics).
**Alternatives**: `inject_identify_read(&self, uuid, bytes)` setter — rejected (mutable interior state for test-only injection; the sim already has a beacon; serving is the honest GATT-server shape); no-op read on `SimulatedBleAdapter` — rejected (breaks AC-3 round-trip); add read only inside the discovery branch (no trait change) — rejected (violates BleAdapter = FFI seam; the real iOS/Android adapters must implement the read at the platform tier).
**Consequence**: BleAdapter trait grows 1 op; all 7 impls updated in-pass (SimulatedBleAdapter, RecordingBleAdapter, IosPayloadAdapter, TwoMtuAdapter, FailingWriteAdapter, FailingConnectAdapter, BleBridge); +5 iOS-leg tests (AC-1/2/3/4/6/10/12); discovery code path unchanged in shape, now calls `adapter.gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC)`.
**Evidence**: BLE-001 `ble.rs` (L4) trait surface; DEC-BLE-002-0002/0004; BLE-002 IMPLEMENT pass (workspace green, clippy 0, fmt 0)
