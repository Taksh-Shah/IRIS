# Architecture Decision Records — Index

**Last updated:** 2026-08-11  

---

## Purpose

Architecture Decision Records (ADRs) document significant technical decisions made during IRIS development. Each ADR records: the context that motivated the decision, the options considered, the decision made, and its consequences (both positive and negative).

ADRs are immutable records — once approved, an ADR is not edited to change its content (only to update its status). If a decision is reversed, a new ADR supersedes the old one; the old ADR is marked "Superseded" with a reference to the new one.

## ADR Status Values

- `Draft` — under discussion, not yet approved
- `Approved` — accepted and implemented
- `Superseded` — replaced by a later ADR (with reference)
- `Deprecated` — no longer applicable (feature removed or approach abandoned)

---

## Index

| ID | Title | Status | Date | Summary |
|----|-------|--------|------|---------|
| [ADR-0001](ADR-0001.md) | Use CBOR over Protocol Buffers for wire format | Approved | 2026-01-15 | CBOR (RFC 8949) chosen for self-describing binary encoding; no schema compilation; DTN ecosystem uses it |
| [ADR-0002](ADR-0002.md) | Use Ed25519 for identity and signing | Approved | 2026-01-15 | Ed25519 chosen over RSA and ECDSA for constant-time 64-byte signatures; fast verification on ARM |
| [ADR-0003](ADR-0003.md) | Use Rust for core protocol implementation | Approved | 2026-01-20 | Rust chosen for memory safety, C-level performance, and cross-platform FFI to Android/iOS |
| [ADR-0004](ADR-0004.md) | Use SQLite for message store | Approved | 2026-02-01 | SQLite (via rusqlite) chosen for DTN bundle store; WAL mode, ubiquitous on mobile, SQL for complex queries |
| [ADR-0005](ADR-0005.md) | PRoPHET as primary opportunistic routing algorithm | Approved | 2026-02-10 | PRoPHET with Spray-and-Wait fallback chosen over Epidemic and MaxProp; better delivery/overhead tradeoff |
| [ADR-0006](ADR-0006.md) | ChaCha20-Poly1305 for symmetric encryption | Approved | 2026-02-15 | ChaCha20-Poly1305 (RFC 8439) chosen over AES-GCM for software performance on ARM without AES-NI |

---

## Proposed (Not Yet Decided)

| ID | Title | Status | Owner | Target Date |
|----|-------|--------|-------|------------|
| ADR-0007 | BLE discovery protocol: GATT vs advertisement-only | Draft | Transport team | Month 4 |
| ADR-0008 | Contact history storage: per-event vs aggregated | Draft | Routing team | Month 4 |
| ADR-0009 | Android foreground service architecture | Draft | Android team | Month 3 |
| ADR-0010 | iOS background transport strategy | Draft | iOS team | Month 5 |

---

## ADR Template

New ADRs use the following template:

```markdown
# ADR-NNNN: [Title]

**Status:** Draft | Approved | Superseded | Deprecated
**Date:** YYYY-MM-DD
**Deciders:** [names or teams]
**Supersedes:** [ADR-XXXX if applicable]
**Superseded by:** [ADR-XXXX if applicable]

## Context

[Why is this decision needed? What problem does it solve?]

## Options Considered

### Option A: [Name]
[Description, pros, cons]

### Option B: [Name]
[Description, pros, cons]

## Decision

[Which option was chosen, stated clearly]

## Rationale

[Why this option over the others]

## Consequences

### Positive
- [What gets better]

### Negative
- [What gets worse or what risk is accepted]

### Neutral
- [Side effects that are neither good nor bad]
```

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial index |
