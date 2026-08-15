# EMERG-001 Emergency System — Design (v1.0)

**Node**: EMERG-001 (P0 SECURITY/safety) — SOS, emergency broadcast, P0-P3
priority routing, disaster mode.
**Status**: DESIGN — ratifies under review, feeds IMPLEMENT.
**Date**: 2026-08-15
**Dependencies (COMPLETE)**: MSG-001 (message engine), IDENT-001 (identity/
key mgmt/trust), ROUTE-001 (baseline routing), CRYPTO-001 (crypto provider,
signing), PROTO-001 (wire codec).
**Sources**: RES-0017 (§1-§8) · docs/emergency/EMERGENCY_ARCHITECTURE.md ·
EMERGENCY_BROADCAST.md · SOS.md · EMERGENCY_SCENARIOS.md · DISASTER_MODES.md ·
docs/safety/EMERGENCY_GOVERNANCE.md · docs/security/EMERGENCY_ABUSE.md ·
docs/product/EMERGENCY_UX.md · docs/protocol/PRIORITY_MODEL.md ·
engineering/ACCEPTANCE_POLICY.yaml.

---

## 1. Scope

Implement the IRIS emergency system as an `emergency` module in `iris-core`
that layers on the existing wire (envelope fields 1–18) and existing identity
(`verify_chain` RED-0008, `TrustStore`, `KeyAdvertisementV1`) and message-engine
(priority queue, ack, dedup, SCF interaction) seams.

Deliverables in this node:

- Emergency payload schemas + CBOR encode/decode (capital-CAP-inspired subset).
- Authority certificate model (SPKI-style key-anchored, offline) + root store +
  chain verification + revocation + geographic/functional scope + permissions.
- SOS message build/verify + rate limiting (3/hr + downgrade) + signed cancel.
- Verified-authority broadcast relay path (mandatory relay, anti-probing drop).
- Disaster-mode state machine + activation triggers + gossip propagation +
  deactivation, battery/storage/priority policies.
- Authority-metadata-only audit log (pseudonymous, content-free).
- TEST_MODE drill capability.
- Engine integration seams (send-path attach chain; receive-path verify +
  rate-limit + deliver marker).
- Document corrections C1/C3/C4/C5/C6 (below).
- Acceptance criteria AC-1..AC-14 (this doc) + evidence at VERIFY.

Non-goals (deferred with explicit owner):

- PROTO-001 v2: 16-byte sender collapse (RED-0010), signed encryption_hdr
  (RED-0003), new ContentType codes >16 (BroadcastCancel-as-own-type),
  multi-key rotation decrypt E2E (IDENT-001 RT-002).
- OS-level alert surfaces (WEA/Cell-Broadcast channels, Android FGS, iOS
  overlay) — surfaced as data + typed events for platform nodes
  (ANDROID-001 / IOS-001 / DESKTOP-001) to render.
- Physical-transport duty-cycle enforcement beyond existing WPC constraints
  (BLK-0005 device layer).
- Full fuzzing campaign (24h min per ACCEPTANCE_POLICY safety-critical): the
  structural/negative tests in this node + redteam review gate the checks;
  long-duration fuzz is a known_limitation carried to TEST-001 unless the
  adversarial coverage evidence is sufficient at VERIFY.

---

## 2. Wire reconciliation

### 2.1 What already exists (code-verified, Level 4)

