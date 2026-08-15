# DESKTOP-001 Verification — Tauri Desktop Shell

**Document ID**: IRIS-DESKTOP-VERIFICATION-001
**Version**: 1.0
**Node**: DESKTOP-001 (WP-10), type PLATFORM, priority P1
**Date**: 2026-08-14
**Design basis**: docs/implementation/DESKTOP_DESIGN.md v1.0 (AC 1-6)
**Research basis**: RES-0013 (Tauri v2 SOTA)
**Verifier**: independent subagent review of durable repository artifacts

---

## 1. Verdict

**ALL 6 ACCEPTANCE CRITERIA PASS.** Node DESKTOP-001 is COMPLETE with evidence.

- Workspace test suite: **225 passed, 0 failed, 1 ignored** (`cargo test --workspace`)
- `cargo clippy --workspace --all-targets`: **0 warnings**
- `cargo build -p iris-desktop`: success; `target/debug/iris-desktop.exe` launches and
  stays alive (windowless smoke check, no crash)
- Dependencies TRANSPORT-001 + MSG-001 COMPLETE (graph-verified, edges direction-confirmed)

## 2. Acceptance Criteria — Evidence

| AC | Criteria | Verdict | Evidence |
|----|----------|---------|----------|
| 1 | Desktop app launches and hosts the engine | **PASS** | `src/engine_handle.rs:90-166` `DesktopEngine::build` constructs `TransportManager` + registers transports + `MessageEngine::new_with_telemetry` with node identity; `src/main.rs` → `src/lib.rs:14-31` runs Tauri `Builder` with `.setup` → `manage(Arc<DesktopEngine>)` + `generate_handler![4 commands]` + `generate_context!` + `run()`. Mock app construction asserts `manage()` succeeds and `try_state::<Arc<DesktopEngine>>()` visible (`tests/commands_mock.rs:22-43`). Build artifact `target/debug/iris-desktop.exe`; `gen/schemas/desktop-schema.json` generated. |
| 2 | Send/receive over available transports | **PASS** | `tests/engine_roundtrip.rs:37-63` `loopback_wire_delivers_from_alice_to_bob`: full production path `send_text` → engine → shared `SimulatedTransport` → auto-spawned inbox forwarder (`engine_handle.rs:171-185`) → `process_incoming` → `delivered_messages` → asserts payload/recipient/truncated-id/`metrics.sent==1`. `multiple_messages_arrive_in_order` (`:66-89`) proves ordering. `subscribe_inbox` forwards the delivered stream to a Tauri `Channel` (`engine_handle.rs:223-233`). |
| 3 | Message history via STORE-001 seam | **PASS** | Seam wired: engine over `Arc<dyn MessageStorage>` = `MemoryStorage` (`engine_handle.rs:136-148`); `MessageEngine::send_message` persists via `storage.persist` (`message_engine/mod.rs`); `MemoryStorage` implements persist/load. AC3 v1 scope = "seam is wired, verified by engine tests persisting" — met. |
| 4 | Telemetry visible | **PASS** | `tests/commands_mock.rs:73-83` `telemetry_command_returns_typed_snapshot` + `:60-70` `mesh_status_command_reports_online_and_transport` exercise the exact registered `#[tauri::command]` fns; `get_telemetry` → `engine.telemetry()` non-zero OBS-001 snapshot (`engine_handle.rs:259-273`); UI renders counters (`ui/app.js`). |
| 5 | Graceful degradation without transport | **PASS** | `tests/engine_roundtrip.rs:92-113` `graceful_degradation_when_transport_down`: online → `shutdown()` → `!online`, transport reports `"Unavailable"`, `send_text` still enqueues (store-carry-forward, not dropped, truncated id). `TransportState` discriminant ordering makes `online` computation correct. |
| 6 | Tests cover 1-5; workspace green; clippy 0 warnings | **PASS** | Coverage map: AC1←mock-app construction; AC2←2 roundtrip tests; AC3←engine persist path; AC4←2 commands_mock tests; AC5←graceful test. 217 pre-desktop + 8 desktop tests (3 roundtrip + 3 commands_mock + 2 types unit) = 225. Clippy clean. |

## 3. Security Review Note

- No payload bytes cross the IPC metadata boundary except the explicit user-initiated
  send text. All identifiers in views are privacy-truncated via `ShortId` (P2).
- Tauri capabilities/ACL grant only `core:default` + our four commands; the webview
  has no excess IPC surface (`capabilities/default.json`).
- Webview never touches transports directly — all transport ops through Rust commands.
- `tauri.conf.json` CSP is `null` in v1 (no remote content; local-only vanilla UI) —
  hardening follow-up.

## 4. Known Limitations (v1, recorded on graph node)

- Loopback transport always + optional Internet relay (`IRIS_RELAY_ADDR`); BLE/Wi-Fi
  local paths wait for BLK-0005 hardware.
- No message-history read command in v1 (STORE-001 seam wired; `MemoryStorage` means
  queued/history lost on restart; UI shows session inbox only).
- Node identity is a fresh random id per process (`IRIS_NODE_ID` override) until
  IDENT-001 lands — not persisted per install.
- `PgStorage` optional path not wired in v1 (D9 partial; `iris-storage` dependency
  declared but unused).
- Single window; tray/daemon/notifications documented (RES-0013 §8), enabled later.
- `subscribe_inbox` Channel IPC path and `IRIS_RELAY_ADDR` Internet registration not
  covered by automated tests (gap; engine paths covered).
- CSP null hardening follow-up (no remote content in v1).
- GAP-DESKTOP-001/002/003 carried from RES-0013 (webview rendering differences,
  bundle-size benchmarks, channel backpressure in daemon mode).

## 5. Deviation Resolved In-Pass

- DESKTOP_DESIGN.md D7 wording corrected: state key is `Arc<DesktopEngine>` (constructors
  return `Arc<Self>`); commands request `State<'_, Arc<DesktopEngine>>`.
- Node-identity wording corrected in design §4/§9 + code comment: per-process random
  (not per-install) until IDENT-001.

## 6. Evidence Artifacts

- `crates/iris-desktop/` — Cargo.toml, build.rs, tauri.conf.json, capabilities/, ui/,
  src/{lib,main,engine_handle,commands,types}.rs, tests/{engine_roundtrip,commands_mock}.rs
- `docs/implementation/DESKTOP_DESIGN.md` v1.0
- `engineering/memory/records/research/RES-0013.md`
- Workspace 225 green, clippy 0 warnings (2026-08-14)
