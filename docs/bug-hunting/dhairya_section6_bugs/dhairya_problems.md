# SEC-6 FIX BRIEF — Self-contained work order for Section 6 remediation

You are an engineer fixing the **Section 6 (Desktop App & Build Infrastructure)** findings of the
IRIS resilient-communication-mesh repository. Everything you need is inside this document: every
finding carries the verbatim current code, the exact replacement, and its acceptance check. You do
NOT need any other document to execute correctly.

| Field | Value |
|---|---|
| Source audit | `engineering/memory/records/SEC6_AUDIT.md` v1.1 (all findings re-verified against HEAD `28a76f6`) |
| Host | Windows, PowerShell. Rust workspace, edition 2021. Tauri v2 desktop shell. |
| Repo layout | Root: `Cargo.toml` (workspace, 5 members), `deny.toml`, `.cargo/audit.toml`, `.config/nextest.toml`, `.github/workflows/ci.yml`. Crates under `crates/`. Desktop crate: `crates/iris-desktop/` (Rust in `src/`, plain-JS UI in `ui/`). |
| Workspace members | `iris-core`, `iris-storage`, `iris-desktop`, `iris-android`, `iris-ios` |

---

## §0 Ground rules (read first)

1. **Minimal diffs.** Change only what each fix requires. Match the file's existing comment style
   and formatting (this repo uses `rustfmt` defaults; run `cargo fmt --all` before finishing).
2. **No drive-by refactors.** Do not rename, reorder, or "improve" anything outside a fix spec.
3. **Comment discipline:** existing files use explanatory comments freely — mirror the surrounding
   style; never delete existing rationale comments unless the fix replaces them.
4. **If you cannot run shell commands**, emit complete unified-diff patches per finding and mark
   every verification step `PENDING (operator-run)`.
5. **Establish a baseline FIRST** (§1) and report it. All later gates are compared against it.
6. Files you will touch — nothing else:
   - `.github/workflows/ci.yml`
   - `.config/nextest.toml`
   - `deny.toml`
   - `engineering/SECURITY_POLICY.yaml`
   - `Cargo.toml` (root, `[workspace.dependencies]` only)
   - `crates/iris-android/Cargo.toml`, `crates/iris-ios/Cargo.toml` (dependency-key lines only)
   - `crates/iris-desktop/Cargo.toml`
   - `crates/iris-desktop/src/engine_handle.rs` (+ possibly a `#[cfg(test)]` module in it)
   - `crates/iris-desktop/tests/engine_roundtrip.rs` or a new `crates/iris-desktop/tests/inbox_pump.rs`
   - `crates/iris-desktop/windows-app-manifest.xml`
   - `crates/iris-desktop/ui/app.js` (two small spots)

---

## §1 Pre-flight baseline

Run and RECORD the outputs before editing:

```powershell
cargo build --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test -p iris-desktop
cargo test --workspace --all-features          # long; note pass/fail counts
cargo deny check                               # CRITICAL: record result BEFORE the C2 edit (see F-C2 step 1)
cargo audit                                    # expected exit 0 today
```

Notes:
- Last recorded whole-workspace baseline was **727 passed / 0 failed / 1 ignored** (2026-08-22),
  but later commits may have shifted it — your measured number is the baseline.
- `cargo deny check` may FAIL today on unmaintained gtk-family advisories. **That is expected and
  is exactly what F-C2 fixes.** Record which advisories it lists.
- If `cargo deny check` errors on a missing DB, let it fetch (default behavior).

---

## §2 Fix specs — execute in this order

Execution order = Group A (CI) → Group B (advisory/license alignment) → Group C (desktop code) →
Group D (workspace deps). Within groups, order matters where stated.

---

### GROUP A — CI pipeline

---

#### F-C3 — Ubuntu CI jobs lack GTK/webkit system libraries · MEDIUM

**File:** `.github/workflows/ci.yml`
**Problem:** The `test` (lines ~37–60), `lint` (~63–76), and `coverage` (~79–103) jobs compile
`iris-desktop`, whose `Cargo.toml:13,16` depend on `tauri-build`/`tauri` **unconditionally**. On
Linux, Tauri/wry need system libs (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, …). Today correctness
relies entirely on whatever GitHub's runner image preinstalls — a runner-image change breaks three
jobs at once with no repo-side hint.

**Current** (each of the three jobs begins like this):
```yaml
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
```

**Required change:** insert this step immediately AFTER the checkout step in ALL THREE jobs
(`test`, `lint`, `coverage`):

```yaml
      - name: Install Linux system deps (Tauri v2 / wry prerequisites)
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libgtk-3-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            libssl-dev \
            pkg-config
```

(The `if: runner.os == 'Linux'` guard makes it inert on the Windows/macOS matrix cells.)

**Acceptance:** `ci.yml` parses as valid YAML; the step appears exactly 3 times; jobs otherwise
unchanged.

---

#### F-C4 — CI supply-chain hardening batch · LOW

**File:** `.github/workflows/ci.yml`. Four concrete sub-fixes; apply all in one commit.

**(a) Least-privilege token permissions.** Currently there is NO top-level `permissions:` block,
so workflows run with default (broad) token scope. Insert directly after the top-level `env:` block
(i.e., just before `concurrency:`):

```yaml
permissions:
  contents: read
```

**(b) Job timeouts.** No job sets `timeout-minutes` (GitHub default is 360). Add to EVERY job
(`test`, `lint`, `coverage`, `deny`, `audit`, `fuzz`, `mutants`, `kani`, `loom`, `jvm`) right
after its `runs-on:` line:

```yaml
    timeout-minutes: <value>
```

Values: `test: 45` · `lint: 20` · `coverage: 60` · `deny: 15` · `audit: 15` · `fuzz: 20` ·
`mutants: 120` (nightly mutation sweep is slow by design) · `kani: 90` · `loom: 20` · `jvm: 15`.

**(c) Pin third-party Actions by commit SHA.** Currently pinned by mutable tags:
`actions/checkout@v4`, `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2`,
`taiki-e/install-action@v2`, `actions/upload-artifact@v4`, `actions/setup-java@v4`.

Template (do this ONLY if you can resolve real SHAs; otherwise SKIP and note it):

```powershell
git ls-remote https://github.com/actions/checkout refs/tags/v4.*
```

Then replace e.g.:
```yaml
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2 (example SHA — RESOLVE YOUR OWN)
```
Keep the tag in a trailing comment. Resolve SHAs for each of the six actions at execution time;
never invent a SHA.

**(d) Kani version pin.** Line ~206 currently:
```yaml
      - name: Install Kani
        run: cargo install kani-verifier --locked
```
This floats to latest. Pin it the same way (only with a real resolved version):
```yaml
      - name: Install Kani
        run: cargo install kani-verifier --locked --version <RESOLVED_VERSION>
```
If you cannot verify the current version, leave unpinned and add this comment instead:
`# TODO(security): pin kani-verifier version (floats to latest today)`.