| Seam | Location | Status |
|------|----------|--------|
| `ContentType::Sos = 2` | `protocol/content_type.rs` | EXISTS, maps to `P0` |
| `ContentType::Medical = 8` | same | EXISTS, maps to `P1` |
| `ContentType::Location = 3` | same | EXISTS, maps to `P2` |
| `ContentType::EmergencyAlert = 14` | same | EXISTS, maps to `P3` |
| `EMERGENCY_BROADCAST = [0x00,0,0,1]` | `protocol/mod.rs` | EXISTS (4-byte recipient) |
| `P0_MAX_ENVELOPE_BYTES = 255` | `protocol/envelope.rs` | EXISTS |
| Field 18 `auth_cert_chain: Option<Vec<Vec<u8>>>` | `envelope.rs` + `codec.rs` (encode 193-196, decode 511-519, fieldname 290) | EXISTS, unsigned extension |
| `broadcast_peer()` padded pseudo-peer | `message_engine/mod.rs:62` | EXISTS |
| `recipient_matches` / empty-recipient path | engine `deliver_or_relay` | EXISTS — P0 broadcast never encrypted |
| Fragment eligibility — `EmergencyAlert` + P0-P3 excluded | `message_engine/fragment.rs` | EXISTS |
| `identity::verify_chain` (RED-0008, cap 8) | `identity/chain.rs` | EXISTS |
| `ChainError::{UntrustedRoot,BrokenLink,Expired,...}` | same | EXISTS |
| `authorizing_cert` helper | `identity/chain.rs:180` | EXISTS (builds chains) |
| `KeyAdvertisementV1::build/verify` | `identity/advertise.rs` | EXISTS |
| `TrustStore::{is_trusted_root, revoke, entries}` | `identity/trust_store.rs` | EXISTS |
| `MessagePriority::is_emergency()` (P0/P1) | `message.rs` | EXISTS |

### 2.2 What must be added

1. `emergency/` module (payload schemas, authority, SOS, broadcast, rate-limit,
   mode, audit, drill) — wire-content only, no envelope field changes.
2. Payload CBOR content inside envelope field 13 for `ContentType::Sos` and
   `ContentType::EmergencyAlert` (compact, canonical CDE per RFC 8949 §4.2).
3. Engine integration seams (see §10): `set_emergency_provider` on
   `MessageEngine` (analogous to `set_key_directory`), emergency hooks in
   `send_message` / `process_incoming` / `deliver_or_relay`.
4. `EmergencyBroadcast`/`Sos` payload enums must not collide with existing
   content-type semantics; extend off-wire only.

### 2.3 Priority reconciliation (C5 + wire mapping)

- `ContentType::Sos` → **P0** (255 B envelope, LoRa-native). Payload ceiling
  **84 B** (DISC-0008: 237-B measured envelope, 64-B signature dominates).
- `ContentType::Medical` → **P1**, `ContentType::Location` → **P2** — unchanged.
- `ContentType::EmergencyAlert` → **P3** (existing wire mapping, 12 h TTL,
  1 KB budget; no LoRa — PRIORITY_MODEL: P3 not LoRa-eligible). The
  authority-broadcast *severity* field drives UI/takeover tiers; wire priority
  stays P3 for all EmergencyAlert (a critical alert cannot be P0 due to the
  255-B/84-B envelope and the chain+body size — documented decision,
  DEC-EMERG-0001 ⇒ C1). §4 budget correction (TEST, 2026-08-16): 2-element
  verified chain fits < 1 KB (measurement below); full 4-element chains run
  ~1.3 KB and ride ≥ 64 KB-class links (BLE 512 B limitation recorded).
- **No new ContentType codes in v1.** Cancel and disaster-mode markers ride
  payload subtypes (message_type) — avoids a wire-version gate (deferred to
  PROTO-001 v2 / RED-0010). BroadcastCancel is an `EmergencyAlert` body with
  `message_type := Cancel`; SOS-cancel is a `Sos` body with `kind := Cancel`.
- Status reporting: **OFF by default** (RFC 9171 §6.2). SOS delivery ack is an
  application message when a direct recipient exists; broadcasts get **no
  per-relay ACK** (anti-probing — DEC-EMERG-0004).

---

## 3. Authority certificate model (DEC-EMERG-0002, resolves C6)

Reuse **`KeyAdvertisementV1`** (the SPKI/RFC 9804 keyholder certificate) as the
chain element — do **not** introduce a second certificate struct. Field 18 is
`Vec<Vec<u8>>` = ordered encoded `KeyAdvertisementV1` elements; the
chain-sender binding is enforced by `identity::verify_chain` (final element's
`identity_pubkey == envelope.sender_id`).

```
Chain (field 18), order root→leaf:
  [0] NDMA root            (preloaded; TrustStore trusted-root)
  [1] SDMA state cert      (issued by root)
  [2] DDMA district cert   (issued by state)
  [3] local cert           (issued by district)   ← must equal sender_id
```

