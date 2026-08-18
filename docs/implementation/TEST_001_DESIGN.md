# TEST-001 Design — Test Infrastructure v1

**Document ID**: IRIS-TEST-001-DESIGN-001
**Version**: 1.0
**Node**: TEST-001 (P0 TEST, deps ROUTE-001/MSG-001/CRYPTO-001 all COMPLETE)
**Date**: 2026-08-18
**Stages**: DISCOVER (iter 123, TEST-001_DISCOVER.md) → RESEARCH (iter 124, RES-0023) → DESIGN (iter 125)
**Research basis**: RES-0023 (2026 best-in-class Rust/Android test infra, 7 evidence
passes, primary sources L1-L5, no AI citations — verdict PROCEED across G-1..G-8);
`TEST-001_DISCOVER.md` (in-repo inventory, iter 123: 613/0/1, proptest, fuzz 44M+,
tarpaulin 95.6% security, 39 JVM);
ACCEPTANCE_POLICY.yaml (universal + node-type criteria); RESEARCH_POLICY.yaml
(evidence maturity); reference design docs (BLE/WIFI_AWARE/WIFI_DIRECT_TRANSPORT_DESIGN.md + ANDROID_DESIGN.md — C2 AC-definition pattern);
**Absorbs**: Gaps G-1..G-8 (nextest; audit/deny; kani/loom; mutants+tarpaulin;
interop fixtures; CI; golden vectors; bench) + G-TI-1..6 (research gaps);
BLK-0003 (audit/deny gate), BLK-0004 (interop fixtures), BLK-0005 (bench/hardware).

---

## 1. Scope

Establish a durable, best-in-class **test infrastructure** for the IRIS workspace
(Rust `crates/iris-core` + `iris-android` + `iris-storage` + `iris-desktop` +
integration suites; JVM/Android unit suites in `android/app/src/test`). The node
produces **CI-executable enforcement** of existing quality (613/0/1, clippy 0,
rustfmt, proptest, fuzz) plus the gap-closing layers below. All net-new artifacts
are **config / fixtures / harness / workflow** — no protocol/routing/crypto code
changes (core logic is covered by COMPLETE nodes ROUTE-001/MSG-001/CRYPTO-001).

**Net-new deliverables**:
1. `deny.toml` — cargo-deny policy (advisories/bans/licenses/sources) as PR gate
   (G-2).
2. `.cargo/audit.toml` — cargo-audit suppression policy (fast live-DB scan,
   SARIF; committed audited suppressions with expires+reason) (G-2).
3. `.config/nextest.toml` — cargo-nextest profile (parallel runner, JUnit XML,
   retries, leaky-test detection, CI profile) (G-1).
4. `.github/workflows/ci.yml` — GitHub Actions matrix (OS × channel + MSRV cell),
   rust-cache, nextest via taiki-e, separate doctest, fmt/clippy lint job,
   tarpaulin coverage gate (ubuntu), cargo-deny + cargo-audit jobs, JVM job
   (JDK ≤ 21), mutants nightly-sharded sweep job (G-4/G-6).
5. `tests/golden_vectors/` corpus + `tests/protocol_conformance.rs` fixture
   harness — byte-exact envelope/framing/CBOR vectors both directions +
   adversarial/resync fixtures (G-5/G-7).
6. `crates/iris-core/kani/` (or `tests/kani/`) — Kani proof harnesses for pure
   functions, Linux-only CI job (G-3).
7. `cfg(loom)` leaf-structure concurrency models where feasible (dedup cache,
   replay high-water) + `tokio-test` behavior suites (G-3).
8. `benches/` — criterion 0.8.x CPU-path bench harness (codec, envelope, crypto,
   routing score, fragment); regression gate + battery/perf DEFERRED to BLK-0005
   (G-8).

**Explicitly deferred** (recorded, not blockers): physical-device/BLE/iOS/battery
bench (BLK-0005); hard mutation-score PR gate (trend signal v1, G-TI-2);
full-model formal verification of `transport::manager` (Kani models no
concurrency; loom bounded to leaf structures, G-TI-3); cross-OS coverage/formal
parity (Linux-only tooling, G-TI-1); JVM execution on toolchain host/CI only
(env-gated, G-TI-5).

## 2. Design decisions (RES-0023 verdict absorbed → DEC-TEST-0001..n)

