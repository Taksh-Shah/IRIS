//! IRIS desktop shell library (DESKTOP-001).
//!
//! The binary is a thin wrapper over [`run`]; the crate is a library so the
//! engine host and command surface are testable without a window.

pub mod commands;
pub mod engine_handle;
pub mod identity;
pub mod types;

use engine_handle::DesktopEngine;
use tauri::Manager;

/// Build and run the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let engine = tauri::async_runtime::block_on(DesktopEngine::new())
                .map_err(|e| format!("failed to start IRIS engine: {e}"))?;
            app.manage(engine);
            tracing::info!("IRIS desktop engine online");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_mesh_status,
            commands::get_telemetry,
            commands::subscribe_inbox,
        ])
        .run(tauri::generate_context!())
        .expect("error while running IRIS Mesh");
}