- Max depth 4 (National→State→District→Local; EMERGENCY_GOVERNANCE). Cap
  remains `MAX_CHAIN_LEN = 8` (identity); emergency layer enforces ≤ 4 via
  chain-length check before `verify_chain` (cheap pre-guard).
- Validity: 2y / 1y / 6mo / 3mo per governance (IRIS-specific — no external
  benchmark; justified by offline-revocation lag, RES-0017 G5).
- Root trust: `TrustStore::is_trusted_root(root identity_pubkey)` — preloaded
  NDMA root registered at provisioning (app/firmware update only).
- Revocation: signed `RevocationList`? **v1 = rotation/null-rotation path**
  (IDENT-001). Identity `rotate.rs::apply` + `TrustStore::revoke` run when a
  signed `KeyRotation` (type 16) revokes; emergency verifies each chain element
  is **not Revoked** (verify_chain rule 9 already enforces). A broadcast-scope
  revocation *list* message is carried as `EmergencyAlert` body
  `message_type := RevocationList` (P3) — propagates offline like any message
  (EMERGENCY_ABUSE "revocation propagates within minutes"; isolated-partition
  lag accepted as known_limitation, governance-documented).
- Geographic/functional scope + permissions are **payload-level**, not in the
  key advertisement: the *authority profile* (issuer hierarchy, scope, serial)
  lives in the `EmergencyBroadcast` header and is signed over a canonical body;
  relay enforces:
  1. `verify_chain(field18, trust, sender_id)` OK
  2. broadcast `area_code` ⊂ cert chain's claimed authority area
  3. broadcast `message_type` ⊂ cert's functional scope
  4. broadcast severity ≤ cert's `broadcast_max_severity`
  Any failure → **drop + log**, no reply (anti-probing).

> C6 resolution: governance doc header language changes from "X.509-inspired"
> to "SPKI-style key-anchored certificate, CBOR-serialized" (§12).

---

## 4. EmergencyBroadcast payload (DEC-EMERG-0001, resolves C1)

Capital-CAP (OASIS CAP v1.2) is the **semantic reference only** (XML; no
wire conformance). ITU-T X.1303bis (ASN.1/PER) rejected as encoding
(FAIL-0005). Wire = **minimal CAP-inspired CBOR subset** in envelope field 13,
CDE canonical.

```rust
pub struct EmergencyBroadcast {
    pub format_version: u8,          // 1
    pub broadcast_id: [u8; 16],      // UUID v4 (broadcast identity)
    pub republish_id: Option<[u8; 16]>, // parent broadcast_id for update/cancel linkage
    pub issued_at: u64,              // unix seconds
    pub expires_at: u64,             // unix seconds (must be < issued_at + 72h)
    pub severity: Severity,          // CRITICAL | SEVERE | MODERATE | MINOR | TEST
    pub certainty: Certainty,        // OBSERVED | LIKELY | POSSIBLE | UNLIKELY
    pub message_type: AlertMessageType, // ALERT | UPDATE | CANCEL | ACCEPT | DRILL | REVOCATION_LIST
    pub area_code: String,           // ISO 3166-2 ("IN-GJ" | "IN-GJ-19"); UTF-8, ≤ 8 chars
    pub language: String,            // BCP 47 ("en", "hi", "gu"); ≤ 5 chars
    pub headline: String,            // ≤ 96 chars (compact; fits 1KB envelope with chain)
    pub instructions: Option<String>, // ≤ 96 chars follow-up-URL-free compact
    pub authority: AuthorityMeta,    // issuer_name, functional scope, perm bits (see §5)
    pub drill: bool,                 // TEST_MODE marker (DEC-EMERG-0008)
}
```

Size geometry (P3, no fragmentation — Transport budget reconciled at TEST
stage, 2026-08-16, measured on the wire):
- chain elements measure **~195 B each** (KeyAdvertisementV1 = 64-B sig +
  2 × 32-B keys + counter/valid-until + CBOR), NOT the ~150 B estimate.
  Measured full envelope (header + 4-element chain + 96/96 text): **1277 B**;
  2-element chain: **690 B**.
