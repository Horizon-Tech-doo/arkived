//! Microsoft Entra ID auth (OAuth 2.0 device-code flow, hand-rolled).
//!
//! `azure_identity 0.34` no longer exposes a user-facing `DeviceCodeCredential`,
//! so we implement the four-endpoint flow directly against
//! `login.microsoftonline.com`.

pub mod device_code;

/// Default Entra client ID used for the device-code flow.
///
/// This is Microsoft's public "Azure CLI" multi-tenant app registration, which
/// is a well-known public client suitable for unbranded dev tools. Override via
/// a constructor argument or environment variable before v0.1.0 GA.
///
/// To register your own Arkived Entra app, follow
/// <https://learn.microsoft.com/entra/identity-platform/quickstart-register-app>
/// and configure it as a *public client* with the "Azure Storage" API permission.
pub const DEFAULT_CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// Default scope for Azure Storage access via Entra ID.
pub const STORAGE_SCOPE: &str = "https://storage.azure.com/.default";
