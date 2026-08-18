# TEST-001 Verification Record (AC-1..12 evidence)

**Schema version**: 1.0
**Node**: TEST-001 (P0 TEST)
**Iteration**: ~130 (VERIFY stage, after TEST COMPLETE iter ~129)
**Date**: 2026-08-18
**Status**: evidence table written; **independent verifier APPROVE** (FAIL → reconciled in-pass, 4 findings fixed/recorded)
**Baseline**: 633 passed / 0 failed / 1 ignored (lifted from 613 by TEST-001 net-new suites)

---

## AC-1..12 evidence table

| AC | Claim | Evidence artifact | Host verification | Verifier result |
|----|-------|-------------------|-------------------|-----------------|
| AC-1 | cargo-nextest primary-runner config + CI run + separate doctest step | `.config/nextest.toml` (`[profile.ci]`: JUnit XML `target/nextest/junit.xml`, retries 0, `leak-timeout 100ms`, `slow-timeout`; `[profile.ci-mutants]` deterministic retries=0 LEAK-free) + `.github/workflows/ci.yml` `test` job: `cargo nextest run --workspace --all-features --profile ci` + separate `cargo test --doc` | nextest binary absent on host → **execution CI-leg**; config + jobs authored, parse-valid | APPROVE |
| AC-2 | supply-chain recurring gates + 0 vulnerabilities | `deny.toml` (advisories/bans/licenses/sources) + `.cargo/audit.toml` (17 suppressions with reason + expiry **2026-12-31**) + CI `deny` + `audit` jobs | **`cargo audit --deny warnings` EXIT 0** — 0 vulns / 574 deps; `deny.toml` PARSE OK after in-pass fix (removed stale `skip=[]`/`skip-tree=[]` duplicate-key) | **APPROVE** |
| AC-3 | Kani proofs ≥4 pure-fn targets, Linux CI documented, no concurrency overclaim | `crates/iris-core/src/kani_proofs.rs` — **7 `#[cfg(kani)]` harnesses** (MessageId exact-16; ContentType 1..=16 / MessagePriority 0..=7 round-trip; fragment split_payload conservation+budget incl empty branch; quota evict_amount; rate_limiter refill_tokens; prophet floor_intervals) + kani CI job (`cargo install kani-verifier --locked` + `cargo kani --package iris-core`) | Kani absent on Windows host (DEC-TEST-0003) → **execution CI-leg Linux** | APPROVE |
| AC-4 | loom leaf models + tokio-test async behavior suites; soundness documented; full-manager out-of-scope | `loom/loom_models.rs` — 5 leaf models (replay high-water stream-integrity + replay-rejection, dedup first-wins, quota no-lost-update CAS, quota saturate-not-wrap); `tests/tokio_behavior.rs` — 4 paused-time suites (round-trip delivery, manager register/select/deregister, timer background tasks, clean drop); `engineering/tools/verify-loom.sh`; soundness limits in TEST_001_DESIGN.md (single-stream only; cross-stream drop legitimate; `(0,0)`==sentinel==replay; SeqCst-as-AcqRel; no load-buffering) | **loom 5/5 PASS 3.24s** (`cargo rustc -p iris-core --test loom_models -- --cfg loom` + run binary); **tokio_behavior 4/4 PASS 0.00s** (paused clock) | APPROVE |
| AC-5 | tarpaulin workspace gate wired + coverage recorded | CI `coverage` job: `cargo tarpaulin --workspace --all-features --out lcov --output-dir target/coverage --fail-under 80` (ubuntu x86_64; ptrace Linux only G-TI-1) | tarpaulin 0.37.2 host-present but ptrace Linux-only → **execution CI-leg**; security path ≥95% established at DISCOVER (95.6%) | APPROVE |
| AC-6 | cargo-mutants nightly/sharded trend job, NOT hard PR gate | CI `mutants` job: `NEXTEST_PROFILE=ci-mutants cargo mutants --shard 1 --shard-count 1 --test-tool cargo-nextest --timeout 30 --in-place` (schedule/dispatch only) | cargo-mutants absent on host → **execution CI-leg** | APPROVE |
| AC-7 | PROTOCOL_CONFORMANCE fixture harness + property sim, passes host-side | `tests/protocol_conformance.rs` — **10 tests** (RFC 8949 §4.2 CDE integer vectors hand-computed; 256-B definite-length byte-string boundary; RFC 9562 UUIDv7 nibble/variant; RFC 9171 §4.2.7 timestamp range; bounded-recovery resync ≤512 B; capture-style PDU dissection; 3 sim property suites over Sim* coordinators) + literal `tests/fixtures/README.md` provenance (AC-7 layout) | **10/10 PASS 0.02s** | APPROVE |
| AC-8 | golden-vector corpus byte-exact both-way + DISC-0009-class regression + provenance | `tests/golden_vectors.rs` — **6 tests** + corpus `tests/golden_vectors/V001_p0_sos.md` / `V002_p0_sos_signing.md` / `V003_full_envelope.md`; regression `timestamp_arithmetic_reveget_66b7ff00_not_doc_error_66b5c000` (field-7 must encode `0x071a66b7ff00`, never doc-wrong `0x071a66b5c000`); V001 pinned 237 B ≤ P0_MAX_ENVELOPE_BYTES; provenance in TEST_001_DESIGN.md §G-TI-4 (in-repo generation + round-trip/fuzz cross-check) | **6/6 PASS 0.02s** | APPROVE |
| AC-9 | CI workflow matrix + lint + deny/audit + coverage + mutants + JVM JDK≤21; fail-fast false; YAML valid | `.github/workflows/ci.yml` — matrix `test` (ubuntu MSRV 1.86.0/stable/beta + windows/macos stable; nextest + doctest; JUnit artifact), `lint` (fmt + clippy `-D warnings`), `coverage`, `deny`, `audit`, `fuzz`, `mutants`, `kani`, `loom`, `jvm` (Temurin JDK 21); `fail-fast: false`; weekly cron + workflow_dispatch | **python YAML parse-valid** | APPROVE |
| AC-10 | criterion 0.8.x bench harness + host run + baseline stored; perf regression deferred | `benches/{codec,envelope,crypto,routing,fragment}.rs` — 5 criterion harnesses, `[[bench]] harness = false` | **compile + run Success** (fragment re-confirmed this pass: warm-up→collect→analyze); baseline/regression gates DEFER BLK-0005 | APPROVE |
| AC-11 | baseline held + lifted | workspace suite | **`cargo test --workspace --all-features` = 633 passed / 0 failed / 1 ignored**; clippy `-D warnings` all targets = 0; rustfmt clean (`cargo fmt --all` exit 0, applied in-pass for bench import-order/wrap) | APPROVE |
| AC-12 | doc reconciliation + VERIFY + independent verifier + known_limitations | TEST_001_DESIGN.md claims ↔ committed artifacts (DBEH checked; fixtures README added for AC-7 literal layout); known_limitations G-TI-1..6 + BLK-0005 + env-gates current | **reconciliation PASS** (stale 613/0/1 texts fixed in-pass); independent verifier **APPROVE** after 4 findings reconciled | APPROVE |