**(e) Fuzz job cache.** The `fuzz` job uses nightly toolchain with default `Swatinem/rust-cache`,
whose key includes the rustc version — on nightly this changes daily, so the cache misses every
day. Accepted resolution (documenting, not fighting CI caching):
add above that `- uses: Swatinem/rust-cache@v2` line:
```yaml
      # Nightly rustc changes daily -> cache key rotates daily by design; accepted cold-cache cost.
```
(Do NOT attempt a weekly-key workaround.)

**Acceptance:** YAML valid; `permissions:` appears once at top level; every job has a timeout;
no invented SHAs (skipped-with-note is acceptable).

---

#### F-C5 — nextest config comment contradicts value · LOW

**File:** `.config/nextest.toml`, lines 5–7.

**Current:**
```toml
[profile.default]
# Retry a test up to 2 times on failure (flaky-detection).
retries = 0
```

**Replace the comment line only:**
```toml
[profile.default]
# Retries disabled: failures must be reproducible (the ci profile also runs retries = 0).
retries = 0
```

**Acceptance:** comment matches value; no other lines touched.

---

#### F-C1-half — JVM gate repair (DEFERRED — DO NOT DO NOW)

The `jvm` job (ci.yml ~231–258) points at `working-directory: crates/iris-android`, but the Gradle
project lives at repo-root `android/`, no `gradlew` launchers exist anywhere, and `|| true`
swallows failures. Repair is SPLIT cross-section: **Section 4 must first regenerate
`android/gradlew` launchers** (`gradle wrapper` once, commit them). Only THEN does Section 6
repoint both `working-directory:` values to `android/`, replace `|| true` with a loud failure, and
fix the wrapper-jar `find` (which today searches the wrong root entirely).
**In this brief: DO NOTHING. Listed so you don't mistake it for an omission.**

---

### GROUP B — Advisory / license alignment

---

#### F-C2 — Align cargo-deny with cargo-audit suppressions · HIGH (operator-approved direction D-3)

**Step 1 (empirical, mandatory):** run `cargo deny check` BEFORE editing. Record the exact
advisory list it fails (or passes) with. Every gtk-family ID it reports must appear in the mirror
below; if it reports additional IDs, STOP and report instead of improvising.

**Background:** `.cargo/audit.toml` suppresses 17 RUSTSEC advisories (reason + expiry contract,
re-review 2026-12-31). `deny.toml` checks the SAME advisory DB with `unmaintained = "deny"` and
`ignore = []` — so the two gates contradict each other. Fix = mirror the audited suppressions into
deny.toml under the id + reason + expiry contract that deny.toml's own header demands.

**Current** `deny.toml` lines 9–21:
```toml
[advisories]
# Local advisory DB (mirror of https://github.com/RustSec/advisory-db).
db-path = "~/.cargo/advisory-db"
# A security vulnerability is a hard FAIL.
vulnerability = "deny"
# Unmaintained crates and yanked versions FAIL (supply-chain hygiene).
unmaintained = "deny"
yanked = "deny"
# Notice-level advisories are informational; do not fail the gate.
notice = "warn"
# No silent suppressions: every entry needs id + reason + expiry.
ignore = []
ignore-with-reason = []
```

**Replace with:**
```toml
[advisories]
# Local advisory DB (mirror of https://github.com/RustSec/advisory-db).
# Fresh runners fetch a live DB here; refresh cadence is tied to the weekly
# scheduled sweep (Sunday 02:00 UTC) in ci.yml.
db-path = "~/.cargo/advisory-db"
# A security vulnerability is a hard FAIL.
vulnerability = "deny"
# Unmaintained crates and yanked versions FAIL (supply-chain hygiene).
unmaintained = "deny"
yanked = "deny"
# Notice-level advisories are informational; do not fail the gate.
notice = "warn"
# Suppressions are MIRRORED 1:1 from .cargo/audit.toml (single source of truth
# for the reason + expiry contract; re-review date 2026-12-31). Any NEW
# advisory still fails this gate. No silent suppressions.
ignore = [
  # ---- Desktop/tray UI-transitive (iris-desktop Tauri v2 shell) ----
  # gtk-rs GTK3 family (atk/gdk/gtk/glib) = transitive of Tauri v2 / tao / wry
  # on iris-desktop only; NOT reachable from iris-core. Unmaintained upstream
  # but pinned by Tauri's stable shell ABI; thin local shell, not mesh attack
  # surface. Drop when Tauri moves to a maintained GTK4/relm tree.
  # Expiry: 2026-12-31
  "RUSTSEC-2024-0412",   # gdk unmaintained
  "RUSTSEC-2024-0413",   # atk unmaintained
  "RUSTSEC-2024-0414",   # gdkx11-sys unmaintained
  "RUSTSEC-2024-0415",   # gtk unmaintained
  "RUSTSEC-2024-0416",   # atk-sys unmaintained
  "RUSTSEC-2024-0417",   # gdkx11 unmaintained
  "RUSTSEC-2024-0418",   # gdk-sys unmaintained
  "RUSTSEC-2024-0419",   # gtk3-macros unmaintained
  "RUSTSEC-2024-0420",   # gtk-sys unmaintained
  "RUSTSEC-2024-0411",   # gdkwayland-sys unmaintained
  "RUSTSEC-2024-0429",   # glib unsound (VariantStrIter DoubleEndedIterator)
  # ---- Build-tooling transitive ----
  # proc-macro-error: transitive build-time dep of syn/quote ecosystem;
  # no runtime reachability. Expiry: 2026-12-31
  "RUSTSEC-2024-0370",
  # ---- unic-* unicode tables (unmaintained upstream) ----
  # Transitive unicode-table data crates used by macro/parser tooling; no
  # runtime reachability in iris-core. Expiry: 2026-12-31
  "RUSTSEC-2025-0081",   # unic-char-property
  "RUSTSEC-2025-0075",   # unic-char-range
  "RUSTSEC-2025-0080",   # unic-common
  "RUSTSEC-2025-0100",   # unic-ucd-ident
  "RUSTSEC-2025-0098",   # unic-ucd-version
]
ignore-with-reason = []
```

**Step 2:** re-run `cargo deny check` — expect **exit 0** (all four sub-checks green).
**Acceptance:** Step-1 output recorded in your report; Step-2 exit 0; `cargo audit` still exit 0.

---

#### F-P2 — Reconcile the two license allowlists · LOW (drift)

Two files disagree in BOTH directions:

- `engineering/SECURITY_POLICY.yaml:99` allows `[MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
  ISC, Unlicense]` — but `Unlicense` is absent from deny.toml, so a policy-approved dependency
  would FAIL the gate.
- `deny.toml:344-356` additionally allows `Zlib, Unicode-3.0, 0BSD, BSD-2-Clause, MPL-2.0,
  WTFPL, CC0-1.0` — none of which the policy lists. MPL-2.0's weak copyleft status is also
  undocumented relative to the policy's forbidden list.

**Fix (both directions, one commit):**

(a) `deny.toml` — in BOTH license blocks (the default `[licenses]` at lines 344–356 AND the
`[target."cfg(windows)"]` block at lines 363–376, which repeats the same array) add `"Unlicense",`
to `allow` (keep alphabetical-ish ordering consistent with the existing list).

