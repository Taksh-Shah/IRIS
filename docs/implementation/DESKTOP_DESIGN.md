# DESKTOP-001 Design — Tauri Desktop Shell

**Document ID**: IRIS-DESKTOP-DESIGN-001
**Version**: 1.0
**Node**: DESKTOP-001 (WP-10), type PLATFORM, priority P1
**Date**: 2026-08-14
**Research basis**: RES-0013 (Tauri v2 state of the art, verified 2026-08-14)

---

## 1. Objective

A cross-platform desktop messenger (Windows first; macOS/Linux same codebase)
that hosts the `iris-core` engine in-process and binds it to a native window
via **Tauri v2**. The desktop shell is a *thin adapter*: all protocol,
routing, telemetry, and storage logic lives in `iris-core`/`iris-storage`
(already GUI-free and fully tested). The shell adds the window, the chat UI,
and the Tauri IPC command surface.

Build host is Windows (`x86_64-pc-windows-msvc`, rustc 1.97.1, node v22,
crates.io reachable). WebView2 is preinstalled on Windows 10 1803+ / 11 —
no extra webview setup for dev.

## 2. Decisions (grounded in RES-0013)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Tauri v2** (`tauri = "2"`, 2.11.x line) | System webview (~10-15 MB, ~1/3 Electron RAM), Rust core (DEC-0003), official plugins (tray/process/autostart), mobile path later. |
| D2 | **Vanilla HTML/JS frontend** — no npm build, `withGlobalTauri: true`, `frontendDist: "../ui"` | RES-0013: official first-class "vanilla" template; zero toolchain; dev server built-in. Revisit Vite+React only if UI grows. |
| D3 | **New workspace crate `crates/iris-desktop`** | Stays in `cargo test --workspace`; shares workspace deps; AC6 "workspace green" includes it. |
| D4 | **Engine hosted in-process via `DesktopEngine`** (owns `MessageEngine` + `TransportManager` + node identity) | Matches PLATFORM_ARCHITECTURE.md: Core (Rust) owns global state; shell is a thin binding. |
| D5 | **Inbound messages streamed over `tauri::ipc::Channel`** | RES-0013: events are "not designed for low latency or high throughput"; Channels are ordered + fast. Chat stream = Channel. |
| D6 | **Commands are thin `async fn → Result<_, String>`** with owned `String` args | RES-0013 #2533: async commands can't borrow; State must be Send+Sync. |
| D7 | **State via `app.manage(Arc<DesktopEngine>)`** | Constructors return `Arc<Self>`, so the managed state key is `Arc<DesktopEngine>` and commands request `State<'_, Arc<DesktopEngine>>`. Tauri wraps the value in its own `Arc` internally; the explicit `Arc` avoids a redundant second wrapper and keeps `&self` call sites deref-through-Arc. |
| D8 | **Transports: loopback `SimulatedTransport` always; `InternetTransport` when `IRIS_RELAY_ADDR` configured** | Loopback proves send/receive without hardware; Internet real when a relay is available. BLE/local deferred (BLK-0005). |
| D9 | **Storage via `MessageStorage` seam** — `MemoryStorage` default; `PgStorage` if `IRIS_PG_*` configured | STORE-001 trait is the seam; desktop v1 stays standalone, PG optional. |
| D10 | **No new iris-core API changes** — binding only (additive) | Keeps the engine crate's 217-test suite + clippy clean untouched. |
| D11 | **Daemon-ready** (documented, not enabled by default): tray + `RunEvent::ExitRequested` `prevent_exit()` when code is None | RES-0013 §8; a resident mesh node must survive window close. |

## 3. Crate Layout

```
crates/iris-desktop/
  Cargo.toml              # tauri 2, iris-core, serde, tokio
  build.rs                # tauri_build::build()
  tauri.conf.json         # identifier "in.iris.mesh.desktop", frontendDist "../ui",
                          #   withGlobalTauri true, main window 900x600
  capabilities/default.json   # core:default + our commands
  ui/                     # index.html + app.js + styles.css (vanilla)
  src/
    lib.rs                # pub fn run() -> Builder setup, manage EngineHandle,
                          #   register commands (commands::foo), tray+daemon hooks
    main.rs               # binary: calls desktop_lib::run()
    engine_handle.rs      # DesktopEngine: engine + manager + node_id + inbox forwarder
    commands.rs           # #[tauri::command] thin wrappers
    types.rs              # serde DTOs: SendMessageRequest, IncomingMessageView,
                          #   MeshStatus, TransportStatus, MetricView
  tests/
    engine_roundtrip.rs   # no-GUI engine-level tests (loopback SimulatedTransport)
    commands_mock.rs      # tauri::test MockRuntime IPC tests
```

## 4. DesktopEngine (engine_handle.rs)

```rust
pub struct DesktopEngine {
    pub node_id: [u8; 32],          // configurable, stable per install
    pub engine: Arc<MessageEngine>, // new_with_telemetry(MetricsRegistry::new())
    pub manager: Arc<TransportManager>,
    transports: Vec<TransportId>,   // registered ids for status
}
```