- ⇒ v1 budget: **2-element chains fit the <1 KB target** (690 B); **full
  N→S→D→L 4-element chains = ~1.3 KB**, above the nominal 1 KB target but well
  within the P3 envelope class (transport `max_message_size`: simulated 64 KB,
  internet 1 MB, gateway 1 MB; BLE = 512 B).
- **known_limitation (recorded at TEST):** `EmergencyAlert` is never-fragmentable
  (fragment.rs), so verified alerts are unroutable over BLE-only links — even a
  2-element verified chain (690 B) exceeds BLE's 512 B. Radio-native emergency
  remains SOS (P0, 84 B, 255 B envelope). Full-depth authority chains therefore
  ride simulated/internet/gateway-class links only. No silent truncation: the
  build-side `EmergencyError::TooLarge` guard (design-fixed) is expressed as
  hard length caps on headline (96) / instructions (96) / issuer (64) /
  area (8) / language (5), enforced at build (see AC-1 test
  `oversized_payload_rejected_no_silent_truncation`).
- 96-char headline + 96-char instructions replaces the "≤ 2000-char
  instructions" doc claim (**C1**) — full detail publishes as a signed
  follow-up `EmergencyAlert` body (UPDATE) or a P3 `Text` when translated
  copy is required.

`Severity`/`Certainty`/`AlertMessageType` map to CAP semantics; ENUM values are
fixed wire integers (documented in the module).

---

## 5. SOS message payload (DEC-EMERG-0003)

`ContentType::Sos`, priority P0, **empty-recipient / `EMERGENCY_BROADCAST`**
broadcast path, **never encrypted** (no recipient key — existing engine
behavior). Envelope is Ed25519-signed (sender identity). Payload ≤ **84 B**.

```rust
pub struct SosMessage {
    pub format_version: u8,          // 1
    pub latitude: i32,               // degrees * 1_000_000 (i32, ~±180)
    pub longitude: i32,              // degrees * 1_000_000
    pub accuracy_m: u16,             // 0 = unknown
    pub location_source: LocationSource, // GPS | NETWORK | USER_MANUAL | NONE
    pub timestamp: u32,              // unix seconds (signed-envelope carrier clock)
    pub kind: SosKind,               // SOS | CANCEL | TEST (drill)
    pub original_message_id: Option<[u8; 16]>, // present when kind == CANCEL
    pub reason: Option<u8>,          // compact code: 0 inj 1 trap 2 med 3 fire 4 sar 5 other
}
```

- 84-B budget: lat(4)+lon(4)+acc(2)+src(1)+ts(4)+kind(1)+reason(1) + CBOR
  overhead ≈ 22-30 B ⇒ leaves room for a compact `note` (≤ 32 B) when needed.
- CANCEL (`kind=CANCEL` + `original_message_id`) must be signed by **same
  sender key** as the original SOS; accepted only within 60 min of the
  original (EMERGENCY_ABUSE §Signed SOS Cancel). Different signer ⇒ rejected
  + audited.
- `reason` codes are fixed integers; free-text note is a follow-up `Text`
  (P4) referencing the SOS `message_id` — keeps P0 payloads tiny (C5).

---

## 6. SOS rate limiting (DEC-EMERG-0005)

Enforced at the **message-engine layer per sender identity** (`sender_id`),
independent of routing.

- **3 SOS/hour per NodeId**, rolling 60-min window.
- 4th+ SOS from the same sender → **downgraded to P3** (not dropped; no P0
  routing/storage guarantee) — engine keeps a small per-sender ring buffer.
- Reset counter on `kind=CANCEL` confirmed (genuine emergency resolved).
- **Never rate-limit verified authority broadcasts** (`EmergencyAlert` with a
  valid chain) — authority channel is exempt.
- Soft emergency-trigger exception: if the node detected disaster mode (SOS
  density surge), the per-sender cap is *raised to 5/hr* for that sender
  (bounded burst; documented, DEC-EMERG-0005).