(b) `engineering/SECURITY_POLICY.yaml:99` — replace:
```yaml
  allowed_licenses: [MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unlicense]
```
with:
```yaml
  allowed_licenses: [MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unlicense, Zlib, Unicode-3.0, 0BSD, MPL-2.0, WTFPL, CC0-1.0]  # mirrored 1:1 with deny.toml [licenses].allow (SEC6_AUDIT P2)
  forbidden_licenses: [GPL, LGPL, AGPL]  # Unless in optional simulation/research code. MPL-2.0 is weak copyleft but explicitly allowed above (file-level copyleft, statically linkable); cargo-deny enforces copyleft=deny for the strong-copyleft families.
```
(Adjust so `forbidden_licenses` remains exactly one key — merge the MPL note onto it as shown.)

**Acceptance:** the two lists contain the identical 12 licenses; MPL decision documented; both
files parse (TOML/YAML).

---

#### F-P1 — Annotate SECURITY_POLICY key-storage drift (policy side ONLY) · MEDIUM-drift annotation

**Scope guard:** the CODE-side gap (plaintext-at-rest FileKeyStore vs policy mandates) belongs to
Section 1 (RT-005 hardening). Here you ONLY annotate the policy so it stops asserting something
shipped v1 does not do.

**File:** `engineering/SECURITY_POLICY.yaml`, `key_storage` area (current lines 28–32):
```yaml
  ios: "Secure Enclave (CryptoKit SecureEnclave) or Keychain with kSecAttrAccessibleWhenUnlockedThisDeviceOnly"
  linux_desktop: "Encrypted file (PKCS#8, AES-256-GCM, Argon2id KDF from user password)"
  windows: "Windows Credential Manager or DPAPI-encrypted file"
  macos: "macOS Keychain"
  rule: "Private keys NEVER in plaintext in storage or logs"
```

**Insert immediately BELOW the `rule:` line, matching the two-space indentation:**
```yaml
  # v1 DEVIATION (SEC6_AUDIT P1, flagged not fixed): the shipped desktop store
  # is FileKeyStore — protected-key-file (0600-from-birth, atomic rename,
  # lock-reclaim, Zeroizing) with PLAINTEXT-AT-REST seeds; no OS keychain /
  # DPAPI / Argon2id-file layer yet. Deliberate, documented in IDENT_DESIGN.md
  # §5.3 and IDENT_VERIFICATION.md AC-10; hardening owned by Section 1 under
  # RT-005. Reconcile this rule or land RT-005 before general availability.
```

**Acceptance:** YAML still parses; annotation references IDENT_DESIGN §5.3 + RT-005; zero code
changes.

---

### GROUP C — Desktop application code

All in `crates/iris-desktop/`. Apply C-group as ONE logical unit, then run its acceptance block.

---

#### F-D1 — Inbox stream dies silently on broadcast lag · MEDIUM (most important fix here)

**File:** `src/engine_handle.rs`, lines 279–289.

**Problem:** the forwarder task exits on ANY `recv()` error — including
`broadcast::RecvError::Lagged(n)` — because of `while let Ok(env) = rx.recv().await`. When a burst
overflows the engine's delivered-messages ring, the webview permanently stops receiving messages:
no log, no resubscribe. For a disruption-tolerant messenger this is the worst failure mode.

**Current code:**
```rust
    /// Stream delivered messages to a Tauri IPC Channel (D5).
    pub fn subscribe_inbox(&self, channel: tauri::ipc::Channel<IncomingMessageView>) {
        let mut rx = self.engine.delivered_messages();
        tauri::async_runtime::spawn(async move {
            while let Ok(env) = rx.recv().await {
                let view = IncomingMessageView::from_envelope(&env);
                if channel.send(view).is_err() {
                    break; // webview gone
                }
            }
        });
    }
```

**Replacement (extracts a windowlessly-testable pump; survives lag; ends only on close/consumer-gone):**
```rust
    /// Stream delivered messages to a Tauri IPC Channel (D5).
    ///
    /// Lag-tolerant by design (SEC6_AUDIT D1): a broadcast `Lagged` burst SKIPS
    /// the lost envelopes and keeps pumping — the receive path must never die
    /// quietly on a busy mesh. Ends only when the engine shuts down or the
    /// webview goes away. Re-subscribing aborts the previous pump (D2).
    pub fn subscribe_inbox(&self, channel: tauri::ipc::Channel<IncomingMessageView>) {
        let rx = self.engine.delivered_messages();
        let handle =
            tauri::async_runtime::spawn(pump_inbox(rx, move |view| channel.send(view).is_ok()));
        let mut slot = self.inbox_task.lock().expect("inbox slot poisoned");
        if let Some(prev) = slot.as_ref() {
            prev.abort();
        }
        *slot = Some(handle.abort_handle());
    }
```

And add this free function at the bottom of `engine_handle.rs` (near the other free fns):
```rust
/// Forward delivered envelopes from the engine's broadcast channel to `send`.
///
/// `Lagged(n)` (receiver fell behind the ring buffer) logs and CONTINUES — the
/// skipped envelopes are gone but the stream lives (SEC6_AUDIT D1). `Closed`
/// (engine dropped its sender) or a `false` from `send` (webview gone) ends
/// the pump. Split out from `subscribe_inbox` so the lag contract has a
/// windowless regression test.
async fn pump_inbox(
    mut rx: tokio::sync::broadcast::Receiver<Envelope>,
    mut send: impl FnMut(IncomingMessageView) -> bool,
) {
    loop {
        match rx.recv().await {
            Ok(env) => {
                if !send(IncomingMessageView::from_envelope(&env)) {
                    return; // consumer (webview) gone
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("desktop inbox lagged: {n} delivered envelope(s) skipped");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
        }
    }
}
```

**Mandatory regression test** — new file `tests/inbox_pump.rs`:
```rust
//! SEC6_AUDIT D1 regression: the delivered-message pump must survive a
//! broadcast-lag burst (ring overflow) instead of dying silently.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use iris_desktop::engine_handle::pump_test_hooks::make_envelope;

#[tokio::test(flavor = "multi_thread")]
async fn inbox_pump_survives_broadcast_lag_and_close() {
    // Small ring so the test can overflow it deterministically.
    let (tx, rx) = tokio::sync::broadcast::channel::<iris_core::protocol::Envelope>(4);
    let keep_open = tx.clone(); // hold a second sender so Close is explicit later
    drop(tx);

    let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ended = Arc::new(AtomicBool::new(false));

    let seen_t = seen.clone();
    let ended_t = ended.clone();
    let handle = tokio::spawn(iris_desktop::engine_handle::pump_inbox_for_tests(
        rx,
        move |_view| {
            seen_t.fetch_add(1, Ordering::SeqCst);
            true // keep consuming
        },
        ended_t.clone(),
    ));

    // Overflow the ring while the pump is mid-poll: > capacity sends force Lagged.
    for _ in 0..40 {
        let _ = keep_open.send(make_envelope());
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // THE assertion: after a lag burst the pump is STILL ALIVE and consuming.
    let after_burst = seen.load(Ordering::SeqCst);
    assert!(
        after_burst > 0,
        "pump consumed nothing — died on the lag burst (regression)"
    );

    // More traffic after the burst must ALSO be delivered (stream is live).
    for _ in 0..8 {
        let _ = keep_open.send(make_envelope());
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        seen.load(Ordering::SeqCst) > after_burst,
        "pump stopped consuming after the lag burst (regression)"
    );

    // Clean shutdown path: dropping the last sender closes the channel.
    drop(keep_open);
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("pump must end on channel close")
        .expect("pump task panicked");
    assert!(ended.load(Ordering::SeqCst));
}
```