- **Construction** (`DesktopEngine::new(config)`): creates `TransportManager`,
  registers loopback `SimulatedTransport` (`"loopback"`), optionally
  `InternetTransport` when `IRIS_RELAY_ADDR` (comma-separated `SocketAddr`s)
  is set; builds `MessageEngine` over the storage seam with this node's
  identity; subscribes `engine.delivered_messages()`.
- **Inbox forwarding**: `subscribe_inbox(channel)` spawns a task that reads
  `engine.delivered_messages()` and forwards each envelope as a
  serde-typed `IncomingMessageView` (truncated ids via `short()`, no payload
  in metadata — privacy P2). Runs on `tauri::async_runtime::spawn` so it dies
  with the app (RES-0013 §4).
- **Node identity**: `node_id` from `IRIS_NODE_ID` (hex 64) else a fresh
  random id per process (persistence deferred to IDENT-001).

## 5. Command Surface (commands.rs — thin wrappers)

| Command | Args | Returns | Description |
|---------|------|---------|-------------|
| `send_message` | `recipient: String`, `text: String`, `priority: u8` | `MessageIdView` | Builds `Envelope` (ContentType::Text, timestamp now, ttl 3600), `engine.send_message()` |
| `get_mesh_status` | — | `MeshStatus` | node_id short, registered transports + states, engine metrics (sent/delivered/relayed/expired) |
| `get_telemetry` | — | `Vec<MetricView>` | OBS-001 `MetricsRegistry.snapshot()` (non-zero iris.* counters) |
| `subscribe_inbox` | `Channel<IncomingMessageView>` | `()` | Streams delivered messages to UI (Channel) |

All commands: `async fn`, owned `String` args, `Result<_, String>` returns,
`State<'_, Arc<DesktopEngine>>` for the engine (RES-0013 §3/§4).

## 6. Frontend (vanilla)

- `index.html`: chat layout — inbox list, composer (recipient hex + text +
  priority), status bar, telemetry panel.
- `app.js`: `window.__TAURI__.core.invoke(...)` for commands;
  `window.__TAURI__.core.Channel` for `subscribe_inbox`; poll
  `get_mesh_status`/`get_telemetry` on a timer for the status bar.
- `styles.css`: minimal, mobile-first; no external assets (works offline).

## 7. Acceptance Criteria

1. **Desktop app launches and hosts the engine** — `DesktopEngine` constructs
   a `MessageEngine` + `TransportManager` with node identity; `main` runs the
   Tauri app (`Builder` + `run()`). Verified by build + `tauri::test`
   `mock_builder` app construction and engine-level tests.
2. **Send/receive over available transports** — loopback `SimulatedTransport`
   round-trip: `send_message` → engine → transport → `process_incoming` →
   delivered → `subscribe_inbox` Channel. Internet path when relay configured.
3. **Message history via STORE-001 seam** — engine persists through the
   `MessageStorage` trait (`MemoryStorage` default, `PgStorage` when
   configured); a `get_messages`-style extension is a documented follow-up
   (AC3 in v1 = seam is wired, verified by engine tests persisting).
4. **Telemetry visible** — `get_telemetry` returns OBS-001 registry snapshot;
   UI renders counters. Verified by mock command + registry test.
5. **Graceful degradation without transport** — status reports transport
   `Unavailable`/missing; UI shows offline; sends still enqueue
   (store-carry-forward semantics, not dropped).
6. **Tests cover 1-5; workspace green; clippy 0 warnings.**

## 8. Security / Privacy Notes

- No payload bytes cross the IPC metadata boundary except the explicit send
  text (user-initiated). `IncomingMessageView` uses `ShortId`/`short()` for
  all identifiers (P2).
- Tauri v2 capabilities/ACL grant only `core:default` + our commands — the
  webview has no excess IPC surface.
- The webview never touches transports directly (RES-0013 §7 / TS layer doc) —
  all transport ops go through Rust commands.
- Daemon mode (D11) not enabled by default in v1; documented as follow-up.

## 9. Known Limitations (v1)

- Loopback transport only + optional Internet relay — BLE/Wi-Fi local paths
  wait for BLK-0005 hardware.
- No message-history read command in v1 (STORE-001 seam wired; UI shows only
  session inbox).
- Node identity is a fresh random id per process (`IRIS_NODE_ID` override
  available) until IDENT-001 lands.
- Single window; tray/daemon/notifications documented (RES-0013 §8), enabled
  later.

## 10. Verification Plan

- `cargo test --workspace`: engine_roundtrip (loopback deliver),
  commands_mock (`get_ipc_response` for send_message/status/telemetry),
  plus all existing 217 core/storage/sim tests.
- `cargo clippy --workspace --all-targets`: 0 warnings.
- Graph DESKTOP-001 → COMPLETE with evidence + known_limitations.