- **Sybil caveat recorded**: keygen is free ⇒ per-identity caps are evadable;
  absolute Sybil resistance is out of scope. Damage is bounded by (a) no
  spoofed broadcasts, (b) area-cluster triggers, (c) dedup + P0-exempt storage
  (documented decision, not a gap — RES-0017 §7).

Implementation surface (message-engine integration, §10):
`EmergencyRateLimiter` registry (map sender → rolling window), pure +
deterministic, unit-testable; registry bounded (LRU cap 4096 senders, oldest
evict) to prevent a memory-DoS (redteam will probe).

---

## 7. Broadcast relay path (verify_and_relay)

Relay behavior on receipt of `ContentType::EmergencyAlert`:

```
1. decode envelope (codec)          → fail: drop+log
2. version + signature verify       → fail: drop+log
3. dedup by message_id              → dup: drop silently
4. chain present + verify_chain
     (field18, trust, sender_id)    → fail: drop+log (no reply = anti-probing)
5. scope checks (area ⊂ authority area,
     message_type ⊂ functional scope,
     severity ≤ broadcast_max_severity)
   [post-DESIGN: authority profile] → fail: drop+log
6. NOT expired (expires_at ≥ now)   → fail: drop+log
7. mandatory relay to all link-local peers (P3 queue)
8. deliver to local app layer (subscribes to emergency event)
9. audit metadata entry (no content)
```

- Mandatory-relay rule: a node **must** relay *verified* broadcasts (even when
  "relay off" is set in normal mode, EXCEPT drill broadcasts) — anti-
  starvation; it may only drop unverifiable ones.
- No per-relay ACK, no sender feedback (anti-probing), no status reports.
- Hop accounting via existing `hop_count`/`max_hops`; `max_hops` unset for
  broadcasts = flood (matches DISASTER_MODES "255 hops authority").

---

## 8. Disaster-mode state machine (DEC-EMERG-0008 + §4)

```
states:  NORMAL → EMERGENCY → CRISIS → DEGRADED → NORMAL
         (forward-only transition; each ≥ 15 min on deactivation path)
```

Activation signals (automatic, DISASTER_MODES):

1. **SOS density surge**: >10 SOS within 1 km² / 10-min rolling window, or
   >5 SOS same area / 5 min.
2. **Network degradation**: gateway connectivity < 10% of 7-day average for
   > 15 min.
3. **SOS rate increase**: >10× baseline in 30 min.

Manual activation (`EmergencyAlert` body `message_type := ALERT` with
`DisasterMode` authority profile + chain) — authority-commanded, overrides
automatic deactivation.

Policies on EMERGENCY (from DISASTER_MODES + PRIORITY_MODEL):

| Concern | Policy |
|---------|--------|
| P0/P1 routing | Epidemic + all transports |
| P4-P7 | Suspended (existing P4-P7 suspended) |
| Storage | P0-P2 pooled ≤ 80%, P3 ≤ 20%, P4-P7 evicted if needed |
| Battery | cutoff 15%→3% for P0-P2 relay; P0 escape hatch at any battery |
| Scan | BLE 5 s→0.5 s (documented ~30% drain, EPISTLE corroboration) |
| Gossip hop limit | auto 5, authority 255 |

Mode propagation: `EmergencyAlert` body `message_type := DRILL` no —
`Activate`/`Deactivate` subtypes carried via a `DisasterSignal` structure in
the EmergencyAlert body (message_type + area + reason + hop_limit), gossip
re-broadcast with hop decrement, converged (30-90 s @ 50 nodes). Local
activation is immediate (no consensus wait).

Deactivation: authority `Deactivate` OR (SOS below 1× baseline for 30 min AND
gateway > 50% of 7-day average AND no authority activate). Slow on purpose.

Battery policy table lives in the emergency module as a pure function
(`battery_policy(battery_pct, mode) -> PriorityCutoff`) — unit-testable +
usable by platform nodes.

---

## 9. Audit log (pseudonymous, content-free)

`EmergencyAuditRecord { event_type, authority_id_hash: [u8;16],
area_code, severity, timestamp, broadcast_id, message_type, was_cancelled,
cancel_timestamp }` — matches governance's record.

