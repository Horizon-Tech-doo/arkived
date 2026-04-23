//! Arkived Tauri backend — stub IPC commands for the UI scaffold.
//!
//! Stage 3 scaffold: commands return mock data that mirrors the design prototype.
//! When `arkived-core` gains a real Azure implementation (Stage 1), these
//! handlers will delegate to it and wire the Policy trait through.

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::list_sign_ins,
            commands::list_sign_in_tenants,
            commands::update_sign_in_filter,
            commands::list_subscriptions,
            commands::list_discovered_storage_accounts,
            commands::connect_connection_string,
            commands::connect_account_key,
            commands::connect_sas,
            commands::connect_azurite,
            commands::start_entra_device_login,
            commands::poll_entra_device_login,
            commands::start_entra_browser_login,
            commands::poll_entra_browser_login,
            commands::start_sign_in_tenant_reauth,
            commands::poll_sign_in_tenant_reauth,
            commands::start_entra_discovery_login,
            commands::poll_entra_discovery_login,
            commands::connect_discovered_storage_account,
            commands::list_containers,
            commands::list_blobs,
            commands::disconnect_connection,
            commands::list_activities,
            commands::agent_transcript,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
