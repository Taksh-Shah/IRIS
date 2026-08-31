# IRIS Bug-Hunt — 6-Section Completion Summary

**Status as of 2026-08-31.** IRIS was audited and hardened via a 6-way parallel split. Each
section has its own self-contained tracker (state), loop spec (execution logic), and fix log
(per-run journal) in this directory.

## Roll-up

| Section | Owner | Scope | Tracker | Findings | Fixed | Deferred / blocked |
|---|---|---|---|---:|---:|---|
| 1 | Priyam | crypto · identity · security (`iris-core/src/{crypto,identity,security}`) | [`priyam_problems.md`](priyam_problems.md) | 36 | 34 | 1 🔒 PRY-11 (SOS — future scope) · 1 ⚪ PRY-28 (premise disproved) |
| 2–3 | Taksh | routing · DTN · transport · gateway · FFI seam · simulator fidelity | [`taksh_problems.md`](taksh_problems.md) | ~281 + ~21 hardware | ~275 + all HW | 5 🔒 (BLE-1 Arc-share, ROUT-23/24/26 spec conflicts, RF-18 TLS ext. dep) · FFI-6 (Wi-Fi Aware NDP responder — largest remaining architectural item) |
| 4 | Sohan | Android (`crates/iris-android`, `android/`) | [`sohan_problems.md`](sohan_problems.md) | 16 | 14 | AN-6 → crypto sprint |
| 5 | Shrey | iOS (`ios/`, `crates/iris-ios`) | [`shrey_problem.md`](shrey_problem.md) | 72 | 62 | 10 🔮 (deferred with rationale — MTU rework, dev-seam gating, ATT-spec-correct, etc.) |
| — | Rahul | persistent-maintenance / `PM-*` (message-engine, emergency, storage GC) | [`rahul_problems.md`](rahul_problems.md) | 33 | 29 | 4 🔒 (Tier 3 structural — awaiting Section 1/3 contract sign-off) |
| 6 | Dhairya | Desktop app & build infrastructure | [`dhairya_section6_bugs/`](dhairya_section6_bugs/) | 28 | 21 | 3 🔀 routed to other sections · 3 ⬜ · 1 🔮 |
| **Total** | | | | **≈ 490** | **≈ 455** | remainder = documented future scope / cross-owner sign-off |

## Before → after — the material shift

**From "does not actually work" to "verified working on real hardware."**
- iOS did not compile at all (5 build breaks). Now: builds clean, 62 findings fixed.
- Android's Wi-Fi/BLE transports "appeared unable to move a single byte" on real devices —
  Critical FFI contract mismatches (peer handles from two namespaces, beacons never
  advertised, an NDP path with no responder, sends reporting success for queued-only frames),
  all masked by simulated adapters that satisfied the Rust side by construction and Kotlin
  adapters that returned success unconditionally. Now: **BLE 3-way mesh and Wi-Fi Direct
  message delivery confirmed live** across three physical phones (vivo, a second Android
  device, Samsung S24 Ultra), end-to-end send + receive, over `adb logcat` on both ends.

**The security layer became a real trust boundary.**
- Was: an armed node authorized *every* emergency broadcast (empty allowlist → `Authorized`);
  the replay high-water advanced on an unverified, attacker-spoofable `sender_id`; P0/P1
  storage accounting was unbounded (one sender marking traffic P0 → `u64::MAX`); reputation
  had no decay caller so a peer floored by a few transient drops was permanently unroutable;
  the RED-0011 small-order blocklist was corrupted and incomplete; `diffie_hellman` never
  rejected small-order peer keys.
- Now: every accept/deny decision hardened, chains validate multi-level, SOS/authority
  envelopes are signature-verified at the ACL, quotas and the reserved pool are enforced,
  reputation self-heals with a non-zero routing floor. The four security engines are
  **sharded 64-way** so flooding a node no longer stalls all inbound processing
  (~3× throughput under spread load — `crates/iris-core/benches/security_contention.rs`).

**Resource safety and correctness across the stack.**
- DTN store/eviction bounded and priority-aware (P0 never evicted); relay traffic now counts
  against per-sender storage quota; ACK and relay states are no longer absorbing (exponential
  backoff retry); wire framing hardened against corruption (version byte, bounded resync,
  reject-oversize without panic); the desktop `/sos` command sends the actual emergency text
  instead of debug output.

## How to read a section

Each section directory follows the same three-file pattern:
- `*_problems.md` (or `*_problem.md`) — **state**: every finding with its `Fix status` line,
  plus a Progress Tracker roll-up table.
- `*_problems_loop.md` (or `*_problem_loop.md`) — **logic**: tier order, per-finding
  protocol, safety gates, scope boundaries.
- `*_fix_log.md` (or `*_problem_log.md`) — **journal**: one entry per run — what was
  attempted, what landed, what was blocked/reverted, and the verification evidence.

The remaining open items are deliberate future scope (SOS service, LoRa transport,
Wi-Fi Aware data path) or blocked on a cross-section contract that another owner must
specify — not undiscovered defects.