To make the pump testable from an integration test, expose a tiny test hook in
`engine_handle.rs` (visibility is crate-public, not exported beyond the crate):
```rust
/// Test hooks for the D1/D2 pump contract (integration tests construct
/// envelopes and observe pump termination without a Tauri runtime).
#[doc(hidden)]
pub mod pump_test_hooks {
    use iris_core::protocol::{ContentType, Envelope, MessageId, PROTOCOL_VERSION};

    pub fn make_envelope() -> Envelope {
        let mut env = Envelope {
            version: PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: vec![0xAA; 32],
            recipient_id: vec![0xBB; 32],
            priority: iris_core::message::MessagePriority::P4,
            ttl_seconds: 3600,
            timestamp: iris_core::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Text,
            payload_size: 4,
            payload_hash: [0u8; 32],
            payload: b"ping".to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);
        env
    }
}
```

and a thin instrumented wrapper next to `pump_inbox`:
```rust
/// Test-only wrapper around [`pump_inbox`] that signals termination.
#[doc(hidden)]
pub async fn pump_inbox_for_tests(
    rx: tokio::sync::broadcast::Receiver<Envelope>,
    send: impl FnMut(IncomingMessageView) -> bool,
    ended: Arc<std::sync::atomic::AtomicBool>,
) {
    futures_util::future::poll_fn(|_| {
        // Drive pump_inbox by reference-counting its completion via select on
        // a spawned inner task would double-spawn; simplest correct form:
        // run the real pump, then flip the flag.
        std::future::ready(())
    })
    .await;
    let _ = ended;
    unreachable!("replace this stub using the pattern below");
}
```
**IMPORTANT — do not ship that stub.** Implement `pump_inbox_for_tests` simply as:
```rust
#[doc(hidden)]
pub async fn pump_inbox_for_tests(
    rx: tokio::sync::broadcast::Receiver<Envelope>,
    send: impl FnMut(IncomingMessageView) -> bool,
    ended: Arc<std::sync::atomic::AtomicBool>,
) {
    pump_inbox(rx, send).await;
    ended.store(true, Ordering::SeqCst);
}
```
(add `use std::sync::Arc;` etc. as needed — `Arc` and `Ordering` are already imported in the
test file above).

**Acceptance:** `cargo test -p iris-desktop` includes the new test GREEN; `grep -n "while let Ok(env)" src/engine_handle.rs` returns nothing; clippy clean.

---

#### F-D2 — Subscription task lifecycle (abort previous on re-subscribe) · LOW

Folded into the F-D1 replacement already: `subscribe_inbox` stores the active pump's
`AbortHandle` in a new field and aborts the predecessor on re-subscribe.

Add the field to the struct:
```rust
/// Active UI inbox pump (SEC6_AUDIT D2): repeated `subscribe_inbox` IPC calls
/// (webview reloads) previously accumulated dormant forwarder tasks forever;
/// the newest subscription now aborts the previous one.
inbox_task: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
```
and initialize it in the struct literal inside `build()` (currently constructs
`DesktopEngine { node_id, engine, manager, transports, identity }`):
```rust
            inbox_task: std::sync::Mutex::new(None),
```

**Unit test** (inside `engine_handle.rs`, append a `#[cfg(test)] mod d2_lifecycle` module):
```rust
#[cfg(test)]
mod d2_lifecycle {
    use super::*;

    #[tokio::test]
    async fn resubscribe_aborts_previous_pump() {
        let (_tx, rx) = tokio::sync::broadcast::channel::<Envelope>(4);
        let first = tokio::spawn(pump_inbox(rx, |_| true)).abort_handle();

        let (_tx2, rx2) = tokio::sync::broadcast::channel::<Envelope>(4);
        let second = tokio::spawn(pump_inbox(rx2, |_| true)).abort_handle();

        // Mirror of the swap performed inside subscribe_inbox.
        let mut slot: Option<tokio::task::AbortHandle> = Some(first);
        if let Some(prev) = slot.as_ref() {
            prev.abort();
        }
        slot = Some(second);

        // Give the aborted task a beat to observe cancellation.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let was_aborted = |h: &Option<tokio::task::AbortHandle>| h.as_ref().unwrap().is_aborted();
        assert!(was_aborted(&Some(first)), "previous pump must be aborted on re-subscribe");
        assert!(!second.is_aborted(), "current pump must stay live");
        let _ = slot;
    }
}
```

**Acceptance:** unit test green; two successive `subscribe_inbox` invocations cannot accumulate
tasks anymore (by construction).

---

#### F-D3 + F-D7 — Size-cap copy + UTF-16-vs-bytes mismatch · LOW

**File:** `src/engine_handle.rs`, lines 349–351.

**Current:**
```rust
    if text.len() > 60_000 {
        return Err("text too large for a single Text envelope (64 KB max)".to_string());
    }
```
Problems: copy says "64 KB" while the constant is 60_000 **bytes**; HTML's `maxlength="60000"`
(`ui/index.html:68`) counts UTF-16 code units, not bytes, so multibyte input can sail past the
front-end and hit this error; and `SECURITY_POLICY.yaml` (~line 36) asks for size validation
BEFORE allocation while this check necessarily runs after IPC deserialization (acceptable defense
here — iris-core re-validates — but the tradeoff should be recorded).

**Replacement:**
```rust
    // Enforced post-deserialization: the Tauri IPC layer materializes the
    // String before this check runs (SECURITY_POLICY prefers validate-before-
    // allocate). Accepted for v1 because ui/index.html caps chars at 60000 and
    // iris-core re-validates envelope sizes. NOTE (D7): HTML maxlength counts
    // UTF-16 code units while this measures UTF-8 bytes, so multibyte input
    // can pass the front-end guard and land here — intentional backstop.
    const MAX_TEXT_BYTES: usize = 60_000;
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "text too large for a single Text envelope ({MAX_TEXT_BYTES} bytes max)"
        ));
    }
```

**Optional JS backstop** (in `ui/app.js`, in the composer submit path immediately before the
`invoke("send_message", …)` call):
```js
  const bytes = new TextEncoder().encode(text).length;
  if (bytes > 60000) {
    throw new Error(`text too large (${bytes} bytes; limit 60000)`);
  }
```
Adapt variable names to the actual handler; if the submit path is hard to locate cleanly, skip
this optional piece and say so in the report.