| # | Decision | Basis (RES-0023) |
|---|----------|-------------------|
| DEC-TEST-0001 | **cargo-nextest = primary runner** (`.config/nextest.toml`: ci profile, JUnit XML, retries, leaky-test detection 100 ms default / LEAK-FAIL option); **doctests keep `cargo test --doc`** as separate step (tokio CI precedent; nextest stable-Rust limitation issue #16). MSRV-free. | G-1 ADOPT, nexte.st L1 |
| DEC-TEST-0002 | **cargo-deny = PR policy gate** (`deny.toml`: `[advisories]` deny + unmaintained=workspace, `[bans]` multiple-versions=deny + wildcards=deny, `[licenses]` allowlist MIT/Apache-2.0/ISC/Zlib, `[sources]` unknown-registry/unknown-git deny) + **cargo-audit = fast live-DB scan** on PR/schedule (`--deny warnings`, committed `.cargo/audit.toml` suppressions with expires+reason, SARIF upload). | G-2 ADOPT, cargo-deny book L1, RustSec L1 |
| DEC-TEST-0003 | **Kani = pure-function proofs** (Linux CI job only): codec decode bounds, fragment assembly arithmetic, quota/rate-limiter arithmetic, PRoPHET score, envelope size invariants. **Kani does NOT model concurrency** — no concurrency claims. | G-3 ADOPT-WITH-CONDITION, Kani book L1, ASE 2026 L2 |
| DEC-TEST-0004 | **loom = leaf-structure concurrency models** (`cfg(loom)` replacement types) for dedup cache, replay high-water marks, quota counters; **tokio-test = async behavior suites** (message_engine queue/ack/lifecycle, transport::manager registration/start-stop under paused time). transport::manager-full-model declared out-of-scope; correctness rests on tokio-test + property + adversarial + fuzz layers. | G-3 ADOPT-WITH-CONDITION, loom docs L1, tokio-test L1 |
| DEC-TEST-0005 | **tarpaulin = PR/merge coverage gate** (ubuntu runner, ptrace x86_64): workspace line threshold **≥ 80%** (`--fail-under 80`); **security path stays ≥ 95%** (SEC-001 AC-14 baseline). | G-4 ADOPT-WITH-CONDITION, tarpaulin README L4 |
| DEC-TEST-0006 | **cargo-mutants = nightly sharded trend signal** (mutants.out/{caught,missed,timeout,unviable}.txt triage; survival-rate metric frozen per G-TI-6: `killed / (mutants − unviable − timeout)`, equivalent-mutant exemptions scoped with rationale); **NOT a hard PR gate v1**; PR-time = changed-file incremental only. Deterministic-suite requirement → nextest retries off in mutants profile. | G-4 ADOPT-WITH-CONDITION, mutants.rs L1/L4, tensogram L4 |
| DEC-TEST-0007 | **PROTOCOL_CONFORMANCE fixtures** in `tests/protocol_conformance.rs` + `tests/fixtures/`: (a) RFC 9171/RFC 8949-derived CBOR vectors for shared primitive encodings (CBOR/UUIDv7/timestamp/framing); (b) Meshtastic-style **bounded-recovery** resync adversarial fixtures (corrupt length byte swallows ≤512 B until resync); (c) capture-style PDU fixtures (Wireshark BP pattern) for IRIS framing/envelope. Property-based sim harness over existing Sim* coordinators (ns-3 BPv7 pattern corroborated). | G-5 ADOPT, RFC 9171 L1, Meshtastic L4, Wireshark L4 |
| DEC-TEST-0008 | **Golden-vector corpus** `tests/golden_vectors/` + byte-exact both-way encode/decode harness; catches DISC-0009-class (PROTOCOL_TEST_VECTORS.md timestamp hex arithmetic) permanently; back-compat guard for BLE/Wi-Fi Direct framing + codec. Corpus provenance: IRIS envelope is flat non-RFC-9171 → vectors generated in-repo from DESIGN spec + cross-checked by round-trip + fuzz; maturity DESIGNED→UNIT_VALIDATED progressively (G-TI-4). | G-7 ADOPT |
| DEC-TEST-0009 | **GitHub Actions CI** `.github/workflows/ci.yml` (official + tokio reference): `actions/checkout@v4` → `dtolnay/rust-toolchain@stable` (+ MSRV cell via `include:`) → `Swatinem/rust-cache@v2` per-matrix-cell `key:` → `taiki-e/install-action@v2 tool: cargo-nextest` → `cargo nextest run --profile ci` + separate `cargo test --doc`; matrix `{ubuntu, macos, windows}-latest × {stable, MSRV}` with `fail-fast: false`; lint job (fmt+clippy) once on ubuntu; coverage job on ubuntu; audit+deny jobs; JVM job. Windows/macOS cells = behavioral only (G-TI-1). | G-6 ADOPT, GitHub docs L1, tokio CI L4 |
| DEC-TEST-0010 | **JVM job**: `actions/setup-java` Temurin ≤ 21 (Gradle 8.9 compatibility, G-TI-5) on ubuntu runner; runs `android/app` JUnit5 suites + shell checks; cargo-ndk/uniffi-bindgen legs stay env-gated (BLK-0005/ANDROID-001 notes). | G-6 ADOPT (JVM), G-TI-5 |
| DEC-TEST-0011 | **criterion 0.8.x bench harness now, gate deferred**: `benches/` with `harness = false`, `criterion_group!`/`criterion_main!`, `std::hint::black_box`; CPU-path benches (codec, envelope, crypto, routing score, fragment, store) run on CI without regression gate; **battery/physical-transport perf DEFERRED to BLK-0005 hardware** (criterion 0.8.x stable-channel; `#[bench]` nightly-only hard error ≥ 1.88). | G-8 ADOPT scaffold / DEFER gate, criterion docs L1 |
| DEC-TEST-0012 | **Baseline preservation**: all infra gates must hold existing workspace **613/0/1**, clippy **0** (--workspace --all-features --tests), rustfmt clean, proptest feature suites, fuzz 44M+ clean; config/fixtures/harness additions must not break the deterministic suite (mutants requires non-flaky suite; nextest CI profile serializes known-flaky only if any). | Existing evidence (iter 122/123/124), ACCEPTANCE_POLICY |

## 3. Acceptance criteria (AC-1..n — C2 gap RESOLVED, pattern BLE-001)

1. **AC-1** — cargo-nextest primary-runner config committed (`.config/nextest.toml`:
   ci profile with JUnit XML output + retries + leaky-test detection) and the CI
   workflow runs `cargo nextest run --profile ci` **plus** a separate
   `cargo test --doc` step; workspace results reproduce **613/0/1** under the
   runner (host-run where nextest installable; else CI-job documented + env-gated).
2. **AC-2** — supply-chain recurring gates committed: `deny.toml` (advisories/
   bans/licenses/sources policy per DEC-TEST-0002) + `.cargo/audit.toml`
   (zero suppression blocks without `expires`/`reason`); CI jobs `cargo deny
   check` + `cargo audit` present; running `cargo audit` host-side (v0.22.2
   present) reports **0 vulnerabilities** against current Cargo.lock.
3. **AC-3** — Kani proof harnesses committed for ≥ 4 pure-function targets (codec
   decode bounds, fragment arithmetic, quota/rate-limiter arithmetic, PRoPHET or
   envelope-size invariant), each `#[kani::proof]`-style, with a documented
   Linux-only CI job; no concurrency-overclaim text (Kani limitation recorded).
   *(Kani binary missing on dev host → harness + CI job authored; execution
   env-gated to Linux runner; verifier reproduces on CI.)* → env-gate note.
4. **AC-4** — loom leaf-structure models `#[cfg(loom)]` (dedup cache or replay
   high-water or quota counters) + tokio-test async behavior suites for at least
   message_engine lifecycle and transport::manager start/stop under paused time;
   documented soundness limits (SeqCst-as-AcqRel, no load-buffering); full-manager
   modeling explicitly out-of-scope.
5. **AC-5** — tarpaulin workspace coverage gate wired: CI job `cargo tarpaulin
   --out lcov --fail-under 80` (ubuntu x86_64); measured coverage recorded
   (workspace ≥ 80% line; security path stays ≥ 95%); JVM path excluded (host CI
   job separate).
6. **AC-6** — cargo-mutants wired as nightly/sharded trend job (deterministic
   suite, nextest retries off in mutants profile), with survival-rate metric
   definition frozen (G-TI-6) + exemptions policy; **not** a hard PR gate.
7. **AC-7** — PROTOCOL_CONFORMANCE fixture harness committed: `tests/
   protocol_conformance.rs` + `tests/fixtures/` with RFC 9171/RFC 8949-derived
   CBOR vectors + bounded-recovery resync adversarial fixtures + capture-style
   PDU fixtures; property-based sim harness over existing Sim* coordinators;
   tests pass host-side (feature or default).
8. **AC-8** — golden-vector corpus committed (`tests/golden_vectors/`, byte-exact
   both-way harness): IRIS envelope + framing + CBOR/UUIDv7/timestamp vectors;
   at least one regression test re-catches a DISC-0009-class byte-exact error;
   provenance documented (G-TI-4).
9. **AC-9** — CI workflow committed (`.github/workflows/ci.yml`): matrix
   OS×channel + MSRV cell, rust-cache per-cell, nextest+doctest, lint job,
   deny/audit jobs, tarpaulin job, mutants nightly job, JVM job (JDK ≤ 21);
   `fail-fast: false`; workflow YAML parse-validates.
10. **AC-10** — criterion bench harness committed (`benches/`: codec, envelope,
    crypto, routing score, fragment; `harness = false`); `cargo bench` runs
    host-side (criterion compiles); baseline stored; regression gate + battery/
    perf deferred to BLK-0005 documented.
11. **AC-11** — baseline preserved: pre-existing **613/0/1** held + 20 net-new
    (tokio_behavior 4 + protocol_conformance 10 + golden_vectors 6) =
    workspace `cargo test --workspace --all-features` **633/0/1**, clippy **0**,
    rustfmt clean after all infra additions
    (config/fixtures/harness must not break the deterministic suite).
12. **AC-12** — doc reconciliation + VERIFY: this design's claims match
    committed artifacts (execution-log + PROJECT_GRAPH updated per-pass);
    SECURITY_REVIEW-equivalent recorded only if code touched (none — config/
    fixtures/harness only); independent verifier reproduces AC-1..11 evidence;
    known_limitations updated (G-TI-1..6, BLK-0003/0004/0005, env-gates).

## 4. Sensible default gate matrix (per-OS, honors G-TI-1)

| Job | ubuntu-latest | macos-latest | windows-latest |
|-----|---------------|--------------|----------------|
| nextest + doctest | ✅ | ✅ | ✅ |
| fmt + clippy lint | ✅ | — | — |
| tarpaulin coverage gate | ✅ ptrace x86_64 | — | — |
| Kani proofs | ✅ | — | — |
| deny + audit | ✅ | ✅ | ✅ |
| JVM (JDK ≤ 21) | ✅ | — | — |
| mutants nightly sharded | ✅ | — | — |
| criterion bench (no gate) | ✅ | — | — |

## 5. Risks & known_limitations (carried to graph)

- **G-TI-1**: tarpaulin ptrace + Kani Linux-only → Windows/macOS cells run
  behavioral suites only; gate matrix records this explicitly (AC-3/AC-5/AC-9).
- **G-TI-2**: mutation gate economics → v1 nightly trend, PR changed-file
  incremental; thresholds after first baseline.
- **G-TI-3**: loom soundness ceiling → leaf structures only, no full-manager claim.
- **G-TI-4**: golden-corpus provenance → in-repo generation + round-trip/fuzz
  cross-check; maturity DESIGNED→UNIT_VALIDATED.
- **G-TI-5**: JVM host → CI `setup-java` Temurin ≤ 21; Gradle 8.9 compat confirm
  at IMPLEMENT.
- **G-TI-6**: mutation-score metric semantics → frozen definition + exemptions.
- **Env-gates**: nextest/deny/kani/mutants binaries absent on dev host →
  configs/workflows authored + host-run where possible; CI execution is the
  authoritative evidence source; tarpaulin 0.37.2 + audit 0.22.2 host-present.
- **BLK-0005**: physical-device/battery/perf bench deferred (AC-10).

## 6. Next

**IMPLEMENT (iter ~126)**: author `.config/nextest.toml`, `deny.toml`,
`.cargo/audit.toml`, `.github/workflows/ci.yml`, `tests/golden_vectors/` +
`tests/protocol_conformance.rs`, `benches/`, Kani/loom harnesses → host-run
verifiable legs → **TEST** (AC-1..11 evidence) → **VERIFY** (independent verifier)
→ **ACCEPT** → NODE_TRANSITION to IOS-001.