- 30-day retention for SOS metadata (per-abuse), 1-year for authority
  events (per-governance).
- Always pseudonymous (`SHA-256(identity)[..16]`), never plaintext sender.
- No message content ever stored in the audit trail.
- Wired to observability via typed `tracing` events (`emergency.*`) + metrics
  counters (OBS-001 seam).

---

## 10. Engine integration seams

`MessageEngine` gains:

- `pub fn set_emergency_provider(&self, provider: Arc<dyn EmergencyProvider>)`
  (parallel to `set_key_directory`).
- **receive-path gate (wired at IMPLEMENT/TEST, iter 68/69):** `deliver_or_relay`
  consults the provider when armed for `EmergencyAlert` (`verify_envelope`
  pipeline → drop-without-reply on failure per AC-10; verified relay; drill
  surfaced-suppressed) and `Sos` (module classifier + AC-5/AC-6 rate-limit
  bookkeeping: record on accepted SOS, reset on verified cancel, drop on
  classifier rejection). Unarmed (`NoopEmergencyProvider`) ⇒ byte-identical
  pre-emergency behavior (AC-11 regression test).
- `send_message` hook (design reference): when `payload_type == EmergencyAlert`,
  attach `auth_cert_chain` from the provider; when `payload_type == Sos`,
  enforce rate limit. v1 ships the **receive-side** gate + provider seam; the
  end-to-end **send-side** build path is platform/tooling-owned (authority
  broadcaster) and exercises the same provider + module gates — recorded as a
  known_limitation of this node (engine does not synthesize emergency
  envelopes; app/authority layers build them via the module).
- `process_incoming` hook: on `EmergencyAlert` → run §7 pipeline (chain verify,
  scope, expiry, mandatory relay, deliver marker, audit) — **implemented** via
  the `deliver_or_relay` gate above; on `Sos` → rate-limit bookkeeping + audit
  + deliver marker — **implemented**. Audit rows are written by
  `EmergencyGateway::verify_envelope`.
- `deliver_or_relay` unchanged structurally; emergency deliverables are surfaced
  via the existing `delivered_tx` broadcast channel. Typed `EmergencyEvent`
  wrappers (`emergency.broadcast.delivered`, …) are **deferred to platform
  nodes** (ANDROID-001/IOS-001/DESKTOP-001) — the engine exposes the raw
  envelope; OS rendering happens there. Privacy rule P1/P2: never emitted with
  message content — the audit path already carries metadata-only records.

`EmergencyProvider` trait (implemented by `emergency::EmergencyModule` +
desktop adapter thread later):

```rust
pub trait EmergencyProvider {
    fn authority_root(&self) -> Option<[u8; 32]>;   // preloaded NDMA root pubkey
    fn build_emergency_broadcast(&self, spec: BroadcastSpec)
        -> Result<Envelope, EmergencyError>;
    fn build_sos(&self, spec: SosSpec) -> Result<Envelope, EmergencyError>;
    fn verify_authoritative(&self, envelope: &Envelope) -> Result<VerifiedAuthority, EmergencyError>;
    fn rate_limit(&self, sender: &[u8], mode: DisasterMode) -> RateLimitDecision;
    fn current_mode(&self) -> DisasterMode;
    fn audit(&self, record: EmergencyAuditRecord);
}
```

- Default provider: `NoopEmergencyProvider` (nothing loaded — engine behavior
  identical to today until armed). This preserves the existing 361-green
  baseline; `set_emergency_provider` must be safe to call at any time.

### Module layout

```
crates/iris-core/src/emergency/
  mod.rs          // EmergencyModule facade + EmergencyProvider + exports
  model.rs        // broadcast/SOS/authority payload structs + enums
  codec.rs        // CBOR encode/decode for emergency payloads (CDE)
  authority.rs    // authority profile, chain building/verification wrapper
  broadcast.rs    // build/verify/relay pipeline
  sos.rs          // build/verify/cancel
  rate_limit.rs   // rolling-window per-sender limiter
  mode.rs         // disaster state machine + triggers + battery policy
  audit.rs        // EmergencyAuditRecord store (bounded)
  drill.rs        // TEST_MODE (DRILL authority certs, display suppression)
```