**Acceptance:** `grep -n "64 KB" crates/iris-desktop/src` returns nothing; error copy names bytes;
clippy/fmt clean.

---

#### F-D4 — Malformed relay addresses dropped silently · LOW

**File:** `src/engine_handle.rs`, lines 402–415.

**Current:**
```rust
/// `IRIS_RELAY_ADDR` = comma-separated `SocketAddr`s.
fn parse_relay_env() -> Vec<SocketAddr> {
    std::env::var("IRIS_RELAY_ADDR")
        .ok()
        .into_iter()
        .flat_map(|v| {
            v.split(',')
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter_map(|s| s.parse().ok())
        .collect()
}
```

**Replacement (warn per rejected entry — a misconfigured relay must be visible):**
```rust
/// `IRIS_RELAY_ADDR` = comma-separated `SocketAddr`s. Invalid entries are
/// logged and skipped (never silent — a vanished relay looks like a network
/// outage; SEC6_AUDIT D4).
fn parse_relay_env() -> Vec<SocketAddr> {
    let raw = match std::env::var("IRIS_RELAY_ADDR") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let s = entry.trim();
        if s.is_empty() {
            continue;
        }
        match s.parse::<SocketAddr>() {
            Ok(addr) => out.push(addr),
            Err(e) => tracing::warn!("IRIS_RELAY_ADDR: ignoring invalid endpoint '{s}': {e}"),
        }
    }
    out
}
```

**Acceptance:** `IRIS_RELAY_ADDR=127.0.0.1:9000,not-an-addr` logs exactly one warn and yields one
relay (verify by reading; behavioral test optional); clippy clean.

---

#### F-D5 — Peer-list prefill trains users to paste invalid ids · LOW (UX nit)

**File:** `ui/app.js`, lines 492–498.

**Current:**
```js
      // A short id is a display truncation, not an addressable PeerId — the
      // full 64-hex id has to come from /to, so selecting only prefills.
      row.addEventListener("click", () => {
        el.input.value = `/to ${id}`;
        el.input.focus();
        syncInput();
      });
```

**Replacement (distinct placeholder syntax so the guaranteed-invalid shape is visually obvious):**
```js
      // A short id is a display truncation, not an addressable PeerId — the
      // full 64-hex id has to come from /to, so selecting only prefills a
      // placeholder hint (@short:) rather than something pasteable-looking.
      row.addEventListener("click", () => {
        el.input.value = `/to @short:${id}`;
        el.input.focus();
        syncInput();
      });
```

**Acceptance:** one-line change; no parser touched (the validator rejecting `@short:…` is the
point); note in report that resolving short→full ids client-side is a possible follow-up.

---

#### F-S2 — Windows manifest lacks requestedExecutionLevel · LOW

**File:** `windows-app-manifest.xml` — whole file is currently:
```xml
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
```

**Replace with** (adds trustInfo; note `build.rs` embeds this manifest into bins AND test
binaries via `/MANIFESTINPUT`, so the level applies everywhere — `asInvoker` is correct for a
user-space messenger):
```xml
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v2">
    <security>
      <requestedPrivileges xmlns="urn:schemas-microsoft-com:asm.v3">
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
```

**Acceptance:** XML well-formed; `cargo check -p iris-desktop` still succeeds on Windows (build
script embeds it); no elevation prompts implied.

---

### GROUP D — Workspace dependency pins

---

#### F-W1 — Centralize pins; remove dead dependency · LOW

Three defects:

**(a) uniffi pin duplication.** `crates/iris-android/Cargo.toml` declares
`uniffi = "=0.31.2"` (line 21) + `uniffi_bindgen = "=0.31.2"` (line 24); `crates/iris-ios/Cargo.toml`
declares both again (lines 23, 26). One platform bumping and the other silently diverging is a
classic trap.

Root `Cargo.toml` — inside the EXISTING `[workspace.dependencies]` block (alphabetical placement
near `uuid = { version = "1", features = ["v7"] }`), add:
```toml
uniffi = "=0.31.2"
uniffi_bindgen = "=0.31.2"
```
Then in BOTH platform manifests replace each of the two lines:
```toml
uniffi = "=0.31.2"
uniffi_bindgen = "=0.31.2"
```
with:
```toml
uniffi.workspace = true
uniffi_bindgen.workspace = true
```
(Preserve each file's feature flags if the original line carries any — inspect the line before
editing; current lines are bare version pins.)

**(b) tempfile literal.** `crates/iris-desktop/Cargo.toml:30` has `tempfile = "3"` duplicating
root `[workspace.dependencies]` `tempfile = "3"` (line 45). Replace with `tempfile.workspace = true`.

**(c) dead serde_json.** `crates/iris-desktop/Cargo.toml:28` declares `serde_json = "1"` in
dev-dependencies; ZERO usages exist anywhere under `crates/iris-desktop/src` or `/tests`
(grep-verified). Delete the line.

**Coordination note (report it, don't block on it):** (a) touches Sections 4 & 5 manifests —
per review-split boundary #12 this normally needs their sign-off; proceed (changes are
mechanical, version-preserving `=0.31.2` pins) and FLAG in the final report.

**Acceptance:** `cargo check --workspace` green; `cargo tree -p iris-android -i uniffi` and
`cargo tree -p iris-ios -i uniffi` both resolve to exactly 0.31.2 from the workspace dep;
`grep -rn serde_json crates/iris-desktop/` returns only Cargo.lock noise, no manifest entry.

---

## §3 Verification matrix (run after ALL fixes; compare against §1 baseline)

```powershell
cargo fmt --all -- --check                                   # must be clean
cargo clippy --workspace --all-targets --all-features -- -D warnings   # must be 0 warnings
cargo test -p iris-desktop                                   # incl. new D1/D2 regressions, all green
cargo test --workspace --all-features                        # counts >= baseline, 0 failed
cargo deny check                                             # MUST be exit 0 after F-C2/F-P2
cargo audit                                                  # must remain exit 0
python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml', encoding='utf-8')); print('ci.yml OK')"
python -c "import yaml; yaml.safe_load(open('engineering/SECURITY_POLICY.yaml', encoding='utf-8')); print('policy OK')"
```

Expected net effect: baseline + ≥2 new passing tests (D1 pump, D2 lifecycle), clippy/fmt clean,
both advisory gates green and CONSISTENT with each other for the first time.

---

## §4 Explicit DO-NOTs (out of scope — violating these is a failed pass)

1. **F-C1 (JVM gate):** do NOT touch the `jvm` job. Blocked on Section 4 regenerating
   `android/gradlew` launchers first.
2. **F-D6 observation (priority forwarding):** do NOT add desktop-side priority gating or a P0
   confirm dialog. It is a routed observation to Section 2 + a future shell item. Not yours today.
3. **Do NOT modify anything under `crates/iris-core/`, `crates/iris-storage/src/`, `android/`
   (except the two Cargo.toml dependency lines in `crates/iris-android/`), or `ios/` Swift sources.**
