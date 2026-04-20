//! Arkived Tauri backend — stub IPC commands for the UI scaffold.
//!
//! Stage 3 scaffold: commands return mock data that mirrors the design prototype.
//! When `arkived-core` gains a real Azure implementation (Stage 1), these
//! handlers will delegate to it and wire the Policy trait through.

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_subscriptions,
            commands::list_blobs,
            commands::list_activities,
            commands::agent_transcript,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
