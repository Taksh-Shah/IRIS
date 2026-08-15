//! Thin `#[tauri::command]` surface over the engine host (DESKTOP_DESIGN.md §5).
//!
//! Every command is a sync/async `fn` returning owned types; async commands
//! never borrow managed state (RES-0013 #2533). The webview has no transport
//! access — all mesh operations go through these commands.

use std::sync::Arc;

use tauri::State;

use crate::engine_handle::DesktopEngine;
use crate::types::{IncomingMessageView, MeshStatus, MessageIdView, MetricView};

/// Send a text message to a 64-hex node id. `priority` 0-7 (P0=P0 SOS .. P7).
#[tauri::command]
pub async fn send_message(
    engine: State<'_, Arc<DesktopEngine>>,
    recipient: String,
    text: String,
    priority: u8,
) -> Result<MessageIdView, String> {
    engine.send_text(&recipient, &text, priority).await
}

/// Aggregate status for the shell header (node id, transports, counters).
#[tauri::command]
pub async fn get_mesh_status(engine: State<'_, Arc<DesktopEngine>>) -> Result<MeshStatus, String> {
    Ok(engine.mesh_status().await)
}

/// Non-zero OBS-001 counters for the telemetry panel.
#[tauri::command]
pub fn get_telemetry(engine: State<'_, Arc<DesktopEngine>>) -> Vec<MetricView> {
    engine.telemetry()
}

/// Stream delivered messages to the UI over an ordered IPC Channel (D5).
#[tauri::command]
pub fn subscribe_inbox(
    engine: State<'_, Arc<DesktopEngine>>,
    channel: tauri::ipc::Channel<IncomingMessageView>,
) {
    engine.subscribe_inbox(channel);
}