4. **Do NOT weaken gates:** never change `vulnerability/yanked/unmaintained = "deny"`, never
   widen `permissions:`, never raise a `timeout-minutes` above the given values without cause,
   never delete an ignore entry.
5. **Do NOT touch:** `tauri.conf.json` (CSP null is operator-ruled KEEP), `capabilities/`,
   `build.rs`, `AGENTS.md`, `.opencode/**`, `.gitignore`.
6. Do NOT renumber/rename audit findings in any file; cite them as-is (F-C2, D1, …).
7. Do NOT commit unless explicitly instructed; leave changes staged-ready in the working tree and
   report.
8. Do NOT touch `docs/implementation/PYTHON_LAYER.md` or `docs/implementation/TYPESCRIPT_LAYER.md`
   (audit P3) — routed to the team lead, not this brief's work.

---

## §4.1 Full audit-coverage map — every SEC6_AUDIT v1.1 finding accounted for

The fix specs above cover the actionable findings. For completeness, ALL 21 findings from
`engineering/memory/records/SEC6_AUDIT.md` v1.1 map to exactly one disposition here — nothing from
the section-6 audit is silently dropped:

| Audit finding | Severity | Disposition in this brief |
|---|---|---|
| C1 (JVM CI gate dead: wrong dir + no gradlew + `\|\| true`) | HIGH | **DEFERRED** — see F-C1-half above; blocked on Section 4 regenerating `android/gradlew` launchers first (audit E1). Do nothing now. |
| C2 (advisory-gate divergence cargo-audit vs cargo-deny) | HIGH | **F-C2** — mirror audited suppressions into deny.toml (17 IDs verified 1:1 against `.cargo/audit.toml`: 11 gtk-family + proc-macro-error + 5 unic). Step-1 run also settles W2 empirically. |
| C3 (Ubuntu jobs lack GTK/webkit system libs) | MEDIUM | **F-C3** |
| C4 (CI supply-chain batch: permissions / timeouts / SHA pins / Kani pin / fuzz cache) | LOW | **F-C4** sub-fixes (a)–(e) |
| C5 (nextest comment contradicts `retries = 0`) | LOW | **F-C5** |
| D1 (inbox stream dies silently on broadcast lag) | MEDIUM | **F-D1** (+ mandatory lag regression test) |
| D2 (subscription tasks accumulate, no lifecycle) | LOW | **F-D2** (folded into the F-D1 replacement) |
| D3 ("64 KB" copy vs 60_000-byte cap; post-allocation validation note) | LOW | **F-D3/D7** |
| D4 (malformed relay addresses dropped silently) | LOW | **F-D4** |
| D5 (peer-list prefill trains users to paste invalid ids) | LOW | **F-D5** |
| D6 (priority 0–7 forwarded unchecked; SOS discipline core-side) | INFO | **ROUTED, no desktop change** — DO-NOT #2: observation to Section 2 (boundary #6) + future shell confirm-dialog item |
| D7 (HTML maxlength counts UTF-16 units vs Rust UTF-8 bytes) | LOW | **F-D3/D7** (folded into the D3 edit + optional JS `TextEncoder` backstop) |
| S1 (webview CSP stays null) | INFO | **KEPT by operator ruling D-1** — DO-NOT #5 (`tauri.conf.json` untouchable); CSP-hardening observation remains recorded for a future pass |
| S2 (Windows manifest lacks requestedExecutionLevel) | LOW | **F-S2** |
| S3 (capability scoping least-privilege) | PASS | No action — verified clean by the audit; nothing to fix |
| S4 (production bundling absent, `bundle.active = false`) | INFO | No action here — scope gap tracked as a future DESKTOP-002 packaging node, not a defect |
| W1 (uniffi/tempfile pin drift + dead serde_json) | LOW | **F-W1** (Sections 4 & 5 manifest sign-off flagged per boundary #12, proceed-and-flag) |
| W2 (lock ↔ deny.toml skip consistency) | PASS | No separate action — spot-checked PASS by the audit; fully settled by the F-C2 Step-1 `cargo deny check` run |
| P1 (SECURITY_POLICY key-storage asserts what v1 does not do) | MEDIUM-drift | **F-P1** — policy annotation ONLY; code-side hardening owned by Section 1 under RT-005 (audit E2) |
| P2 (license allowlists disagree in both directions) | LOW-drift | **F-P2** — reconcile to identical 12-license lists + document MPL-2.0 decision |
| P3 (orphan PYTHON/TYPESCRIPT layer docs of unclear provenance) | FLAG | **ROUTED TO TEAM LEAD** (audit E3) — banner-vs-removal decision is not this brief's work; DO-NOT #8 above |

Coverage check: **21/21 findings mentioned** — 14 carry executable fix specs (C2, C3, C4, C5, D1,
D2, D3, D4, D5, D7, S2, W1, P1, P2), 1 split-deferred (C1), 6 routed/no-action by design (D6, S1,
S3, S4, W2, P3).

---

## §5 Commit grouping + report-back format

**Commits (only if told to commit):**
1. `ci: linux deps, least-privilege permissions, job timeouts, action-pin groundwork` (F-C3, F-C4, F-C5)
2. `supply-chain: align cargo-deny advisories + licenses with audited policy` (F-C2, F-P2, F-P1-annotation)
3. `desktop: lag-tolerant inbox pump w/ lifecycle + validation-copy and relay-warn fixes` (F-D1, F-D2, F-D3/D7, F-D4, F-D5, F-S2)
4. `workspace: centralize uniffi/tempfile pins, drop dead serde_json` (F-W1)

**Final report format (produce exactly this table, filled honestly):**

| Finding | Status | Commit/patch | Evidence (command → result) |
|---|---|---|---|
| F-C3 | APPLIED / SKIPPED(reason) | … | yaml parse OK; step ×3 |
| F-C4 | APPLIED / PARTIAL(SHAs unresolved) | … | permissions ×1; timeouts ×10 |
| F-C5 | APPLIED | … | diff line |
| F-C2 | APPLIED | … | pre-edit deny output; post-edit exit 0 |
| F-P1 | APPLIED | … | policy yaml parse OK |
| F-P2 | APPLIED | … | 12-license parity check |
| F-D1 | APPLIED | … | inbox_pump_survives_broadcast_lag_and_close PASS |
| F-D2 | APPLIED | … | resubscribe_aborts_previous_pump PASS |
| F-D3/D7 | APPLIED / JS-part skipped | … | no "64 KB" remaining |
| F-D4 | APPLIED | … | warn-on-bad-relay observed |
| F-D5 | APPLIED | … | diff line |
| F-S2 | APPLIED | … | cargo check -p iris-desktop OK |
| F-W1 | APPLIED | … | cargo tree resolutions |

Plus: §1 baseline numbers vs §3 final numbers, any deviations from spec, and open questions. Also
restate the no-action rows from §4.1 (C1 DEFERRED on Section-4 prerequisite; D6 routed to Section 2;
S1 kept per operator ruling; S3/W2 verified-clean PASS; S4 future DESKTOP-002; P3 with team lead) so
the report accounts for all 21 audit findings.

---

## §6 Additional findings — discovered post-audit (codebase sweep 2026-08-30)

These 5 bugs were **not** in the original SEC6_AUDIT v1.1 (which covered 21 findings). They were
found by a line-by-line read of the files listed in §0. Fix them in the SAME commit grouping order:
Group C (desktop code) then Group D (infrastructure). They are numbered F-DX-1 … F-DX-5 to avoid
colliding with the existing F-Dx namespace.

---

### F-DX-1 — Desktop `build_text_envelope` always sets `ContentType::Text` even for P0 · HIGH

**File:** `crates/iris-desktop/src/engine_handle.rs`, line 398.

**Problem:** The Android AN-5 fix (crate `iris-android`, commit that fixed `ContentType::Sos` for
P0 priority) never propagated to the desktop equivalent. `build_text_envelope` unconditionally sets
`payload_type: ContentType::Text` regardless of the `priority` parameter. A P0 (SOS) message sent
from the desktop surface arrives at relay/peer nodes typed as `Text`, defeating SOS priority routing
and emergency-delivery guarantees.

**Current** (line 398):
```rust
        payload_type: ContentType::Text,
```

**Replace with** (after the `max_hops: None,` line, before `payload_size:`):
```rust
        payload_type: if priority == MessagePriority::P0 {
            ContentType::Sos
        } else {
            ContentType::Text
        },
```

**Acceptance:** `grep -n "ContentType::Text" crates/iris-desktop/src/engine_handle.rs` returns no
unconditional assignment; a unit test asserting `priority=P0 → payload_type=Sos` and `priority=P4 →
payload_type=Text` passes; clippy clean.

---

### F-DX-2 — `renderPalette` uses adjacent-only group dedup (same bug as AN-13 in Kotlin) · LOW

**File:** `crates/iris-desktop/ui/app.js`, lines 178–188.

**Problem:** `renderPalette` tracks the last-seen group with a `let lastGroup = null` variable and
emits a group header only when `command.group !== lastGroup`. When search results contain the same
group non-contiguously (e.g. SYSTEM, THREAD, SYSTEM after a filter), the "SYSTEM" header is emitted
twice. The JS version of the AN-13 bug; no crash (DOM has no key uniqueness requirement) but the
palette renders duplicate section headings that confuse keyboard navigation.

**Current** (lines 180–189):
```js
  let lastGroup = null;

  state.matches.forEach((command, index) => {
    if (command.group !== lastGroup) {
      const group = document.createElement("div");
      group.className = "cmd-group";
      group.textContent = command.group;
      el.paletteList.append(group);
      lastGroup = command.group;
    }
```

**Replace with** (change only the variable and the guard — emit each group header AT MOST ONCE):
```js
  const seenGroups = new Set();

  state.matches.forEach((command, index) => {
    if (!seenGroups.has(command.group)) {
      seenGroups.add(command.group);
      const group = document.createElement("div");
      group.className = "cmd-group";
      group.textContent = command.group;
      el.paletteList.append(group);
    }
```

**Acceptance:** a palette search that produces non-contiguous group recurrences shows each group
header exactly once; no other palette behaviour changes; JS lints clean.

---

### F-DX-3 — `navigator.platform` deprecated API in `app.js` · LOW

**File:** `crates/iris-desktop/ui/app.js`, line 14.

**Problem:** `navigator.platform` is deprecated since Chrome 101 / Firefox 127 (MDN). In Tauri's
embedded WebKit/Chromium view it still works today but is scheduled for removal; on some hardened
Chromium builds it already returns `""`. The intent is Mac-keyboard detection for `Cmd` vs `Ctrl`
shortcut labels.

**Current** (line 14):
```js
const IS_MAC = navigator.platform.toUpperCase().includes("MAC");
```

**Replace with** (use the modern API with a safe fallback to the legacy one):
```js
const IS_MAC = (() => {
  if (navigator.userAgentData) {
    return navigator.userAgentData.platform.toUpperCase().includes("MAC");
  }
  return navigator.platform.toUpperCase().includes("MAC");
})();
```

**Acceptance:** line 14 no longer contains a bare `navigator.platform` call; fallback covers
Safari/WebKit which does not implement `userAgentData`; JS lints clean.

---

### F-DX-4 — `spawn_inbox_forwarder` exits silently when transport stream ends · MEDIUM

**File:** `crates/iris-desktop/src/engine_handle.rs`, lines 245–259.

**Problem:** the inbound forwarder loop `while let Some(msg) = incoming.next().await { … }` exits
with NO log when the transport's incoming stream closes (returns `None`). In normal operation
transport streams are long-lived, but an unexpected stream termination (transport bug, early drop,
network-layer error) silently kills the receive path for that transport with zero operator
visibility. Unlike the D1 broadcast-lag fix (which is about the UI channel), this is about the
transport-level ingress path: after silent exit, no frames from that transport are ever processed
again.

**Current** (lines 248–258):
```rust
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = incoming.next().await {
                match engine.process_incoming(msg).await {
                    Ok(InboundOutcome::Delivered) | Ok(InboundOutcome::ReassembledDelivered) => {
                        tracing::debug!("desktop: inbound delivered");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!("desktop: inbound rejected: {e}"),
                }
            }
        });
```

**Replace with** (log on stream end so the operator knows the forwarder died):
```rust
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = incoming.next().await {
                match engine.process_incoming(msg).await {
                    Ok(InboundOutcome::Delivered) | Ok(InboundOutcome::ReassembledDelivered) => {
                        tracing::debug!("desktop: inbound delivered");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!("desktop: inbound rejected: {e}"),
                }
            }
            // Stream closed — no reconnect path exists; operator must restart.
            tracing::warn!(
                "desktop: transport inbound stream closed; this forwarder will not recover"
            );
        });
```

**Acceptance:** log line emitted when transport stream ends; no behaviour change otherwise;
clippy clean.

---

### F-DX-5 — Dev-seam builder methods (`with_node_id`/`with_transport`) are `pub` without `#[cfg(test)]` · LOW

**File:** `crates/iris-desktop/src/engine_handle.rs`, lines 82–103.

**Problem:** `DesktopEngine::with_node_id` and `DesktopEngine::with_transport` set `dev: true` in
config, which routes the engine through `DevCryptoProvider` (always-valid, inert crypto — no real
signing, no real verification). Both methods are `pub` with no `#[cfg(test)]` attribute, so they
are callable from production code paths. A regression (e.g. a Tauri command that accidentally calls
`with_node_id`) would silently deploy fake crypto with no compile-time or runtime guard. The
existing comment ("test seam") documents the intent but does not enforce it.

**Current** (lines 82, 93):
```rust
    pub async fn with_node_id(node_id: [u8; 32]) -> Result<Arc<Self>, String> {
    …
    pub async fn with_transport(
```

**Replace `pub` with `#[cfg(test)] pub`** on both methods:
```rust
    #[cfg(test)]
    pub async fn with_node_id(node_id: [u8; 32]) -> Result<Arc<Self>, String> {
    …
    #[cfg(test)]
    pub async fn with_transport(
```

**Acceptance:** `cargo build -p iris-desktop` (non-test build) succeeds and `with_node_id` /
`with_transport` are no longer reachable; `cargo test -p iris-desktop` still sees both methods;
no integration test references the methods from outside `#[cfg(test)]` blocks; clippy clean.

---

---

### F-DX-5 — CORRECTION: `with_transport`/`with_node_id` cannot use `#[cfg(test)]` · LOW (fix-spec error)

**Correction to the F-DX-5 fix spec above.** After a full read of the test files, both
`tests/engine_roundtrip.rs:39` and `tests/commands_mock.rs:23` call `with_transport` and
`with_node_id` from integration test crates. In Rust, `#[cfg(test)]` on a library item is ONLY
visible to the library's own `#[cfg(test)]` unit tests — it is **NOT** visible to integration tests
in `tests/` (which are compiled as separate crates linking the non-test build of the library). So
the original F-DX-5 fix would break both integration test files with "no method named
`with_transport` found" compile errors.

**Correct fix:** use a non-default Cargo feature instead.

**(a) `crates/iris-desktop/Cargo.toml` — add a test-seams feature (inside `[features]`):**
```toml
[features]
test-seams = []
```

**(b) `crates/iris-desktop/src/engine_handle.rs` — gate both methods on the feature:**
```rust
    #[cfg(feature = "test-seams")]
    pub async fn with_node_id(node_id: [u8; 32]) -> Result<Arc<Self>, String> {
    …
    #[cfg(feature = "test-seams")]
    pub async fn with_transport(
```

**(c) `crates/iris-desktop/Cargo.toml` — enable the feature in `dev-dependencies` and nextest:**
Add to `Cargo.toml`:
```toml
[dev-dependencies]
# test-seams feature is enabled automatically for integration tests via the
# line below; the feature must never appear in release dependency trees.
```
And in `.config/nextest.toml` or the Cargo invocation use `--features test-seams` only under
`cargo test`, not `cargo build`.

Actually the simplest correct approach: add to `Cargo.toml`'s `[package]` metadata or ensure the
integration tests pass `features = ["test-seams"]` at the test harness level. The standard pattern
is to add it to the dev-dependency declaration or run `cargo test --features test-seams`.

**Acceptance:** `cargo build -p iris-desktop` (no `--features`) succeeds and both methods are
absent from the public API; `cargo test -p iris-desktop --features test-seams` passes all tests
including `engine_roundtrip` and `commands_mock`.

---

### F-DX-6 — `IncomingMessageView` drops text for SOS payloads · MEDIUM

**File:** `crates/iris-desktop/src/types.rs`, lines 44–49.

**Problem:** `IncomingMessageView::from_envelope` extracts message text only when
`env.payload_type == ContentType::Text`:

```rust
let text = if env.payload_type == ContentType::Text {
    std::str::from_utf8(&env.payload).ok().map(str::to_owned)
} else {
    None
};
```

After F-DX-1 fixes `build_text_envelope` to set `ContentType::Sos` for P0 messages, any SOS
message received on the desktop will have `text: None` in its view. The UI then shows
`[Sos payload not shown]` (from `app.js:524`: `view.text ?? '[${view.content_type} payload not shown]'`)
instead of the actual emergency message text. This completely defeats the purpose of the /sos command
on the desktop — the receiver sees the SOS tag but not what the emergency is.

**Replacement (lines 44–49):**
```rust
let text = if matches!(env.payload_type, ContentType::Text | ContentType::Sos) {
    std::str::from_utf8(&env.payload).ok().map(str::to_owned)
} else {
    None
};
```

**Acceptance:** a round-trip test that sends priority 0 (`/sos hello`) and receives it confirms
`view.text == Some("hello")`; non-text payloads (future types) still produce `None`; clippy clean.
Must be fixed in the SAME commit as F-DX-1 — applying F-DX-1 without F-DX-6 makes the SOS
command worse (correctly tagged but text invisible to recipient).

---

### F-DX-7 — Telemetry command test is vacuously true on a fresh engine · LOW

**File:** `crates/iris-desktop/tests/commands_mock.rs`, line 83.

**Problem:** `get_telemetry` filters for counters where `value > 0`. On a fresh engine with no
messages sent/received, every counter is zero, so the returned `Vec<MetricView>` is empty. The test
assertion:

```rust
let metrics = commands::get_telemetry(state);
assert!(metrics.iter().all(|m| m.value > 0));
```

`.all()` on an empty iterator returns `true` vacuously. The test always passes regardless of
whether `get_telemetry` is wired up correctly, broken, or returns garbage. The test comment even
says "Fresh registry: zero non-zero counters" — confirming the test knows it checks nothing.

**Replacement:**
```rust
// Telemetry on a fresh engine returns an empty slice (all counters are 0;
// get_telemetry filters for non-zero). Assert the command is wired up and
// returns a typed Vec rather than panicking.
let metrics = commands::get_telemetry(state.clone());
assert!(
    metrics.iter().all(|m| m.value > 0),
    "returned counters must be non-zero (get_telemetry filters them)"
);

// Send one message to make at least the 'sent' counter non-zero, then
// verify a non-zero counter appears.
let _ = tauri::async_runtime::block_on(commands::send_message(
    state.clone(),
    hex(BOB),
    "telemetry-probe".to_string(),
    4,
));
let metrics_after = commands::get_telemetry(state);
assert!(
    metrics_after.iter().any(|m| m.name.contains("sent") && m.value > 0),
    "get_telemetry must surface the 'sent' counter after a send"
);
```

**Acceptance:** test exercises real counter state; `get_telemetry` is not vacuously green on a
fresh engine; no other tests affected.

---

### Additional-findings coverage map

| New finding | Severity | File | Status |
|---|---|---|---|
| F-DX-1 (desktop build_text_envelope P0 ContentType) | HIGH | `crates/iris-desktop/src/engine_handle.rs:398` | **Fix required** |
| F-DX-2 (renderPalette adjacent-only group dedup) | LOW | `crates/iris-desktop/ui/app.js:180` | **Fix required** |
| F-DX-3 (navigator.platform deprecated) | LOW | `crates/iris-desktop/ui/app.js:14` | **Fix required** |
| F-DX-4 (spawn_inbox_forwarder silent exit) | MEDIUM | `crates/iris-desktop/src/engine_handle.rs:248` | **Fix required** |
| F-DX-5 (dev-seam methods visibility — #[cfg(test)] breaks integration tests; use feature flag) | LOW | `crates/iris-desktop/src/engine_handle.rs:82,93` | **Fix required (spec corrected above)** |
| F-DX-6 (IncomingMessageView drops SOS payload text — must fix with F-DX-1) | MEDIUM | `crates/iris-desktop/src/types.rs:44` | **Fix required** |
| F-DX-7 (telemetry test vacuously true on fresh engine) | LOW | `crates/iris-desktop/tests/commands_mock.rs:83` | **Fix required** |

Total findings after deep sweep: **28** (21 original SEC6_AUDIT + 7 new).