---

## 11. TEST_MODE drill capability (DEC-EMERG-0008)

- `drill: bool` on EmergencyBroadcast + `kind := TEST` on SOS + dedicated
  DRILL authority certificates (marked TEST, never valid for real alerts).
- Drill broadcasts: rendered as a yellow "DRILL" banner, **never** trigger
  full-screen takeover or OS-level emergency channels, no real-alert
  side-effects (no storage suspension etc.).
- Drill data isolated in audit (separate `event_type`), cancel-anytime.
- Alignment: FEMA HSEEP exercise methodology (seminar→tabletop→functional→
  full-scale) via `docs` + TEST-001 integration plan; simulator burndown in
  SIM-001 is a follow-up (not in this node's evidence scope).

---

## 12. Document corrections (C1-C6, applied in this node)

- **C1** — EMERGENCY_BROADCAST.md: replace "instructions ≤ 2000 chars" with
  compact headline/instructions (96/96) + signed-follow-up UPDATE. Wire limits
  authoritative.
- **C3** — EMERGENCY_UX.md: replace 880 Hz / 880-660-880 pattern with the
  standard WEA two-tone **853 Hz + 960 Hz** (FCC WEA signal; ATIS device spec
  pin at IOS/ANDROID node), or a documented deliberate alternative. UX doc
  updated in this node.
- **C4** — EMERGENCY_ABUSE.md: migrate IPC §420/§505/§153A‑class refs to
  **BNS 2023** (§420→§318, §505→§353(2); §153A mapping flagged for
  LEGAL-001). Legal consequence text updated.
- **C5** — SOS cadence: align doc claims to implemented ack.rs P0 policy
  (30 s initial, 2×, unlimited-until-TTL; no 15-min standard). RES-0017 §3.
- **C6** — EMERGENCY_GOVERNANCE.md: "X.509-inspired" header →
  "SPKI-style key-anchored certificate" (+ make clear no PKIX parsing, no
  rustls-webpki family).

---

## 13. Acceptance criteria (AC-1..AC-14)

`ACCEPTANCE_POLICY` `safety_critical_additional` applies (EMERG-001): human
approval (gate RESOLVED DEC-0009), external audit recommended at production,
adversarial coverage > 95%, fuzzing ≥ 24 h min — fuzz duration recorded as
known_limitation of this node (deferred to TEST-001 / CI) unless REDTEAM
evidence at SECURITY_REVIEW suffices.

- **AC-1** — EmergencyBroadcast CBOR encode/decode round-trips; canonical CDE;
  2-element chain fits the < 1 KB P3 budget (measured 690 B); full 4-element
  chain measured 1277 B and fits the ≥ 64 KB P3 envelope class —
  known_limitation: never-fragmentable ⇒ unroutable over BLE (512 B);
  oversized build returns an error (no silent truncation).
- **AC-2** — Authority broadcast chain verification: valid chain passes;
  missing/empty/overlong(>4)/untrusted-root/broken-link/expired/revoked/
  sender-mismatched/all small-order chains are rejected + audited (no reply).
- **AC-3** — Geographic + functional scope + severity caps enforced from the
  authority profile; out-of-scope broadcasts dropped.
- **AC-4** — SOS build: `ContentType::Sos`, P0, never encrypted, payload
  ≤ 84 B, Ed25519 signed; direct-encrypted SOS to known peer uses normal path.
- **AC-5** — SOS rate limit: 3/hr per sender downgrades 4th to P3; window
  rolling; reset on verified CANCEL; authority broadcasts exempt.
- **AC-6** — SOS cancel: accepted only from original sender within 60 min;
  otherwise rejected + audited.
- **AC-7** — Disaster mode: activation triggers (SOS density, gateway
  degradation, rate surge) fire; manual authority activation overrides;
  EMERGENCY→CRISIS→DEGRADED→NORMAL transitions with ≥ 15-min holds; gossip
  hop limits 5/255.
- **AC-8** — Status reporting OFF by default; SOS ack is application message;
  no per-relay broadcast ACK; cancel is payload-type (not new ContentType).
- **AC-9** — Audit log metadata-only + pseudonymous + bounded; no message
  content stored; retention 30 d (SOS) / 1 y (authority).
- **AC-10** — Mandatory relay of verified broadcasts; unverifiable dropped
  (anti-starvation + anti-probing); drill broadcasts excluded from OS surfaces.
  **Engine receive-path gate wired** (`deliver_or_relay`): armed provider
  verifies `EmergencyAlert` → Drop-without-reply / relay / drilled-suppressed;
  tests `emergency_gate_drops_unverifiable_alert_no_reply` +
  `emergency_gate_unarmed_noop_keeps_behavior_identical`.
- **AC-11** — Engine integration backward-safe: `NoopEmergencyProvider`
  default keeps all 366 workspace tests green (no behavior change when
  unarmed); verified by `emergency_provider_defaults_to_noop_and_switches` +
  `emergency_gate_unarmed_noop_keeps_behavior_identical`.
- **AC-12** — Doc corrections C1/C3/C4/C5/C6 applied to
  EMERGENCY_BROADCAST/UX/ABUSE/GOVERNANCE docs.
- **AC-13** — Adversarial review: redteam (SECURITY_REVIEW) finds no
  CRITICAL/HIGH; CRITICAL findings halt + operator report (RISK_POLICY).
- **AC-14** — Workspace green + clippy 0 + RUSTSEC clean (no new crypto/runtime
  crates, D6); emergency module has structural + negative + boundary tests.

---

## 14. Test plan outline (→ TEST stage)

- Unit: codec round-trip + size budget + error paths; chain verify matrix
  (mirror identity chain tests incl. scope/permission layer); SOS build/verify/
  cancel; rate limiter (rolling window, downgrade, reset, LRU eviction,
  authority exemption); disaster state machine (trigger matrix, transitions,
  battery policy table); audit (pseudonymize, bound, retention).
- Integration: `send_message` EmergencyAlert attaches chain; `process_incoming`
  EmergencyAlert verified→delivered marker + rate-limit on Sos; mandatory-relay
  via shared SimulatedTransport (SIM-001) two-node; no-op provider baseline.
- Negative/adversarial: spoofed/crafted chains, small-order, expired, replay
  dedup, oversized payload, out-of-scope area, flood downgrade, cancel-forgery.
- Regression: full workspace unchanged when `NoopEmergencyProvider`.

---

## 15. Deferred / known limitations (this node)

- BroadcastCancel/other new ContentTypes → PROTO-001 v2 (RED-0010).
- 24 h fuzz campaign → TEST-001 / CI (this node delivers structural + redteam
  evidence).
- Physical transport + OS alert surfaces (WEA channels, FGS, iOS) → platform
  nodes (ANDROID-001/IOS-001/DESKTOP-001); this node emits typed events.
- Remote revocation propagation lag in isolated partition (governance-
  accepted); revocation-list message is v1, latency documented.
- Sybil-resistance ceiling (per-identity caps evadable) documented, not a gap.
- Authority provisioning UX (QR/load cert chains) → platform nodes + a follow-up
  `emergency` CLI/adapter.
- Emergency-channel auto-join (DISASTER_MODES) + vehicle-relay announcements →
  DISCO-001/platform follow-up.

---

## 16. Ordering for IMPLEMENT

1. `emergency/model.rs` + `codec.rs` (AC-1) — pure types, no engine touch.
2. `authority.rs` + `drill.rs` (AC-2,3,10) — chain wrapper + TEST certs.
3. `sos.rs` + `rate_limit.rs` (AC-4,5,6) — SOS + limiter.
4. `mode.rs` (AC-7) — disaster state machine + battery policy.
5. `audit.rs` (AC-9) — audit store.
6. `broadcast.rs` (AC-8,10) — full relay pipeline.
7. `mod.rs` facade + engine seams (AC-11) — `set_emergency_provider`,
   send/incoming hooks.
8. Doc corrections C1/C3/C4/C5/C6 (AC-12).
9. Tests + clippy + workspace (AC-14) → TEST → SECURITY_REVIEW (redteam) →
   VERIFY → ACCEPT.