## Live re-verify (this pass, this host)

- `cargo test --workspace --all-features` → **633 passed / 0 failed / 1 ignored**
- `cargo clippy --workspace --all-features --all-targets -- -D warnings` → **0 warnings**
- `cargo fmt --all -- --check` → **exit 0** (fixes applied via `cargo fmt --all` this pass)
- `cargo audit --deny warnings` → **exit 0** (0 vulnerabilities / 574 deps)
- loom → **5/5 PASS 3.24s**; tokio_behavior → **4/4 PASS 0.00s**
- protocol_conformance → **10/10 PASS 0.02s**; golden_vectors → **6/6 PASS 0.02s**
- benches → **5/5 run Success**
- YAML → PROJECT_GRAPH / PROJECT_STATE / execution-state all parse-valid

## CI-leg rows (execution env-gated; artifacts + jobs authored)

AC-1 nextest, AC-3 kani, AC-5 tarpaulin, AC-6 mutants, AC-9 matrix execution, AC-12 jvm. Each has a committed config/CI job; execution runs on the GitHub runner (ubuntu/windows/macos). Documented gates: G-TI-1 (coverage/formal Linux-only), G-TI-2 (mutation economics), G-TI-5 (JVM host).

## Independent verifier reproduction

**Dispatched**: verifier subagent (ses_feb169990ffe3YUsbABvSZRFZU) independently reproduced AC-1..12.

