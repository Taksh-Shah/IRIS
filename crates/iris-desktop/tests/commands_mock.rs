//! DESKTOP-001 command surface tests against a real `DesktopEngine` managed as
//! Tauri State (RES-0013 §Testing). Commands run through the exact
//! `#[tauri::command]` fns registered by `generate_handler!`, so the handler
//! wiring is compiled and exercised; the windowless MockRuntime app keeps the
//! tests CI-friendly (no WebView2 display needed).

use std::sync::Arc;

use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Manager, State};

use iris_desktop::commands;
use iris_desktop::engine_handle::DesktopEngine;

const ALICE: [u8; 32] = [0xAA; 32];
const BOB: [u8; 32] = [0xBB; 32];

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn build_app() -> tauri::App<tauri::test::MockRuntime> {
    let engine = tauri::async_runtime::block_on(DesktopEngine::with_node_id(ALICE))
        .expect("engine must build");
    let app = mock_builder()
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_mesh_status,
            commands::get_telemetry,
            commands::subscribe_inbox,
        ])
        .build(mock_context(noop_assets()))
        .expect("mock app must build");
    // Manage state on the built App. The constructors return `Arc<DesktopEngine>`,
    // so the managed state key is `Arc<DesktopEngine>` (commands request the same).
    let managed = app.manage(engine);
    assert!(managed, "DesktopEngine manage() must succeed");
    assert!(
        app.try_state::<Arc<DesktopEngine>>().is_some(),
        "state visible right after manage"
    );
    app
}

#[test]
fn send_message_command_returns_truncated_id() {
    let app = build_app();
    let state: State<'_, Arc<DesktopEngine>> = app.state();
    let view = tauri::async_runtime::block_on(commands::send_message(
        state.clone(),
        hex(BOB),
        "hello".to_string(),
        4,
    ))
    .expect("command ok");
    assert_eq!(view.id_hex.len(), 16, "privacy-truncated id only");
}

#[test]
fn mesh_status_command_reports_online_and_transport() {
    let app = build_app();
    let state: State<'_, Arc<DesktopEngine>> = app.state();
    let status = tauri::async_runtime::block_on(commands::get_mesh_status(state.clone()))
        .expect("command ok");
    assert!(status.online);
    assert_eq!(status.node_id_short.len(), 16, "privacy-truncated node id");
    assert_eq!(status.transports.len(), 1);
    assert_eq!(status.transports[0].id, "loopback");
    assert_eq!(status.metrics.sent, 0);
}

#[test]
fn telemetry_command_returns_typed_snapshot() {
    let app = build_app();
    assert!(
        app.try_state::<Arc<DesktopEngine>>().is_some(),
        "DesktopEngine must be managed"
    );
    let state: State<'_, Arc<DesktopEngine>> = app.state();
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
}
