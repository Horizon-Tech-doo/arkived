//! Arkived Tauri backend — stub IPC commands for the UI scaffold.
//!
//! Stage 3 scaffold: commands return mock data that mirrors the design prototype.
//! When `arkived-core` gains a real Azure implementation (Stage 1), these
//! handlers will delegate to it and wire the Policy trait through.

mod commands;

use arkived_core::auth::credentials::{CredentialStore, OsKeyring};
use arkived_core::Store;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(format!("resolve app data dir: {error}")))?;
            std::fs::create_dir_all(&app_data_dir).map_err(|error| {
                std::io::Error::other(format!(
                    "create app data directory `{}`: {error}",
                    app_data_dir.display()
                ))
            })?;

            let store_path = app_data_dir.join("arkived-state.sqlite3");
            let snapshot_path = app_data_dir.join("arkived-shell-state.json");
            let store = Arc::new(Store::open(&store_path).map_err(|error| {
                std::io::Error::other(format!(
                    "open persistent state store `{}`: {error}",
                    store_path.display()
                ))
            })?);
            let credential_store: Arc<dyn CredentialStore> =
                Arc::new(OsKeyring::new("arkived-desktop"));

            let state = tauri::async_runtime::block_on(commands::AppState::restore(
                store,
                credential_store,
                snapshot_path,
            ))
            .map_err(std::io::Error::other)?;

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::list_sign_ins,
            commands::remove_sign_in,
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
            commands::upload_blob,
            commands::upload_folder,
            commands::download_blob,
            commands::preview_blob,
            commands::download_blob_prefix,
            commands::delete_blob,
            commands::delete_blob_prefix,
            commands::create_blob_folder,
            commands::rename_blob_item,
            commands::copy_blob_item,
            commands::disconnect_connection,
            commands::list_activities,
            commands::clear_activities,
            commands::cancel_activity,
            commands::agent_transcript,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