**Verdict**: initially **FAIL** on a real defect, then **reconciled to APPROVE** by fixing it in-pass:

1. **AC-2 (blocking, FIXED in-pass)**: `deny.toml` was invalid TOML — line 32 `skip = []` plus 100 `[[bans.skip]]` array-of-tables entries → "duplicate key `skip`" (confirmed by both Python tomli and Rust `toml` 0.8). The CI `cargo deny check` job would have failed at config load on every run. **Fixed**: removed the stale `skip = []` (and redundant `skip-tree = []`) lines, keeping the 100 curated `[[bans.skip]]` entries. `deny.toml` now PARSE OK (tomli). Note: the 100 `{name, version}` pairs use cargo-deny's deprecated old package-spec format — functional, migration to `{ crate = "name@version", reason = ... }` deferred (non-blocking).
2. **AC-2 (documentation-level, recorded)**: `.cargo/audit.toml` — cargo-audit 0.22.2 `ignore` accepts only plain ID strings (tested: `[[advisories.ignore]]` map form → "expected a string"; `[advisories.ignore]` map form → "expected a sequence"), so reason+expiry are carried as the review contract in comments above each ID with a machine-checked recurring re-review date of 2026-12-31. `cargo audit --deny warnings` re-ran → **EXIT 0** (574 deps / 0 vulns).
3. **AC-4 (FIXED in-pass)**: stale `crates/iris-core/Cargo.toml:43` comment recommended the global-RUSTFLAGS anti-pattern (`CI: RUSTFLAGS="--cfg loom" cargo test --test loom_models`). Corrected to the per-crate invocation + a warning that global `--cfg loom` breaks tokio `net`.
4. **AC-11 (FIXED in-pass)**: stale "still 613/0/1" texts in `PROJECT_GRAPH.yaml` AC-11 and `TEST_001_DESIGN.md` AC-11/DEC-TEST-0012 updated to "pre-existing 613 preserved; workspace now **633/0/1**" (613 + 4 tokio + 10 protocol + 6 golden).
5. **Non-blocking**: loom 3.24s vs verifier 3.28s — run variance, immaterial.

**Final verdict**: **APPROVE** — all AC-1..12 reproduced independently with the fixes above. AC-2 exercised both legs: audit `--deny warnings` exit 0 AND deny.toml now parse-valid (cargo-deny binary env-gated on Windows host, same status as nextest/kani).

## Known limitations carried (node known_limitations)

- G-TI-1..6 (coverage/formal gate, mutation economics, loom soundness bounds, golden-corpus provenance, JVM CI host, mutation-score semantics)
- Env-gated execution legs (nextest/kani/tarpaulin/mutants/jvm on CI runner)
- BLK-0005 (RESOURCE, LOW): physical-device + battery/perf measurements gated on hardware; bench regression gate deferred

## NEXT

ACCEPT (iter ~131): PROJECT_GRAPH TEST-001 status **COMPLETE** (26 total) + evidence + stage_note; PROJECT_STATE completed 25→26, implementing 1→0; CHANGELOG entry; NODE_TRANSITION → IOS-001 (P0 PILOT dependency, DISCOVERED).
