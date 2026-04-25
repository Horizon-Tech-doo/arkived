//! Tauri IPC commands backed by `arkived-core`.
//!
//! The desktop shell supports two Azure paths:
//! - direct data-plane attachments (connection string, account key, SAS, Azurite)
//! - Azure account sign-in with ARM discovery of subscriptions and storage accounts
//!
//! Discovered accounts can then be activated into live blob-browser connections.

use arkived_core::auth::credentials::CredentialStore;
use arkived_core::auth::entra::cache::{CachedRefresh, RefreshCache};
use arkived_core::auth::entra::credential::{RefreshContext, TokenBundle};
use arkived_core::auth::entra::device_code::{
    poll_for_token, refresh_access_token, start_device_code, DeviceCodeResponse, TokenResponse,
};
use arkived_core::auth::entra::{DEFAULT_CLIENT_ID, STORAGE_SCOPE};
use arkived_core::auth::{
    AccountKeyProvider, AuthProvider, AzuriteEmulatorProvider, ConnectionStringParts,
    ConnectionStringProvider, ResolvedCredential, SasTokenProvider,
};
use arkived_core::backend::{
    AzureBlobBackend, BlobEntry, BlobPath, ByteStream, DeleteOpts, Page, WriteOpts,
};
use arkived_core::policy::AllowAllPolicy;
use arkived_core::Ctx;
use arkived_core::store::{AttachedResource, SignIn};
use arkived_core::types::{AuthKind, AzureEnvironment, ResourceKind};
use arkived_core::Store;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use bytes::Bytes;
use futures::{stream, StreamExt};
use secrecy::SecretString;
use secrecy::ExposeSecret;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use tauri::State;
use time::{Duration as TimeDuration, OffsetDateTime};
use uuid::Uuid;

const ARM_SCOPE: &str = "https://management.azure.com/.default";
const ARM_TENANTS_API_VERSION: &str = "2022-12-01";
const ARM_SUBSCRIPTIONS_API_VERSION: &str = "2020-01-01";
const ARM_STORAGE_ACCOUNTS_API_VERSION: &str = "2023-05-01";
const ENTRA_INTERACTIVE_AUTH_KIND: &str = "entra-interactive";
const ENTRA_DEVICE_CODE_AUTH_KIND: &str = "entra-device-code";
const KEYCHAIN_CONNECTION_PREFIX: &str = "arkived:connection:";

pub struct AppState {
    inner: Arc<Mutex<InnerState>>,
    store: Arc<Store>,
    credential_store: Arc<dyn CredentialStore>,
    snapshot_path: PathBuf,
}

#[derive(Default)]
struct InnerState {
    connections: HashMap<String, LiveConnection>,
    pending_device_logins: HashMap<String, PendingLogin>,
    pending_discovery_logins: HashMap<String, PendingLogin>,
    pending_browser_logins: HashMap<String, PendingLogin>,
    pending_tenant_browser_logins: HashMap<String, PendingLogin>,
    sign_ins: HashMap<String, SignInSession>,
}

#[derive(Clone)]
struct ConnectionOrigin {
    sign_in_id: String,
    subscription_id: String,
}

#[derive(Clone)]
enum LiveConnection {
    ConnectionString {
        id: String,
        display_name: String,
        endpoint: String,
        raw: String,
        fixed_container: Option<String>,
    },
    AccountKey {
        id: String,
        display_name: String,
        endpoint: String,
        account_name: String,
        auth_kind: String,
        key: String,
        origin: Option<ConnectionOrigin>,
    },
    Sas {
        id: String,
        display_name: String,
        endpoint: String,
        sas: String,
        fixed_container: Option<String>,
    },
    Azurite {
        id: String,
        display_name: String,
    },
    Entra {
        id: String,
        display_name: String,
        endpoint: String,
        account_name: String,
        tenant: String,
        auth_kind: String,
        origin: Option<ConnectionOrigin>,
        fallback_note: Option<String>,
        bundle: TokenBundle,
    },
}

#[derive(Clone)]
struct SignInSession {
    id: String,
    display_name: String,
    login_tenant: String,
    environment: String,
    auth_kind: String,
    arm_bundle: TokenBundle,
    tenant_bundles: HashMap<String, TokenBundle>,
    tenants: Vec<DiscoveredTenant>,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedShellState {
    #[serde(default)]
    sign_ins: Vec<PersistedSignInSnapshot>,
}

#[derive(Serialize, Deserialize, Clone)]
struct PersistedSignInSnapshot {
    id: String,
    display_name: String,
    login_tenant: String,
    environment: String,
    auth_kind: String,
    tenants: Vec<DiscoveredTenant>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DiscoveredTenant {
    id: String,
    display_name: String,
    default_domain: Option<String>,
    selected: bool,
    needs_reauth: bool,
    error: Option<String>,
    subscriptions: Vec<DiscoveredSubscription>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DiscoveredSubscription {
    id: String,
    name: String,
    tenant_id: String,
    selected: bool,
    storage_accounts: Vec<DiscoveredStorageAccount>,
}

#[derive(Clone, Serialize, Deserialize)]
struct DiscoveredStorageAccount {
    name: String,
    subscription_id: String,
    kind: String,
    region: String,
    replication: String,
    tier: String,
    hns: bool,
    endpoint: String,
    resource_id: Option<String>,
}

#[derive(Clone)]
struct PendingLogin {
    status: PendingLoginStatus,
}

#[derive(Clone)]
enum PendingLoginStatus {
    Pending,
    Complete { id: String },
    Error { message: String },
}

#[derive(Serialize, Clone)]
pub struct BrowserConnection {
    pub id: String,
    pub display_name: String,
    pub account_name: String,
    pub endpoint: String,
    pub auth_kind: String,
    pub fixed_container: Option<String>,
    pub origin_sign_in_id: Option<String>,
    pub origin_subscription_id: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct BrowserSignIn {
    pub id: String,
    pub display_name: String,
    pub tenant: String,
    pub environment: String,
    pub subscription_count: usize,
    pub selected_subscription_count: usize,
    pub tenant_count: usize,
    pub selected_tenant_count: usize,
}

#[derive(Serialize, Clone)]
pub struct BrowserTenant {
    pub id: String,
    pub sign_in_id: String,
    pub display_name: String,
    pub default_domain: Option<String>,
    pub selected: bool,
    pub needs_reauth: bool,
    pub error: Option<String>,
    pub subscription_count: usize,
    pub selected_subscription_count: usize,
    pub storage_account_count: usize,
    pub subscriptions: Vec<BrowserSubscription>,
}

#[derive(Serialize, Clone)]
pub struct BrowserSubscription {
    pub id: String,
    pub sign_in_id: String,
    pub name: String,
    pub tenant_id: String,
    pub tenant_label: String,
    pub storage_account_count: usize,
    pub selected: bool,
}

#[derive(Serialize, Clone)]
pub struct BrowserStorageAccount {
    pub sign_in_id: String,
    pub subscription_id: String,
    pub name: String,
    pub kind: String,
    pub region: String,
    pub replication: String,
    pub tier: String,
    pub hns: bool,
    pub endpoint: String,
}

#[derive(Serialize)]
pub struct BrowserContainer {
    pub id: String,
    pub name: String,
    pub public_access: Option<String>,
    pub lease: Option<String>,
    pub blob_count: Option<u64>,
}

#[derive(Serialize)]
pub struct BrowserBlobRow {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: Option<String>,
    pub tier: Option<String>,
    pub modified: String,
    pub etag: Option<String>,
    pub lease: Option<String>,
    pub icon: String,
}

#[derive(Serialize)]
pub struct BlobDownloadResult {
    pub path: String,
    pub bytes: u64,
    pub opened: bool,
}

#[derive(Serialize)]
pub struct BlobUploadResult {
    pub path: String,
    pub bytes: u64,
    pub etag: String,
}

#[derive(Serialize, Clone)]
pub struct DeviceCodePrompt {
    pub login_id: String,
    pub verification_uri: String,
    pub user_code: String,
    pub message: String,
    pub expires_in_seconds: u64,
    pub interval_seconds: u64,
}

#[derive(Serialize, Clone)]
pub struct BrowserLoginPrompt {
    pub login_id: String,
    pub authorize_url: String,
    pub redirect_uri: String,
}

#[derive(Serialize)]
pub struct DeviceCodeLoginStatus {
    pub status: String,
    pub connection_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct DiscoveryLoginStatus {
    pub status: String,
    pub sign_in_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct Activity {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub started: String,
    pub duration: Option<String>,
    pub progress: Option<f64>,
    pub result: Option<String>,
}

#[derive(Deserialize)]
struct ArmListResponse<T> {
    value: Vec<T>,
    #[serde(rename = "nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct ArmSubscriptionItem {
    #[serde(rename = "subscriptionId")]
    subscription_id: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "tenantId")]
    tenant_id: Option<String>,
}

#[derive(Deserialize)]
struct ArmTenantItem {
    #[serde(rename = "tenantId")]
    tenant_id: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(rename = "defaultDomain", default)]
    default_domain: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
}

#[derive(Deserialize)]
struct ArmStorageAccountItem {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    sku: Option<ArmStorageSku>,
    #[serde(default)]
    properties: Option<ArmStorageAccountProperties>,
}

#[derive(Deserialize)]
struct ArmStorageSku {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    tier: Option<String>,
}

#[derive(Deserialize)]
struct ArmStorageAccountProperties {
    #[serde(rename = "isHnsEnabled", default)]
    is_hns_enabled: Option<bool>,
    #[serde(rename = "primaryEndpoints", default)]
    primary_endpoints: Option<ArmPrimaryEndpoints>,
}

#[derive(Deserialize)]
struct ArmPrimaryEndpoints {
    #[serde(default)]
    blob: Option<String>,
}

#[derive(Deserialize)]
struct ArmListKeysResponse {
    #[serde(default)]
    keys: Vec<ArmStorageAccountKey>,
}

#[derive(Deserialize)]
struct ArmStorageAccountKey {
    #[serde(default)]
    value: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PersistedAccountKeySecret {
    account_name: String,
    account_key: String,
}

#[derive(Serialize, Deserialize)]
struct PersistedSasSecret {
    sas: String,
    fixed_container: Option<String>,
}

#[derive(Deserialize)]
struct IdTokenClaims {
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    upn: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

impl AppState {
    pub fn new(
        store: Arc<Store>,
        credential_store: Arc<dyn CredentialStore>,
        snapshot_path: PathBuf,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerState::default())),
            store,
            credential_store,
            snapshot_path,
        }
    }

    pub async fn restore(
        store: Arc<Store>,
        credential_store: Arc<dyn CredentialStore>,
        snapshot_path: PathBuf,
    ) -> Result<Self, String> {
        let state = Self::new(store, credential_store, snapshot_path);
        state.restore_sign_ins().await?;
        state.restore_direct_attachments().await?;
        Ok(state)
    }

    async fn restore_sign_ins(&self) -> Result<(), String> {
        let snapshot_index: HashMap<_, _> = match load_persisted_shell_state(&self.snapshot_path)
        {
            Ok(state) => state
                .sign_ins
                .into_iter()
                .map(|snapshot| (snapshot.id.clone(), snapshot))
                .collect(),
            Err(error) => {
                eprintln!(
                    "failed to load persisted sign-in snapshots `{}`: {error}",
                    self.snapshot_path.display()
                );
                HashMap::new()
            }
        };
        let persisted = self
            .store
            .sign_in_list()
            .map_err(|error| format!("failed to list persisted sign-ins: {error}"))?;

        for persisted_sign_in in persisted {
            let snapshot = snapshot_index.get(&persisted_sign_in.id).cloned();
            let restored = match snapshot {
                Some(snapshot) => match restore_sign_in_session_from_snapshot(
                    &*self.credential_store,
                    persisted_sign_in.clone(),
                    snapshot,
                ) {
                    Ok(sign_in) => Ok(sign_in),
                    Err(snapshot_error) => restore_sign_in_session(
                        &*self.credential_store,
                        persisted_sign_in.clone(),
                    )
                    .await
                    .map_err(|live_error| {
                        format!(
                            "failed to restore sign-in `{}` from snapshot ({snapshot_error}) or live refresh ({live_error})",
                            persisted_sign_in.display_name
                        )
                    }),
                },
                None => restore_sign_in_session(&*self.credential_store, persisted_sign_in.clone())
                    .await,
            };

            match restored {
                Ok(sign_in) => {
                    if let Err(error) = persist_sign_in_session_snapshot(
                        &self.store,
                        &*self.credential_store,
                        &self.snapshot_path,
                        &sign_in,
                    ) {
                        eprintln!(
                            "failed to refresh persisted sign-in snapshot `{}`: {error}",
                            sign_in.id
                        );
                    }
                    self.inner
                        .lock()
                        .unwrap()
                        .sign_ins
                        .insert(sign_in.id.clone(), sign_in);
                }
                Err(error) => {
                    eprintln!(
                        "failed to restore persisted sign-in `{}`: {error}",
                        persisted_sign_in.display_name
                    );
                }
            }
        }

        Ok(())
    }

    async fn restore_direct_attachments(&self) -> Result<(), String> {
        let persisted = self
            .store
            .attached_resource_list()
            .map_err(|error| format!("failed to list persisted attachments: {error}"))?;

        for attachment in persisted {
            if let Ok(connection) =
                restore_direct_attachment(&*self.credential_store, attachment).await
            {
                self.inner
                    .lock()
                    .unwrap()
                    .connections
                    .insert(connection_id(&connection).to_string(), connection);
            }
        }

        Ok(())
    }
}

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Vec<BrowserConnection> {
    let guard = state.inner.lock().unwrap();
    let mut connections: Vec<_> = guard
        .connections
        .values()
        .cloned()
        .map(connection_summary)
        .collect();
    connections.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    connections
}

#[tauri::command]
pub fn list_sign_ins(state: State<'_, AppState>) -> Vec<BrowserSignIn> {
    let guard = state.inner.lock().unwrap();
    let mut sign_ins: Vec<_> = guard
        .sign_ins
        .values()
        .cloned()
        .map(sign_in_summary)
        .collect();
    sign_ins.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    sign_ins
}

#[tauri::command]
pub fn list_sign_in_tenants(
    state: State<'_, AppState>,
    sign_in_id: String,
) -> Result<Vec<BrowserTenant>, String> {
    let sign_in = get_sign_in(&state, &sign_in_id)?;
    let mut tenants: Vec<_> = sign_in
        .tenants
        .iter()
        .cloned()
        .map(|tenant| tenant_summary(&sign_in.id, tenant))
        .collect();
    tenants.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(tenants)
}

#[tauri::command]
pub fn update_sign_in_filter(
    state: State<'_, AppState>,
    sign_in_id: String,
    tenant_ids: Vec<String>,
    subscription_ids: Vec<String>,
) -> Result<BrowserSignIn, String> {
    let tenant_ids: HashSet<_> = tenant_ids.into_iter().collect();
    let subscription_ids: HashSet<_> = subscription_ids.into_iter().collect();

    let mut guard = state.inner.lock().unwrap();
    let sign_in = guard
        .sign_ins
        .get_mut(&sign_in_id)
        .ok_or_else(|| format!("unknown sign-in id `{sign_in_id}`"))?;

    for tenant in &mut sign_in.tenants {
        tenant.selected = tenant_ids.contains(&tenant.id);
        for subscription in &mut tenant.subscriptions {
            subscription.selected = subscription_ids.contains(&subscription.id);
        }
    }

    let sign_in_snapshot = sign_in.clone();
    let summary = sign_in_summary(sign_in.clone());
    drop(guard);

    if let Err(error) = persist_sign_in_session_snapshot(
        &state.store,
        &*state.credential_store,
        &state.snapshot_path,
        &sign_in_snapshot,
    ) {
        eprintln!("failed to persist Azure sign-in filter `{sign_in_id}`: {error}");
    }

    Ok(summary)
}

#[tauri::command]
pub fn list_subscriptions(
    state: State<'_, AppState>,
    sign_in_id: String,
) -> Result<Vec<BrowserSubscription>, String> {
    let sign_in = get_sign_in(&state, &sign_in_id)?;
    let mut subscriptions = Vec::new();
    for tenant in sign_in.tenants.iter().filter(|tenant| tenant.selected) {
        let tenant_label = tenant_label(tenant);
        subscriptions.extend(
            tenant
                .subscriptions
                .iter()
                .filter(|subscription| subscription.selected)
                .cloned()
                .map(|subscription| {
                    subscription_summary(&sign_in.id, tenant_label.as_str(), subscription)
                }),
        );
    }
    subscriptions.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(subscriptions)
}

#[tauri::command]
pub fn list_discovered_storage_accounts(
    state: State<'_, AppState>,
    sign_in_id: String,
    subscription_id: String,
) -> Result<Vec<BrowserStorageAccount>, String> {
    let sign_in = get_sign_in(&state, &sign_in_id)?;
    let subscription = sign_in
        .tenants
        .iter()
        .flat_map(|tenant| tenant.subscriptions.iter())
        .find(|subscription| subscription.id == subscription_id)
        .cloned()
        .ok_or_else(|| {
            format!("unknown subscription `{subscription_id}` for sign-in `{sign_in_id}`")
        })?;

    let mut accounts: Vec<_> = subscription
        .storage_accounts
        .into_iter()
        .map(|account| discovered_account_summary(&sign_in.id, account))
        .collect();
    accounts.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(accounts)
}

#[tauri::command]
pub async fn connect_connection_string(
    state: State<'_, AppState>,
    display_name: String,
    connection_string: String,
) -> Result<BrowserConnection, String> {
    let connection = build_connection_string_connection(display_name, connection_string).await?;
    validate_connection(&connection).await?;
    let summary = connection_summary(connection.clone());
    if let Err(error) = persist_direct_connection(&state, &connection) {
        eprintln!(
            "failed to persist connection-string attachment `{}`: {error}",
            summary.display_name
        );
    }
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .insert(summary.id.clone(), connection);
    Ok(summary)
}

#[tauri::command]
pub async fn connect_account_key(
    state: State<'_, AppState>,
    display_name: String,
    account_name: String,
    account_key: String,
    endpoint: Option<String>,
) -> Result<BrowserConnection, String> {
    let connection =
        build_account_key_connection(display_name, account_name, account_key, endpoint)?;
    validate_connection(&connection).await?;
    let summary = connection_summary(connection.clone());
    if let Err(error) = persist_direct_connection(&state, &connection) {
        eprintln!(
            "failed to persist shared-key attachment `{}`: {error}",
            summary.display_name
        );
    }
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .insert(summary.id.clone(), connection);
    Ok(summary)
}

#[tauri::command]
pub async fn connect_sas(
    state: State<'_, AppState>,
    display_name: String,
    endpoint: String,
    sas: String,
    fixed_container: Option<String>,
) -> Result<BrowserConnection, String> {
    let connection = build_sas_connection(display_name, endpoint, sas, fixed_container)?;
    validate_connection(&connection).await?;
    let summary = connection_summary(connection.clone());
    if let Err(error) = persist_direct_connection(&state, &connection) {
        eprintln!(
            "failed to persist SAS attachment `{}`: {error}",
            summary.display_name
        );
    }
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .insert(summary.id.clone(), connection);
    Ok(summary)
}

#[tauri::command]
pub async fn connect_azurite(state: State<'_, AppState>) -> Result<BrowserConnection, String> {
    let connection = LiveConnection::Azurite {
        id: Uuid::new_v4().to_string(),
        display_name: "Azurite (emulator)".into(),
    };
    validate_connection(&connection).await?;
    let summary = connection_summary(connection.clone());
    if let Err(error) = persist_direct_connection(&state, &connection) {
        eprintln!(
            "failed to persist Azurite attachment `{}`: {error}",
            summary.display_name
        );
    }
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .insert(summary.id.clone(), connection);
    Ok(summary)
}

#[tauri::command]
pub async fn start_entra_device_login(
    state: State<'_, AppState>,
    display_name: String,
    account_name: String,
    tenant: Option<String>,
) -> Result<DeviceCodePrompt, String> {
    let display_name = normalized_display_name(&display_name, &account_name);
    let tenant = normalized_tenant(tenant);
    let account_name = account_name.trim().to_string();
    let storage_scope = scope_with_refresh(STORAGE_SCOPE);
    if account_name.is_empty() {
        return Err("storage account name is required".into());
    }

    let client = reqwest::Client::new();
    let device_code = start_device_code(&client, &tenant, DEFAULT_CLIENT_ID, &storage_scope)
        .await
        .map_err(error_to_string)?;
    let login_id = Uuid::new_v4().to_string();
    let prompt = device_code_prompt(login_id.clone(), &device_code);
    let poll_device_code = device_code.clone();

    {
        let mut guard = state.inner.lock().unwrap();
        guard.pending_device_logins.insert(
            login_id.clone(),
            PendingLogin {
                status: PendingLoginStatus::Pending,
            },
        );
    }

    let endpoint = AzureBlobBackend::for_account(
        &account_name,
        &AzureEnvironment::Public,
        ResolvedCredential::Anonymous,
    )
    .map_err(error_to_string)?
    .endpoint()
    .to_string();

    let inner = state.inner.clone();
    tauri::async_runtime::spawn(async move {
        let poll_result = poll_for_token(
            &client,
            &tenant,
            DEFAULT_CLIENT_ID,
            &poll_device_code.device_code,
            StdDuration::from_secs(poll_device_code.interval),
            StdDuration::from_secs(poll_device_code.expires_in),
        )
        .await;

        let mut guard = inner.lock().unwrap();
        match poll_result {
            Ok(token) => {
                let bundle = token_bundle_from_response(
                    token,
                    client.clone(),
                    tenant.clone(),
                    &storage_scope,
                );

                let connection_id = Uuid::new_v4().to_string();
                guard.connections.insert(
                    connection_id.clone(),
                    LiveConnection::Entra {
                        id: connection_id.clone(),
                        display_name: display_name.clone(),
                        endpoint: endpoint.clone(),
                        account_name: account_name.clone(),
                        tenant: tenant.clone(),
                        auth_kind: ENTRA_DEVICE_CODE_AUTH_KIND.into(),
                        origin: None,
                        fallback_note: None,
                        bundle,
                    },
                );

                if let Some(pending) = guard.pending_device_logins.get_mut(&login_id) {
                    pending.status = PendingLoginStatus::Complete { id: connection_id };
                }
            }
            Err(error) => {
                if let Some(pending) = guard.pending_device_logins.get_mut(&login_id) {
                    pending.status = PendingLoginStatus::Error {
                        message: error_to_string(error),
                    };
                }
            }
        }
    });

    Ok(prompt)
}

#[tauri::command]
pub fn poll_entra_device_login(
    state: State<'_, AppState>,
    login_id: String,
) -> Result<DeviceCodeLoginStatus, String> {
    let guard = state.inner.lock().unwrap();
    let pending = guard
        .pending_device_logins
        .get(&login_id)
        .ok_or_else(|| format!("unknown login id `{login_id}`"))?;

    let status = match &pending.status {
        PendingLoginStatus::Pending => DeviceCodeLoginStatus {
            status: "pending".into(),
            connection_id: None,
            error: None,
        },
        PendingLoginStatus::Complete { id } => DeviceCodeLoginStatus {
            status: "complete".into(),
            connection_id: Some(id.clone()),
            error: None,
        },
        PendingLoginStatus::Error { message } => DeviceCodeLoginStatus {
            status: "error".into(),
            connection_id: None,
            error: Some(message.clone()),
        },
    };
    Ok(status)
}

#[tauri::command]
pub async fn start_entra_browser_login(
    state: State<'_, AppState>,
    display_name: String,
    tenant: Option<String>,
) -> Result<BrowserLoginPrompt, String> {
    let tenant = normalized_tenant(tenant);
    let display_name = display_name.trim().to_string();
    let arm_scope = scope_with_refresh(ARM_SCOPE);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to bind OAuth callback listener: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to prepare OAuth callback listener: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to read OAuth callback address: {error}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}");
    let oauth_state = Uuid::new_v4().to_string();
    let code_verifier = pkce_code_verifier();
    let code_challenge = pkce_code_challenge(&code_verifier);
    let authorize_url = build_authorize_url(
        &tenant,
        &redirect_uri,
        &arm_scope,
        &oauth_state,
        &code_challenge,
    )?;
    let login_id = Uuid::new_v4().to_string();
    let prompt = BrowserLoginPrompt {
        login_id: login_id.clone(),
        authorize_url: authorize_url.clone(),
        redirect_uri: redirect_uri.clone(),
    };

    {
        let mut guard = state.inner.lock().unwrap();
        guard.pending_browser_logins.insert(
            login_id.clone(),
            PendingLogin {
                status: PendingLoginStatus::Pending,
            },
        );
    }

    let client = reqwest::Client::new();
    let inner = state.inner.clone();
    let store = state.store.clone();
    let credential_store = state.credential_store.clone();
    let snapshot_path = state.snapshot_path.clone();
    let login_id_for_task = login_id.clone();
    tauri::async_runtime::spawn(async move {
        let callback_result = tauri::async_runtime::spawn_blocking(move || {
            wait_for_authorization_code(listener, &oauth_state, StdDuration::from_secs(900))
        })
        .await
        .map_err(|error| format!("interactive login task failed: {error}"))
        .and_then(|result| result);

        match callback_result {
            Ok(code) => {
                let token_result = exchange_authorization_code(
                    &client,
                    &tenant,
                    &code,
                    &redirect_uri,
                    &code_verifier,
                    &arm_scope,
                )
                .await;

                match token_result {
                    Ok(token) => {
                        let fallback_label = format!("Azure ({tenant})");
                        let account_label = preferred_account_label(&token);
                        let sign_in_label = normalized_display_name(
                            &display_name,
                            account_label.as_deref().unwrap_or(&fallback_label),
                        );
                        let arm_bundle = token_bundle_from_response(
                            token,
                            client.clone(),
                            tenant.clone(),
                            &arm_scope,
                        );
                        let discovery_result = discover_sign_in_session(
                            sign_in_label,
                            tenant.clone(),
                            ENTRA_INTERACTIVE_AUTH_KIND,
                            arm_bundle,
                        )
                        .await;

                        let mut guard = inner.lock().unwrap();
                        match discovery_result {
                            Ok(sign_in) => {
                                let sign_in_id = sign_in.id.clone();
                                if let Err(error) = persist_sign_in_session_snapshot(
                                    &store,
                                    &*credential_store,
                                    &snapshot_path,
                                    &sign_in,
                                ) {
                                    eprintln!("failed to persist Azure sign-in `{sign_in_id}`: {error}");
                                }
                                guard.sign_ins.insert(sign_in_id.clone(), sign_in);
                                if let Some(pending) =
                                    guard.pending_browser_logins.get_mut(&login_id_for_task)
                                {
                                    pending.status =
                                        PendingLoginStatus::Complete { id: sign_in_id };
                                }
                            }
                            Err(message) => {
                                if let Some(pending) =
                                    guard.pending_browser_logins.get_mut(&login_id_for_task)
                                {
                                    pending.status = PendingLoginStatus::Error { message };
                                }
                            }
                        }
                    }
                    Err(message) => {
                        let mut guard = inner.lock().unwrap();
                        if let Some(pending) =
                            guard.pending_browser_logins.get_mut(&login_id_for_task)
                        {
                            pending.status = PendingLoginStatus::Error { message };
                        }
                    }
                }
            }
            Err(message) => {
                let mut guard = inner.lock().unwrap();
                if let Some(pending) = guard.pending_browser_logins.get_mut(&login_id_for_task) {
                    pending.status = PendingLoginStatus::Error { message };
                }
            }
        }
    });

    if let Err(error) = webbrowser::open(&authorize_url) {
        let mut guard = state.inner.lock().unwrap();
        if let Some(pending) = guard.pending_browser_logins.get_mut(&login_id) {
            pending.status = PendingLoginStatus::Error {
                message: format!("failed to open system browser: {error}"),
            };
        }
        return Err(format!("failed to open system browser: {error}"));
    }

    Ok(prompt)
}

#[tauri::command]
pub fn poll_entra_browser_login(
    state: State<'_, AppState>,
    login_id: String,
) -> Result<DiscoveryLoginStatus, String> {
    let guard = state.inner.lock().unwrap();
    let pending = guard
        .pending_browser_logins
        .get(&login_id)
        .ok_or_else(|| format!("unknown login id `{login_id}`"))?;

    let status = match &pending.status {
        PendingLoginStatus::Pending => DiscoveryLoginStatus {
            status: "pending".into(),
            sign_in_id: None,
            error: None,
        },
        PendingLoginStatus::Complete { id } => DiscoveryLoginStatus {
            status: "complete".into(),
            sign_in_id: Some(id.clone()),
            error: None,
        },
        PendingLoginStatus::Error { message } => DiscoveryLoginStatus {
            status: "error".into(),
            sign_in_id: None,
            error: Some(message.clone()),
        },
    };
    Ok(status)
}

#[tauri::command]
pub async fn start_sign_in_tenant_reauth(
    state: State<'_, AppState>,
    sign_in_id: String,
    tenant_id: String,
) -> Result<BrowserLoginPrompt, String> {
    let sign_in = get_sign_in(&state, &sign_in_id)?;
    let tenant = sign_in
        .tenants
        .iter()
        .find(|tenant| tenant.id == tenant_id)
        .cloned()
        .ok_or_else(|| format!("unknown tenant `{tenant_id}` for sign-in `{sign_in_id}`"))?;

    let arm_scope = scope_with_refresh(ARM_SCOPE);
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to bind OAuth callback listener: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to prepare OAuth callback listener: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to read OAuth callback address: {error}"))?
        .port();
    let redirect_uri = format!("http://localhost:{port}");
    let oauth_state = Uuid::new_v4().to_string();
    let code_verifier = pkce_code_verifier();
    let code_challenge = pkce_code_challenge(&code_verifier);
    let authorize_url = build_authorize_url(
        &tenant.id,
        &redirect_uri,
        &arm_scope,
        &oauth_state,
        &code_challenge,
    )?;
    let login_id = Uuid::new_v4().to_string();
    let prompt = BrowserLoginPrompt {
        login_id: login_id.clone(),
        authorize_url: authorize_url.clone(),
        redirect_uri: redirect_uri.clone(),
    };

    {
        let mut guard = state.inner.lock().unwrap();
        guard.pending_tenant_browser_logins.insert(
            login_id.clone(),
            PendingLogin {
                status: PendingLoginStatus::Pending,
            },
        );
    }

    let client = reqwest::Client::new();
    let inner = state.inner.clone();
    let store = state.store.clone();
    let credential_store = state.credential_store.clone();
    let snapshot_path = state.snapshot_path.clone();
    let login_id_for_task = login_id.clone();
    let sign_in_id_for_task = sign_in_id.clone();
    let tenant_id_for_task = tenant.id.clone();
    tauri::async_runtime::spawn(async move {
        let callback_result = tauri::async_runtime::spawn_blocking(move || {
            wait_for_authorization_code(listener, &oauth_state, StdDuration::from_secs(900))
        })
        .await
        .map_err(|error| format!("interactive tenant login task failed: {error}"))
        .and_then(|result| result);

        match callback_result {
            Ok(code) => {
                let token_result = exchange_authorization_code(
                    &client,
                    &tenant_id_for_task,
                    &code,
                    &redirect_uri,
                    &code_verifier,
                    &arm_scope,
                )
                .await;

                match token_result {
                    Ok(token) => {
                        let arm_bundle = token_bundle_from_response(
                            token,
                            client.clone(),
                            tenant_id_for_task.clone(),
                            &arm_scope,
                        );
                        let refresh_result = refresh_sign_in_tenant(
                            &inner,
                            &store,
                            &*credential_store,
                            &snapshot_path,
                            &sign_in_id_for_task,
                            &tenant_id_for_task,
                            arm_bundle,
                        )
                        .await;

                        let mut guard = inner.lock().unwrap();
                        match refresh_result {
                            Ok(()) => {
                                if let Some(pending) = guard
                                    .pending_tenant_browser_logins
                                    .get_mut(&login_id_for_task)
                                {
                                    pending.status = PendingLoginStatus::Complete {
                                        id: sign_in_id_for_task.clone(),
                                    };
                                }
                            }
                            Err(message) => {
                                if let Some(pending) = guard
                                    .pending_tenant_browser_logins
                                    .get_mut(&login_id_for_task)
                                {
                                    pending.status = PendingLoginStatus::Error { message };
                                }
                            }
                        }
                    }
                    Err(message) => {
                        let mut guard = inner.lock().unwrap();
                        if let Some(pending) = guard
                            .pending_tenant_browser_logins
                            .get_mut(&login_id_for_task)
                        {
                            pending.status = PendingLoginStatus::Error { message };
                        }
                    }
                }
            }
            Err(message) => {
                let mut guard = inner.lock().unwrap();
                if let Some(pending) = guard
                    .pending_tenant_browser_logins
                    .get_mut(&login_id_for_task)
                {
                    pending.status = PendingLoginStatus::Error { message };
                }
            }
        }
    });

    if let Err(error) = webbrowser::open(&authorize_url) {
        let mut guard = state.inner.lock().unwrap();
        if let Some(pending) = guard.pending_tenant_browser_logins.get_mut(&login_id) {
            pending.status = PendingLoginStatus::Error {
                message: format!("failed to open system browser: {error}"),
            };
        }
        return Err(format!("failed to open system browser: {error}"));
    }

    Ok(prompt)
}

#[tauri::command]
pub fn poll_sign_in_tenant_reauth(
    state: State<'_, AppState>,
    login_id: String,
) -> Result<DiscoveryLoginStatus, String> {
    let guard = state.inner.lock().unwrap();
    let pending = guard
        .pending_tenant_browser_logins
        .get(&login_id)
        .ok_or_else(|| format!("unknown login id `{login_id}`"))?;

    let status = match &pending.status {
        PendingLoginStatus::Pending => DiscoveryLoginStatus {
            status: "pending".into(),
            sign_in_id: None,
            error: None,
        },
        PendingLoginStatus::Complete { id } => DiscoveryLoginStatus {
            status: "complete".into(),
            sign_in_id: Some(id.clone()),
            error: None,
        },
        PendingLoginStatus::Error { message } => DiscoveryLoginStatus {
            status: "error".into(),
            sign_in_id: None,
            error: Some(message.clone()),
        },
    };
    Ok(status)
}

#[tauri::command]
pub async fn start_entra_discovery_login(
    state: State<'_, AppState>,
    display_name: String,
    tenant: Option<String>,
) -> Result<DeviceCodePrompt, String> {
    let tenant = normalized_tenant(tenant);
    let display_name = display_name.trim().to_string();
    let arm_scope = scope_with_refresh(ARM_SCOPE);

    let client = reqwest::Client::new();
    let device_code = start_device_code(&client, &tenant, DEFAULT_CLIENT_ID, &arm_scope)
        .await
        .map_err(error_to_string)?;
    let login_id = Uuid::new_v4().to_string();
    let prompt = device_code_prompt(login_id.clone(), &device_code);
    let poll_device_code = device_code.clone();

    {
        let mut guard = state.inner.lock().unwrap();
        guard.pending_discovery_logins.insert(
            login_id.clone(),
            PendingLogin {
                status: PendingLoginStatus::Pending,
            },
        );
    }

    let inner = state.inner.clone();
    let store = state.store.clone();
    let credential_store = state.credential_store.clone();
    let snapshot_path = state.snapshot_path.clone();
    tauri::async_runtime::spawn(async move {
        let poll_result = poll_for_token(
            &client,
            &tenant,
            DEFAULT_CLIENT_ID,
            &poll_device_code.device_code,
            StdDuration::from_secs(poll_device_code.interval),
            StdDuration::from_secs(poll_device_code.expires_in),
        )
        .await;

        match poll_result {
            Ok(token) => {
                let fallback_label = format!("Azure ({tenant})");
                let account_label = preferred_account_label(&token);
                let sign_in_label = normalized_display_name(
                    &display_name,
                    account_label.as_deref().unwrap_or(&fallback_label),
                );
                let arm_bundle =
                    token_bundle_from_response(token, client.clone(), tenant.clone(), &arm_scope);
                let discovery_result = discover_sign_in_session(
                    sign_in_label,
                    tenant.clone(),
                    ENTRA_DEVICE_CODE_AUTH_KIND,
                    arm_bundle,
                )
                .await;

                let mut guard = inner.lock().unwrap();
                match discovery_result {
                    Ok(sign_in) => {
                        let sign_in_id = sign_in.id.clone();
                        if let Err(error) = persist_sign_in_session_snapshot(
                            &store,
                            &*credential_store,
                            &snapshot_path,
                            &sign_in,
                        ) {
                            eprintln!("failed to persist Azure sign-in `{sign_in_id}`: {error}");
                        }
                        guard.sign_ins.insert(sign_in_id.clone(), sign_in);
                        if let Some(pending) = guard.pending_discovery_logins.get_mut(&login_id) {
                            pending.status = PendingLoginStatus::Complete { id: sign_in_id };
                        }
                    }
                    Err(message) => {
                        if let Some(pending) = guard.pending_discovery_logins.get_mut(&login_id) {
                            pending.status = PendingLoginStatus::Error { message };
                        }
                    }
                }
            }
            Err(error) => {
                let mut guard = inner.lock().unwrap();
                if let Some(pending) = guard.pending_discovery_logins.get_mut(&login_id) {
                    pending.status = PendingLoginStatus::Error {
                        message: error_to_string(error),
                    };
                }
            }
        }
    });

    Ok(prompt)
}

#[tauri::command]
pub fn poll_entra_discovery_login(
    state: State<'_, AppState>,
    login_id: String,
) -> Result<DiscoveryLoginStatus, String> {
    let guard = state.inner.lock().unwrap();
    let pending = guard
        .pending_discovery_logins
        .get(&login_id)
        .ok_or_else(|| format!("unknown login id `{login_id}`"))?;

    let status = match &pending.status {
        PendingLoginStatus::Pending => DiscoveryLoginStatus {
            status: "pending".into(),
            sign_in_id: None,
            error: None,
        },
        PendingLoginStatus::Complete { id } => DiscoveryLoginStatus {
            status: "complete".into(),
            sign_in_id: Some(id.clone()),
            error: None,
        },
        PendingLoginStatus::Error { message } => DiscoveryLoginStatus {
            status: "error".into(),
            sign_in_id: None,
            error: Some(message.clone()),
        },
    };
    Ok(status)
}

#[tauri::command]
pub async fn connect_discovered_storage_account(
    state: State<'_, AppState>,
    sign_in_id: String,
    subscription_id: String,
    account_name: String,
) -> Result<BrowserConnection, String> {
    let sign_in = get_sign_in(&state, &sign_in_id)?;
    let (target_tenant_id, account) = sign_in
        .tenants
        .iter()
        .find_map(|tenant| {
            tenant
                .subscriptions
                .iter()
                .find(|subscription| subscription.id == subscription_id)
                .and_then(|subscription| {
                    subscription
                        .storage_accounts
                        .iter()
                        .find(|account| account.name == account_name)
                        .cloned()
                        .map(|account| (tenant.id.clone(), account))
                })
        })
        .ok_or_else(|| {
            format!("unknown storage account `{account_name}` in subscription `{subscription_id}`")
        })?;

    let existing =
        find_existing_discovered_connection(&state, &sign_in_id, &subscription_id, &account.name);
    if matches!(existing.as_ref(), Some(LiveConnection::AccountKey { .. })) {
        return Ok(connection_summary(existing.expect("checked above")));
    }
    let existing_entra_id = existing.as_ref().and_then(|connection| match connection {
        LiveConnection::Entra { id, .. } => Some(id.clone()),
        _ => None,
    });

    let origin = Some(ConnectionOrigin {
        sign_in_id: sign_in_id.clone(),
        subscription_id: subscription_id.clone(),
    });
    // If ARM listKeys succeeds, attach the AccountKey connection directly
    // without probing a data-plane call. This matches Storage Explorer's
    // attach-time behavior: probe only at real use, so activation-time
    // quirks (service-level listing rules, edge auth paths) don't prevent
    // per-container operations that would otherwise succeed.
    let fallback_note =
        match try_fetch_storage_account_key(&sign_in, &target_tenant_id, &account).await {
            Ok(account_key) => {
                let connection = LiveConnection::AccountKey {
                    id: Uuid::new_v4().to_string(),
                    display_name: account.name.clone(),
                    endpoint: account.endpoint.clone(),
                    account_name: account.name.clone(),
                    auth_kind: "entra-managed-key".into(),
                    key: account_key,
                    origin: origin.clone(),
                };
                let summary = connection_summary(connection.clone());
                let mut guard = state.inner.lock().unwrap();
                if let Some(existing_id) = existing
                    .as_ref()
                    .and_then(|value| discovered_connection_id(value))
                {
                    guard.connections.remove(existing_id);
                }
                guard.connections.insert(summary.id.clone(), connection);
                return Ok(summary);
            }
            Err(error) => Some(error),
        };

    let storage_scope = scope_with_refresh(STORAGE_SCOPE);
    let bundle = mint_sign_in_scoped_bundle(&sign_in, &target_tenant_id, &storage_scope).await?;
    let connection = LiveConnection::Entra {
        id: existing_entra_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        display_name: account.name.clone(),
        endpoint: account.endpoint.clone(),
        account_name: account.name.clone(),
        tenant: target_tenant_id,
        auth_kind: sign_in.auth_kind.clone(),
        origin,
        fallback_note: fallback_note.clone(),
        bundle,
    };
    if let Err(error) = validate_browsable_connection(&connection).await {
        return Err(compact_discovered_account_error(
            &account.name,
            &error,
            fallback_note.as_deref(),
        ));
    }
    let summary = connection_summary(connection.clone());
    let mut guard = state.inner.lock().unwrap();
    if let Some(existing_id) = existing
        .as_ref()
        .and_then(|value| discovered_connection_id(value))
    {
        if existing_id != summary.id {
            guard.connections.remove(existing_id);
        }
    }
    guard.connections.insert(summary.id.clone(), connection);
    Ok(summary)
}

#[tauri::command]
pub async fn list_containers(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<BrowserContainer>, String> {
    let connection = get_connection(&state, &connection_id)?;
    if let Some(container) = fixed_container(&connection) {
        return Ok(vec![BrowserContainer {
            id: container.to_string(),
            name: container.to_string(),
            public_access: None,
            lease: None,
            blob_count: None,
        }]);
    }

    let backend = build_backend(&connection).await?;
    let Page { items, .. } = backend.list_containers(None).await.map_err(|error| {
        compact_live_browse_error(
            &connection,
            "Container listing",
            None,
            &error_to_string(error),
        )
    })?;
    Ok(items
        .into_iter()
        .map(|container| BrowserContainer {
            id: container.name.clone(),
            name: container.name,
            public_access: container.public_access,
            lease: container.lease_state.or(container.lease_status),
            blob_count: None,
        })
        .collect())
}

#[tauri::command]
pub async fn list_blobs(
    state: State<'_, AppState>,
    connection_id: String,
    container: String,
    prefix: Option<String>,
) -> Result<Vec<BrowserBlobRow>, String> {
    let connection = get_connection(&state, &connection_id)?;
    let container = resolved_container_name(&connection, &container)?;
    let prefix = normalize_prefix(prefix);
    let backend = build_backend(&connection).await?;
    let Page { items, .. } = backend
        .list_blobs(&container, prefix.as_deref(), Some("/"), None)
        .await
        .map_err(|error| {
            compact_live_browse_error(
                &connection,
                "Blob listing",
                Some(container.as_str()),
                &error_to_string(error),
            )
        })?;

    Ok(items
        .into_iter()
        .map(|entry| blob_entry_to_row(entry, prefix.as_deref()))
        .collect())
}

#[tauri::command]
pub async fn upload_blob(
    state: State<'_, AppState>,
    connection_id: String,
    container: String,
    source_path: String,
    destination_prefix: Option<String>,
    overwrite: bool,
) -> Result<BlobUploadResult, String> {
    const MAX_INLINE_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

    let connection = get_connection(&state, &connection_id)?;
    let container = resolved_container_name(&connection, &container)?;
    let source_path = PathBuf::from(source_path);
    let file_name = source_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("upload path `{}` does not contain a file name", source_path.display()))?
        .to_string();
    let metadata = std::fs::metadata(&source_path).map_err(|error| {
        format!(
            "failed to inspect upload source `{}`: {error}",
            source_path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "upload source `{}` is not a regular file",
            source_path.display()
        ));
    }
    if metadata.len() > MAX_INLINE_UPLOAD_BYTES {
        return Err(format!(
            "upload source `{}` is {} bytes; this build supports single-shot uploads up to {} bytes until chunked upload lands",
            source_path.display(),
            metadata.len(),
            MAX_INLINE_UPLOAD_BYTES
        ));
    }

    let prefix = normalize_prefix(destination_prefix).unwrap_or_default();
    let blob_path = normalize_blob_path(&format!("{prefix}{file_name}"))?;
    let bytes = std::fs::read(&source_path).map_err(|error| {
        format!(
            "failed to read upload source `{}`: {error}",
            source_path.display()
        )
    })?;
    let byte_count = bytes.len() as u64;
    let body: ByteStream =
        stream::once(async move { Ok::<Bytes, arkived_core::Error>(Bytes::from(bytes)) }).boxed();
    let backend = build_backend(&connection).await?;
    let ctx = app_operation_ctx();
    let result = backend
        .write_blob(
            &ctx,
            &BlobPath::new(container.clone(), blob_path.clone()),
            body,
            WriteOpts {
                overwrite,
                content_type: infer_content_type(&source_path),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| {
            compact_live_browse_error(
                &connection,
                "Blob upload",
                Some(container.as_str()),
                &error_to_string(error),
            )
        })?;

    Ok(BlobUploadResult {
        path: blob_path,
        bytes: byte_count,
        etag: result.etag,
    })
}

#[tauri::command]
pub async fn download_blob(
    state: State<'_, AppState>,
    connection_id: String,
    container: String,
    path: String,
    open_after_download: bool,
) -> Result<BlobDownloadResult, String> {
    let connection = get_connection(&state, &connection_id)?;
    let container = resolved_container_name(&connection, &container)?;
    let blob_path = normalize_blob_path(&path)?;
    let backend = build_backend(&connection).await?;
    let target_path = unique_download_path(&blob_path)?;
    let bytes = stream_blob_to_file(&backend, &container, &blob_path, &target_path)
        .await
        .map_err(|error| {
            compact_live_browse_error(
                &connection,
                "Blob download",
                Some(container.as_str()),
                &error,
            )
        })?;

    let mut opened = false;
    if open_after_download {
        match url::Url::from_file_path(&target_path) {
            Ok(file_url) => match webbrowser::open(file_url.as_str()) {
                Ok(()) => opened = true,
                Err(error) => eprintln!(
                    "failed to open downloaded blob `{}`: {error}",
                    target_path.display()
                ),
            },
            Err(()) => eprintln!(
                "failed to convert downloaded blob path `{}` to a file URL",
                target_path.display()
            ),
        }
    }

    Ok(BlobDownloadResult {
        path: target_path.to_string_lossy().into_owned(),
        bytes,
        opened,
    })
}

#[tauri::command]
pub async fn delete_blob(
    state: State<'_, AppState>,
    connection_id: String,
    container: String,
    path: String,
    include_snapshots: bool,
) -> Result<(), String> {
    let connection = get_connection(&state, &connection_id)?;
    let container = resolved_container_name(&connection, &container)?;
    let blob_path = normalize_blob_path(&path)?;
    let backend = build_backend(&connection).await?;
    let ctx = app_operation_ctx();
    backend
        .delete_blob(
            &ctx,
            &BlobPath::new(container.clone(), blob_path.clone()),
            DeleteOpts { include_snapshots },
        )
        .await
        .map_err(|error| {
            compact_live_browse_error(
                &connection,
                "Blob delete",
                Some(container.as_str()),
                &error_to_string(error),
            )
        })
}

#[tauri::command]
pub fn disconnect_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    let removed = state
        .inner
        .lock()
        .unwrap()
        .connections
        .remove(&connection_id);
    if removed.is_some() {
        if let Some(connection) = removed.as_ref() {
            if let Err(error) = remove_persisted_direct_connection(&state, connection) {
                eprintln!("failed to remove persisted connection `{connection_id}`: {error}");
            }
        }
        Ok(())
    } else {
        Err(format!("unknown connection id `{connection_id}`"))
    }
}

#[tauri::command]
pub fn list_activities() -> Vec<Activity> {
    Vec::new()
}

#[tauri::command]
pub fn agent_transcript() -> serde_json::Value {
    serde_json::json!([])
}

fn keychain_ref_for_connection(connection_id: &str) -> String {
    format!("{KEYCHAIN_CONNECTION_PREFIX}{connection_id}")
}

fn connection_id(connection: &LiveConnection) -> &str {
    match connection {
        LiveConnection::ConnectionString { id, .. }
        | LiveConnection::AccountKey { id, .. }
        | LiveConnection::Sas { id, .. }
        | LiveConnection::Azurite { id, .. }
        | LiveConnection::Entra { id, .. } => id,
    }
}

fn replace_connection_id(connection: LiveConnection, next_id: String) -> LiveConnection {
    match connection {
        LiveConnection::ConnectionString {
            display_name,
            endpoint,
            raw,
            fixed_container,
            ..
        } => LiveConnection::ConnectionString {
            id: next_id,
            display_name,
            endpoint,
            raw,
            fixed_container,
        },
        LiveConnection::AccountKey {
            display_name,
            endpoint,
            account_name,
            auth_kind,
            key,
            origin,
            ..
        } => LiveConnection::AccountKey {
            id: next_id,
            display_name,
            endpoint,
            account_name,
            auth_kind,
            key,
            origin,
        },
        LiveConnection::Sas {
            display_name,
            endpoint,
            sas,
            fixed_container,
            ..
        } => LiveConnection::Sas {
            id: next_id,
            display_name,
            endpoint,
            sas,
            fixed_container,
        },
        LiveConnection::Azurite { display_name, .. } => LiveConnection::Azurite {
            id: next_id,
            display_name,
        },
        LiveConnection::Entra {
            display_name,
            endpoint,
            account_name,
            tenant,
            auth_kind,
            origin,
            fallback_note,
            bundle,
            ..
        } => LiveConnection::Entra {
            id: next_id,
            display_name,
            endpoint,
            account_name,
            tenant,
            auth_kind,
            origin,
            fallback_note,
            bundle,
        },
    }
}

fn persist_sign_in_session_snapshot(
    store: &Store,
    credential_store: &dyn CredentialStore,
    snapshot_path: &Path,
    sign_in: &SignInSession,
) -> Result<(), String> {
    let existing = store
        .sign_in_get(&sign_in.id)
        .map_err(|error| format!("failed to inspect persisted sign-in `{}`: {error}", sign_in.id))?;
    if existing.is_some() {
        store
            .sign_in_delete(&sign_in.id)
            .map_err(|error| format!("failed to replace persisted sign-in `{}`: {error}", sign_in.id))?;
    }

    store
        .sign_in_insert(&SignIn {
            id: sign_in.id.clone(),
            display_name: sign_in.display_name.clone(),
            tenant_id: sign_in.login_tenant.clone(),
            environment: sign_in.environment.clone(),
            user_principal: sign_in.display_name.clone(),
            added_at: existing.map(|value| value.added_at).unwrap_or_else(Utc::now),
        })
        .map_err(|error| format!("failed to persist sign-in `{}`: {error}", sign_in.id))?;

    let refresh_context = sign_in
        .arm_bundle
        .refresh_context
        .as_ref()
        .ok_or_else(|| format!("sign-in `{}` is missing refresh context", sign_in.id))?;
    let refresh_token = sign_in
        .arm_bundle
        .refresh_token
        .clone()
        .ok_or_else(|| format!("sign-in `{}` is missing refresh token", sign_in.id))?;
    let cache = RefreshCache::new(credential_store);
    cache
        .put(
            &sign_in.id,
            &CachedRefresh {
                refresh_token,
                tenant: refresh_context.tenant.clone(),
                client_id: refresh_context.client_id.clone(),
                scope: refresh_context.scope.clone(),
                obtained_at: OffsetDateTime::now_utc(),
            },
        )
        .map_err(|error| format!("failed to cache refresh token for `{}`: {error}", sign_in.id))?;

    persist_sign_in_snapshot_metadata(snapshot_path, sign_in)?;

    Ok(())
}

fn persist_sign_in_snapshot_metadata(snapshot_path: &Path, sign_in: &SignInSession) -> Result<(), String> {
    let mut shell_state = load_persisted_shell_state(snapshot_path)?;
    let snapshot = PersistedSignInSnapshot {
        id: sign_in.id.clone(),
        display_name: sign_in.display_name.clone(),
        login_tenant: sign_in.login_tenant.clone(),
        environment: sign_in.environment.clone(),
        auth_kind: sign_in.auth_kind.clone(),
        tenants: sign_in.tenants.clone(),
    };

    if let Some(existing) = shell_state
        .sign_ins
        .iter_mut()
        .find(|existing| existing.id == snapshot.id)
    {
        *existing = snapshot;
    } else {
        shell_state.sign_ins.push(snapshot);
    }

    write_persisted_shell_state(snapshot_path, &shell_state)
}

fn load_persisted_shell_state(snapshot_path: &Path) -> Result<PersistedShellState, String> {
    match std::fs::read_to_string(snapshot_path) {
        Ok(json) => serde_json::from_str(&json)
            .map_err(|error| format!("failed to parse shell state `{}`: {error}", snapshot_path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(PersistedShellState::default()),
        Err(error) => Err(format!(
            "failed to read shell state `{}`: {error}",
            snapshot_path.display()
        )),
    }
}

fn write_persisted_shell_state(
    snapshot_path: &Path,
    shell_state: &PersistedShellState,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(shell_state)
        .map_err(|error| format!("failed to serialize shell state: {error}"))?;
    std::fs::write(snapshot_path, json).map_err(|error| {
        format!(
            "failed to write shell state `{}`: {error}",
            snapshot_path.display()
        )
    })
}

async fn restore_sign_in_session(
    credential_store: &dyn CredentialStore,
    persisted: SignIn,
) -> Result<SignInSession, String> {
    let cache = RefreshCache::new(credential_store);
    let cached = cache
        .get(&persisted.id)
        .map_err(|error| format!("failed to read refresh token for `{}`: {error}", persisted.id))?
        .ok_or_else(|| format!("missing refresh token for `{}`", persisted.display_name))?;

    let client = reqwest::Client::new();
    let refreshed = refresh_access_token(
        &client,
        &cached.tenant,
        &cached.client_id,
        &cached.refresh_token,
        &cached.scope,
    )
    .await
    .map_err(error_to_string)?;

    discover_sign_in_session_with_id(
        persisted.id,
        persisted.display_name,
        persisted.tenant_id,
        ENTRA_INTERACTIVE_AUTH_KIND,
        TokenBundle {
            access_token: refreshed.access_token,
            refresh_token: refreshed.refresh_token.or(Some(cached.refresh_token)),
            expires_at: OffsetDateTime::now_utc()
                + TimeDuration::seconds(refreshed.expires_in as i64),
            refresh_context: Some(RefreshContext {
                client,
                tenant: cached.tenant,
                client_id: cached.client_id,
                scope: cached.scope,
            }),
        },
    )
    .await
}

fn restore_sign_in_session_from_snapshot(
    credential_store: &dyn CredentialStore,
    persisted: SignIn,
    snapshot: PersistedSignInSnapshot,
) -> Result<SignInSession, String> {
    let cache = RefreshCache::new(credential_store);
    let cached = cache
        .get(&persisted.id)
        .map_err(|error| format!("failed to read refresh token for `{}`: {error}", persisted.id))?
        .ok_or_else(|| format!("missing refresh token for `{}`", persisted.display_name))?;

    let client = reqwest::Client::new();
    let mut tenants = snapshot.tenants;
    tenants.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    Ok(SignInSession {
        id: persisted.id,
        display_name: if snapshot.display_name.trim().is_empty() {
            persisted.display_name
        } else {
            snapshot.display_name
        },
        login_tenant: if snapshot.login_tenant.trim().is_empty() {
            persisted.tenant_id
        } else {
            snapshot.login_tenant
        },
        environment: if snapshot.environment.trim().is_empty() {
            persisted.environment
        } else {
            snapshot.environment
        },
        auth_kind: if snapshot.auth_kind.trim().is_empty() {
            ENTRA_INTERACTIVE_AUTH_KIND.into()
        } else {
            snapshot.auth_kind
        },
        arm_bundle: TokenBundle {
            access_token: String::new(),
            refresh_token: Some(cached.refresh_token),
            expires_at: OffsetDateTime::now_utc() - TimeDuration::minutes(5),
            refresh_context: Some(RefreshContext {
                client,
                tenant: cached.tenant,
                client_id: cached.client_id,
                scope: cached.scope,
            }),
        },
        tenant_bundles: HashMap::new(),
        tenants,
    })
}

fn persist_direct_connection(state: &AppState, connection: &LiveConnection) -> Result<(), String> {
    let id = connection_id(connection);
    let keychain_ref = keychain_ref_for_connection(id);
    let (display_name, endpoint, auth_kind, secret_payload) = match connection {
        LiveConnection::ConnectionString {
            display_name,
            endpoint,
            raw,
            ..
        } => (
            display_name.clone(),
            endpoint.clone(),
            AuthKind::ConnectionString,
            Some(raw.clone()),
        ),
        LiveConnection::AccountKey {
            display_name,
            endpoint,
            account_name,
            key,
            origin: None,
            ..
        } => (
            display_name.clone(),
            endpoint.clone(),
            AuthKind::AccountKey,
            Some(
                serde_json::to_string(&PersistedAccountKeySecret {
                    account_name: account_name.clone(),
                    account_key: key.clone(),
                })
                .map_err(|error| format!("failed to serialize shared-key connection `{id}`: {error}"))?,
            ),
        ),
        LiveConnection::Sas {
            display_name,
            endpoint,
            sas,
            fixed_container,
            ..
        } => (
            display_name.clone(),
            endpoint.clone(),
            AuthKind::SasToken,
            Some(
                serde_json::to_string(&PersistedSasSecret {
                    sas: sas.clone(),
                    fixed_container: fixed_container.clone(),
                })
                .map_err(|error| format!("failed to serialize SAS connection `{id}`: {error}"))?,
            ),
        ),
        LiveConnection::Azurite { display_name, .. } => (
            display_name.clone(),
            arkived_core::auth::azurite::AZURITE_BLOB_ENDPOINT.to_string(),
            AuthKind::AzuriteEmulator,
            None,
        ),
        _ => return Ok(()),
    };

    if let Some(secret_payload) = secret_payload {
        state
            .credential_store
            .put(&keychain_ref, &SecretString::new(secret_payload))
            .map_err(|error| format!("failed to cache connection secret `{id}`: {error}"))?;
    }

    let _ = state.store.attached_resource_delete(id);
    state
        .store
        .attached_resource_insert(&AttachedResource {
            id: id.to_string(),
            display_name,
            resource_kind: ResourceKind::StorageAccount,
            endpoint,
            auth_kind,
            keychain_ref,
        })
        .map_err(|error| format!("failed to persist connection `{id}`: {error}"))?;

    Ok(())
}

fn remove_persisted_direct_connection(
    state: &AppState,
    connection: &LiveConnection,
) -> Result<(), String> {
    match connection {
        LiveConnection::ConnectionString { .. }
        | LiveConnection::Sas { .. }
        | LiveConnection::Azurite { .. }
        | LiveConnection::AccountKey { origin: None, .. } => {
            let id = connection_id(connection);
            state
                .store
                .attached_resource_delete(id)
                .map_err(|error| format!("failed to delete persisted connection `{id}`: {error}"))?;
            state
                .credential_store
                .delete(&keychain_ref_for_connection(id))
                .map_err(|error| format!("failed to delete cached secret for `{id}`: {error}"))?;
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn restore_direct_attachment(
    credential_store: &dyn CredentialStore,
    attachment: AttachedResource,
) -> Result<LiveConnection, String> {
    let connection = match attachment.auth_kind {
        AuthKind::ConnectionString => {
            let secret = credential_store
                .get(&attachment.keychain_ref)
                .map_err(|error| format!("failed to read connection string for `{}`: {error}", attachment.display_name))?;
            build_connection_string_connection(attachment.display_name.clone(), secret.expose_secret().to_string()).await?
        }
        AuthKind::AccountKey => {
            let secret = credential_store
                .get(&attachment.keychain_ref)
                .map_err(|error| format!("failed to read shared key for `{}`: {error}", attachment.display_name))?;
            let payload: PersistedAccountKeySecret = serde_json::from_str(secret.expose_secret())
                .map_err(|error| format!("failed to parse shared-key secret for `{}`: {error}", attachment.display_name))?;
            build_account_key_connection(
                attachment.display_name.clone(),
                payload.account_name,
                payload.account_key,
                Some(attachment.endpoint.clone()),
            )?
        }
        AuthKind::SasToken => {
            let secret = credential_store
                .get(&attachment.keychain_ref)
                .map_err(|error| format!("failed to read SAS token for `{}`: {error}", attachment.display_name))?;
            let payload: PersistedSasSecret = serde_json::from_str(secret.expose_secret())
                .map_err(|error| format!("failed to parse SAS secret for `{}`: {error}", attachment.display_name))?;
            build_sas_connection(
                attachment.display_name.clone(),
                attachment.endpoint.clone(),
                payload.sas,
                payload.fixed_container,
            )?
        }
        AuthKind::AzuriteEmulator => LiveConnection::Azurite {
            id: attachment.id.clone(),
            display_name: attachment.display_name.clone(),
        },
        _ => {
            return Err(format!(
                "auth kind `{:?}` is not supported for persisted direct attachments",
                attachment.auth_kind
            ))
        }
    };

    Ok(replace_connection_id(connection, attachment.id))
}

fn get_connection(
    state: &State<'_, AppState>,
    connection_id: &str,
) -> Result<LiveConnection, String> {
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .get(connection_id)
        .cloned()
        .ok_or_else(|| format!("unknown connection id `{connection_id}`"))
}

fn get_sign_in(state: &State<'_, AppState>, sign_in_id: &str) -> Result<SignInSession, String> {
    state
        .inner
        .lock()
        .unwrap()
        .sign_ins
        .get(sign_in_id)
        .cloned()
        .ok_or_else(|| format!("unknown sign-in id `{sign_in_id}`"))
}

fn find_existing_discovered_connection(
    state: &State<'_, AppState>,
    sign_in_id: &str,
    subscription_id: &str,
    account_name: &str,
) -> Option<LiveConnection> {
    state
        .inner
        .lock()
        .unwrap()
        .connections
        .values()
        .find(|connection| {
            matches!(
                connection,
                LiveConnection::Entra {
                    account_name: existing_account_name,
                    origin: Some(ConnectionOrigin {
                        sign_in_id: existing_sign_in_id,
                        subscription_id: existing_subscription_id,
                    }),
                    ..
                } if existing_account_name == account_name
                    && existing_sign_in_id == sign_in_id
                    && existing_subscription_id == subscription_id
            ) || matches!(
                connection,
                LiveConnection::AccountKey {
                    account_name: existing_account_name,
                    origin: Some(ConnectionOrigin {
                        sign_in_id: existing_sign_in_id,
                        subscription_id: existing_subscription_id,
                    }),
                    ..
                } if existing_account_name == account_name
                    && existing_sign_in_id == sign_in_id
                    && existing_subscription_id == subscription_id
            )
        })
        .cloned()
}

fn fixed_container(connection: &LiveConnection) -> Option<&str> {
    match connection {
        LiveConnection::ConnectionString {
            fixed_container, ..
        }
        | LiveConnection::Sas {
            fixed_container, ..
        } => fixed_container.as_deref(),
        _ => None,
    }
}

fn discovered_connection_id(connection: &LiveConnection) -> Option<&str> {
    match connection {
        LiveConnection::AccountKey { id, origin, .. }
        | LiveConnection::Entra { id, origin, .. } => origin.as_ref().map(|_| id.as_str()),
        _ => None,
    }
}

fn resolved_container_name<'a>(
    connection: &'a LiveConnection,
    requested: &'a str,
) -> Result<String, String> {
    match fixed_container(connection) {
        Some(container) if requested.is_empty() || requested == container => {
            Ok(container.to_string())
        }
        Some(container) => Err(format!(
            "this connection is scoped to container `{container}`, not `{requested}`"
        )),
        None => {
            let trimmed = requested.trim();
            if trimmed.is_empty() {
                Err("container name is required".into())
            } else {
                Ok(trimmed.to_string())
            }
        }
    }
}

async fn build_backend(connection: &LiveConnection) -> Result<AzureBlobBackend, String> {
    match connection {
        LiveConnection::ConnectionString { endpoint, raw, .. } => {
            let provider =
                ConnectionStringProvider::new("connection-string", SecretString::new(raw.clone()))
                    .map_err(error_to_string)?;
            let credential = provider.resolve().await.map_err(error_to_string)?;
            let endpoint = parse_endpoint(endpoint)?;
            AzureBlobBackend::new(endpoint, credential).map_err(error_to_string)
        }
        LiveConnection::AccountKey {
            endpoint,
            account_name,
            key,
            ..
        } => {
            let provider =
                AccountKeyProvider::new(account_name.clone(), SecretString::new(key.clone()));
            let credential = provider.resolve().await.map_err(error_to_string)?;
            let endpoint = parse_endpoint(endpoint)?;
            AzureBlobBackend::new(endpoint, credential).map_err(error_to_string)
        }
        LiveConnection::Sas { endpoint, sas, .. } => {
            let provider = SasTokenProvider::new("sas", SecretString::new(sas.clone()))
                .map_err(error_to_string)?;
            let credential = provider.resolve().await.map_err(error_to_string)?;
            let endpoint = parse_endpoint(endpoint)?;
            AzureBlobBackend::new(endpoint, credential).map_err(error_to_string)
        }
        LiveConnection::Azurite { .. } => {
            let provider = AzuriteEmulatorProvider::new();
            let credential = provider.resolve().await.map_err(error_to_string)?;
            let endpoint = parse_endpoint(arkived_core::auth::azurite::AZURITE_BLOB_ENDPOINT)?;
            AzureBlobBackend::new(endpoint, credential).map_err(error_to_string)
        }
        LiveConnection::Entra {
            endpoint, bundle, ..
        } => {
            let endpoint = parse_endpoint(endpoint)?;
            let credential = ResolvedCredential::Entra(Arc::new(
                arkived_core::auth::entra::credential::EntraTokenCredential::new(bundle.clone()),
            ));
            AzureBlobBackend::new(endpoint, credential).map_err(error_to_string)
        }
    }
}

async fn validate_connection(connection: &LiveConnection) -> Result<(), String> {
    let backend = build_backend(connection).await?;
    match fixed_container(connection) {
        Some(container) => {
            let _ = backend
                .list_blobs(container, None, Some("/"), None)
                .await
                .map_err(error_to_string)?;
            Ok(())
        }
        None => {
            let _ = backend
                .list_containers(None)
                .await
                .map_err(error_to_string)?;
            Ok(())
        }
    }
}

async fn validate_browsable_connection(connection: &LiveConnection) -> Result<(), String> {
    // Do not probe an arbitrary first container during account activation.
    // Storage accounts can have container-scoped access differences, and
    // Storage Explorer still lets the account activate before a specific
    // container is chosen.
    validate_connection(connection).await
}

fn app_operation_ctx() -> Ctx {
    Ctx::new(
        Arc::new(AzuriteEmulatorProvider::new()),
        Arc::new(AllowAllPolicy),
    )
}

fn normalize_blob_path(path: &str) -> Result<String, String> {
    let normalized = path.trim().trim_start_matches('/').replace('\\', "/");
    if normalized.is_empty() {
        Err("blob path is required".into())
    } else {
        Ok(normalized)
    }
}

async fn stream_blob_to_file(
    backend: &AzureBlobBackend,
    container: &str,
    blob_path: &str,
    target_path: &Path,
) -> Result<u64, String> {
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create download directory `{}`: {error}",
                parent.display()
            )
        })?;
    }

    let mut file = std::fs::File::create(target_path).map_err(|error| {
        format!(
            "failed to create download file `{}`: {error}",
            target_path.display()
        )
    })?;
    let mut stream = backend
        .read_blob(&BlobPath::new(container, blob_path), None)
        .await
        .map_err(error_to_string)?;
    let mut total = 0u64;

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(error_to_string)?;
        file.write_all(&bytes).map_err(|error| {
            format!(
                "failed to write download file `{}`: {error}",
                target_path.display()
            )
        })?;
        total += bytes.len() as u64;
    }
    file.flush().map_err(|error| {
        format!(
            "failed to flush download file `{}`: {error}",
            target_path.display()
        )
    })?;

    Ok(total)
}

fn unique_download_path(blob_path: &str) -> Result<PathBuf, String> {
    let downloads_dir = default_downloads_dir()?;
    let file_name = blob_path
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("download");
    let sanitized = sanitize_file_name(file_name);
    let candidate = downloads_dir.join(&sanitized);
    if !candidate.exists() {
        return Ok(candidate);
    }

    let path = Path::new(&sanitized);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..1000 {
        let next_name = match extension {
            Some(extension) if !extension.is_empty() => format!("{stem} ({index}).{extension}"),
            _ => format!("{stem} ({index})"),
        };
        let next = downloads_dir.join(next_name);
        if !next.exists() {
            return Ok(next);
        }
    }

    Err(format!(
        "could not choose a unique download path for `{}`",
        sanitized
    ))
}

fn default_downloads_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "could not locate the user home directory for downloads".to_string())?;
    Ok(home.join("Downloads"))
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let trimmed = sanitized.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "download".into()
    } else {
        trimmed
    }
}

fn infer_content_type(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;

    let content_type = match extension.as_str() {
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "css" => "text/css",
        "csv" => "text/csv",
        "gif" => "image/gif",
        "htm" | "html" => "text/html",
        "jpeg" | "jpg" => "image/jpeg",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "md" => "text/markdown",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" | "log" => "text/plain",
        "webp" => "image/webp",
        "xml" => "application/xml",
        "zip" => "application/zip",
        _ => return None,
    };

    Some(content_type.into())
}

async fn try_fetch_storage_account_key(
    sign_in: &SignInSession,
    tenant_id: &str,
    account: &DiscoveredStorageAccount,
) -> Result<String, String> {
    let resource_id = account
        .resource_id
        .as_deref()
        .map(str::trim)
        .ok_or_else(|| {
            "Azure Resource Manager did not return a resource ID for this storage account, so Arkived could not request account keys.".to_string()
        })?;
    if resource_id.is_empty() {
        return Err(
            "Azure Resource Manager returned an empty resource ID for this storage account, so Arkived could not request account keys."
                .into(),
        );
    }

    let mut attempts = Vec::new();
    if let Some(bundle) = sign_in.tenant_bundles.get(tenant_id) {
        attempts.push(("cached tenant ARM token", bundle.clone()));
    }
    if sign_in
        .arm_bundle
        .refresh_context
        .as_ref()
        .map(|ctx| ctx.tenant.eq_ignore_ascii_case(tenant_id))
        .unwrap_or(false)
    {
        attempts.push(("initial login ARM token", sign_in.arm_bundle.clone()));
    }

    let mut attempt_errors = Vec::new();
    for (label, bundle) in attempts {
        match request_storage_account_key(&bundle, resource_id).await {
            Ok(key) => return Ok(key),
            Err(error) => attempt_errors.push(format!("{label}: {error}")),
        }
    }

    let arm_scope = scope_with_refresh(ARM_SCOPE);
    match mint_sign_in_scoped_bundle(sign_in, tenant_id, &arm_scope).await {
        Ok(bundle) => match request_storage_account_key(&bundle, resource_id).await {
            Ok(key) => Ok(key),
            Err(error) => {
                attempt_errors.push(format!("refreshed tenant ARM token: {error}"));
                Err(compact_key_fallback_note(&attempt_errors))
            }
        },
        Err(error) => {
            attempt_errors.push(format!(
                "refreshed tenant ARM token: {}",
                compact_arm_token_error(&error)
            ));
            Err(compact_key_fallback_note(&attempt_errors))
        }
    }
}

async fn build_connection_string_connection(
    display_name: String,
    connection_string: String,
) -> Result<LiveConnection, String> {
    let parts = ConnectionStringParts::parse(&connection_string).map_err(error_to_string)?;
    let endpoint = parts
        .blob_endpoint()
        .ok_or_else(|| "connection string does not define a blob endpoint".to_string())?;
    let endpoint = parse_endpoint(&endpoint)?.to_string();
    let display_name = normalized_display_name(
        &display_name,
        parts.account_name().unwrap_or("connection-string"),
    );

    Ok(LiveConnection::ConnectionString {
        id: Uuid::new_v4().to_string(),
        display_name,
        endpoint,
        raw: connection_string,
        fixed_container: None,
    })
}

fn build_account_key_connection(
    display_name: String,
    account_name: String,
    account_key: String,
    endpoint: Option<String>,
) -> Result<LiveConnection, String> {
    let account_name = account_name.trim().to_string();
    if account_name.is_empty() {
        return Err("storage account name is required".into());
    }
    if account_key.trim().is_empty() {
        return Err("storage account key is required".into());
    }

    let endpoint = match endpoint.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => parse_endpoint(raw)?.to_string(),
        None => AzureBlobBackend::for_account(
            &account_name,
            &AzureEnvironment::Public,
            ResolvedCredential::Anonymous,
        )
        .map_err(error_to_string)?
        .endpoint()
        .to_string(),
    };

    Ok(LiveConnection::AccountKey {
        id: Uuid::new_v4().to_string(),
        display_name: normalized_display_name(&display_name, &account_name),
        endpoint,
        account_name,
        auth_kind: "account-key".into(),
        key: account_key,
        origin: None,
    })
}

fn build_sas_connection(
    display_name: String,
    endpoint: String,
    sas: String,
    fixed_container: Option<String>,
) -> Result<LiveConnection, String> {
    if sas.trim().is_empty() {
        return Err("SAS token is required".into());
    }
    let endpoint = parse_endpoint(&endpoint)?.to_string();
    let fixed_container = fixed_container
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    Ok(LiveConnection::Sas {
        id: Uuid::new_v4().to_string(),
        display_name: normalized_display_name(&display_name, "sas-connection"),
        endpoint,
        sas,
        fixed_container,
    })
}

async fn discover_sign_in_session(
    display_name: String,
    tenant: String,
    auth_kind: &str,
    arm_bundle: TokenBundle,
) -> Result<SignInSession, String> {
    discover_sign_in_session_with_id(
        Uuid::new_v4().to_string(),
        display_name,
        tenant,
        auth_kind,
        arm_bundle,
    )
    .await
}

async fn discover_sign_in_session_with_id(
    sign_in_id: String,
    display_name: String,
    tenant: String,
    auth_kind: &str,
    arm_bundle: TokenBundle,
) -> Result<SignInSession, String> {
    if arm_bundle.refresh_token.is_none() {
        return Err(
            "Azure did not return a refresh token for this sign-in. Use browser OAuth sign-in or sign in again with offline access."
                .into(),
        );
    }

    let ctx = arm_bundle
        .refresh_context
        .clone()
        .ok_or_else(|| "ARM sign-in did not include refresh context".to_string())?;
    let (mut tenants, tenant_bundles) =
        discover_tenant_graph(&ctx.client, &tenant, &arm_bundle).await?;
    tenants.sort_by(|a, b| a.display_name.cmp(&b.display_name));

    Ok(SignInSession {
        id: sign_in_id,
        display_name,
        login_tenant: tenant,
        environment: "azure".into(),
        auth_kind: auth_kind.into(),
        arm_bundle,
        tenant_bundles,
        tenants,
    })
}

async fn discover_tenant_graph(
    client: &reqwest::Client,
    login_tenant: &str,
    arm_bundle: &TokenBundle,
) -> Result<(Vec<DiscoveredTenant>, HashMap<String, TokenBundle>), String> {
    let mut tenants = discover_tenants(client, &arm_bundle.access_token).await?;
    if tenants.is_empty() {
        return Err("Azure did not return any tenants for this account".into());
    }

    let arm_scope = scope_with_refresh(ARM_SCOPE);
    let mut tenant_bundles = HashMap::new();
    for tenant in &mut tenants {
        match mint_scoped_bundle(arm_bundle, &tenant.id, &arm_scope).await {
            Ok(tenant_bundle) => {
                match discover_subscriptions_and_accounts(
                    client,
                    &tenant_bundle.access_token,
                    &tenant.id,
                )
                .await
                {
                    Ok(mut subscriptions) => {
                        subscriptions.sort_by(|a, b| a.name.cmp(&b.name));
                        tenant.error = None;
                        tenant.needs_reauth = false;
                        tenant.subscriptions = subscriptions;
                        tenant_bundles.insert(tenant.id.clone(), tenant_bundle);
                    }
                    Err(error) => {
                        tenant.needs_reauth = requires_tenant_reauth(&error);
                        tenant.error = Some(compact_tenant_error(&error));
                    }
                }
            }
            Err(error) => {
                tenant.needs_reauth = requires_tenant_reauth(&error);
                tenant.error = Some(compact_tenant_error(&error));
            }
        }
    }

    apply_initial_tenant_selection(&mut tenants, login_tenant);
    Ok((tenants, tenant_bundles))
}

async fn discover_tenants(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<Vec<DiscoveredTenant>, String> {
    let url = format!("https://management.azure.com/tenants?api-version={ARM_TENANTS_API_VERSION}");
    let arm_tenants = arm_get_paged::<ArmTenantItem>(client, access_token, url).await?;

    let mut seen = HashSet::new();
    let mut discovered = Vec::new();
    for tenant in arm_tenants {
        let tenant_id = tenant.tenant_id.trim().to_string();
        if tenant_id.is_empty() || !seen.insert(tenant_id.clone()) {
            continue;
        }

        let default_domain = tenant
            .default_domain
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                tenant
                    .domains
                    .into_iter()
                    .find(|value| !value.trim().is_empty())
            });
        let fallback = default_domain.as_deref().unwrap_or(tenant_id.as_str());
        let display_name =
            normalized_display_name(tenant.display_name.as_deref().unwrap_or_default(), fallback);

        discovered.push(DiscoveredTenant {
            id: tenant_id,
            display_name,
            default_domain,
            selected: true,
            needs_reauth: false,
            error: None,
            subscriptions: Vec::new(),
        });
    }

    Ok(discovered)
}

fn apply_initial_tenant_selection(tenants: &mut [DiscoveredTenant], login_tenant: &str) {
    let select_all = matches!(login_tenant, "common" | "organizations");
    let normalized_hint = login_tenant.trim().to_ascii_lowercase();
    let mut any_selected = false;

    for tenant in tenants.iter_mut() {
        let selected = if tenant.error.is_some() || tenant.subscriptions.is_empty() {
            false
        } else if select_all {
            true
        } else {
            tenant.id.eq_ignore_ascii_case(login_tenant)
                || tenant
                    .default_domain
                    .as_deref()
                    .map(|value| value.eq_ignore_ascii_case(login_tenant))
                    .unwrap_or(false)
                || tenant.display_name.to_ascii_lowercase() == normalized_hint
        };
        tenant.selected = selected;
        for subscription in &mut tenant.subscriptions {
            subscription.selected = selected;
        }
        any_selected |= selected;
    }

    if !any_selected {
        for tenant in tenants.iter_mut() {
            if tenant.error.is_some() || tenant.subscriptions.is_empty() {
                continue;
            }
            tenant.selected = true;
            for subscription in &mut tenant.subscriptions {
                subscription.selected = true;
            }
        }
    }
}

async fn discover_subscriptions_and_accounts(
    client: &reqwest::Client,
    access_token: &str,
    tenant: &str,
) -> Result<Vec<DiscoveredSubscription>, String> {
    let url = format!(
        "https://management.azure.com/subscriptions?api-version={ARM_SUBSCRIPTIONS_API_VERSION}"
    );
    let arm_subscriptions = arm_get_paged::<ArmSubscriptionItem>(client, access_token, url).await?;

    let mut discovered = Vec::new();
    for subscription in arm_subscriptions {
        let subscription_id = subscription.subscription_id.trim().to_string();
        if subscription_id.is_empty() {
            continue;
        }

        let mut accounts =
            discover_storage_accounts(client, access_token, &subscription_id).await?;
        accounts.sort_by(|a, b| a.name.cmp(&b.name));

        discovered.push(DiscoveredSubscription {
            id: subscription_id,
            name: normalized_display_name(&subscription.display_name, "Unnamed subscription"),
            tenant_id: subscription
                .tenant_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| tenant.to_string()),
            selected: true,
            storage_accounts: accounts,
        });
    }

    Ok(discovered)
}

async fn discover_storage_accounts(
    client: &reqwest::Client,
    access_token: &str,
    subscription_id: &str,
) -> Result<Vec<DiscoveredStorageAccount>, String> {
    let url = format!(
        "https://management.azure.com/subscriptions/{subscription_id}/providers/Microsoft.Storage/storageAccounts?api-version={ARM_STORAGE_ACCOUNTS_API_VERSION}"
    );
    let accounts = arm_get_paged::<ArmStorageAccountItem>(client, access_token, url).await?;

    let mut discovered = Vec::new();
    for account in accounts {
        let endpoint = account
            .properties
            .as_ref()
            .and_then(|properties| properties.primary_endpoints.as_ref())
            .and_then(|endpoints| endpoints.blob.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("https://{}.blob.core.windows.net", account.name.trim()));

        let sku_name = account
            .sku
            .as_ref()
            .and_then(|sku| sku.name.as_deref())
            .map(str::to_string);
        let sku_tier = account
            .sku
            .as_ref()
            .and_then(|sku| sku.tier.as_deref())
            .map(str::to_string);

        discovered.push(DiscoveredStorageAccount {
            name: account.name,
            subscription_id: subscription_id.to_string(),
            kind: account.kind.unwrap_or_else(|| "StorageV2".into()),
            region: account.location.unwrap_or_else(|| "unknown".into()),
            replication: replication_from_sku(sku_name.as_deref()),
            tier: tier_from_sku(sku_tier.as_deref(), sku_name.as_deref()),
            hns: account
                .properties
                .as_ref()
                .and_then(|properties| properties.is_hns_enabled)
                .unwrap_or(false),
            endpoint,
            resource_id: account.id,
        });
    }

    Ok(discovered)
}

async fn arm_get_paged<T: DeserializeOwned>(
    client: &reqwest::Client,
    access_token: &str,
    initial_url: String,
) -> Result<Vec<T>, String> {
    let mut next_url = Some(initial_url);
    let mut items = Vec::new();

    while let Some(url) = next_url.take() {
        let response = client
            .get(&url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|error| format!("ARM request failed: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("ARM request returned {status}: {body}"));
        }

        let page = response
            .json::<ArmListResponse<T>>()
            .await
            .map_err(|error| format!("failed to parse ARM response: {error}"))?;
        items.extend(page.value);
        next_url = page.next_link;
    }

    Ok(items)
}

async fn exchange_authorization_code(
    client: &reqwest::Client,
    tenant: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
    scope: &str,
) -> Result<TokenResponse, String> {
    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "authorization_code"),
        ("client_id", DEFAULT_CLIENT_ID),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
        ("scope", scope),
    ];

    let response = client
        .post(&url)
        .form(&params)
        .send()
        .await
        .map_err(|error| format!("authorization code exchange failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("token endpoint returned {status}: {body}"));
    }

    response
        .json::<TokenResponse>()
        .await
        .map_err(|error| format!("failed to parse authorization token response: {error}"))
}

fn wait_for_authorization_code(
    listener: TcpListener,
    expected_state: &str,
    timeout: StdDuration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => return read_authorization_code(&mut stream, expected_state),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("interactive browser sign-in timed out".into());
                }
                std::thread::sleep(StdDuration::from_millis(200));
            }
            Err(error) => {
                return Err(format!("failed waiting for OAuth callback: {error}"));
            }
        }
    }
}

fn read_authorization_code<S: Read + Write>(
    stream: &mut S,
    expected_state: &str,
) -> Result<String, String> {
    let mut buffer = [0u8; 8192];
    let read = stream
        .read(&mut buffer)
        .map_err(|error| format!("failed reading OAuth callback: {error}"))?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| "OAuth callback request was malformed".to_string())?;
    let callback_url = url::Url::parse(&format!("http://127.0.0.1{path}"))
        .map_err(|error| format!("failed to parse OAuth callback: {error}"))?;
    let query: HashMap<_, _> = callback_url.query_pairs().into_owned().collect();

    if let Some(error) = query.get("error") {
        let description = query
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| "interactive sign-in failed".into());
        write_oauth_callback_response(
            stream,
            false,
            "Azure sign-in failed",
            "You can close this tab and return to Arkived.",
        );
        return Err(format!("{error}: {description}"));
    }

    let returned_state = query
        .get("state")
        .ok_or_else(|| "OAuth callback did not include state".to_string())?;
    if returned_state != expected_state {
        write_oauth_callback_response(
            stream,
            false,
            "Azure sign-in failed",
            "The OAuth callback state did not match. You can close this tab and try again.",
        );
        return Err("OAuth callback state mismatch".into());
    }

    let code = query
        .get("code")
        .cloned()
        .ok_or_else(|| "OAuth callback did not include an authorization code".to_string())?;

    write_oauth_callback_response(
        stream,
        true,
        "Azure sign-in complete",
        "Arkived received the login response. You can close this tab and return to the app.",
    );
    Ok(code)
}

async fn refresh_sign_in_tenant(
    inner: &Arc<Mutex<InnerState>>,
    store: &Store,
    credential_store: &dyn CredentialStore,
    snapshot_path: &Path,
    sign_in_id: &str,
    tenant_id: &str,
    arm_bundle: TokenBundle,
) -> Result<(), String> {
    if arm_bundle.refresh_token.is_none() {
        return Err(
            "Azure did not return a refresh token for this tenant sign-in. Sign in again to continue."
                .into(),
        );
    }

    let refresh_context = arm_bundle
        .refresh_context
        .clone()
        .ok_or_else(|| "tenant sign-in did not include refresh context".to_string())?;
    let subscriptions_result = discover_subscriptions_and_accounts(
        &refresh_context.client,
        &arm_bundle.access_token,
        tenant_id,
    )
    .await;

    let mut guard = inner.lock().unwrap();
    let sign_in = guard
        .sign_ins
        .get_mut(sign_in_id)
        .ok_or_else(|| format!("unknown sign-in id `{sign_in_id}`"))?;
    let tenant = sign_in
        .tenants
        .iter_mut()
        .find(|tenant| tenant.id == tenant_id)
        .ok_or_else(|| format!("unknown tenant `{tenant_id}` for sign-in `{sign_in_id}`"))?;

    let outcome = match subscriptions_result {
        Ok(mut subscriptions) => {
            subscriptions.sort_by(|a, b| a.name.cmp(&b.name));
            tenant.selected = !subscriptions.is_empty();
            tenant.needs_reauth = false;
            tenant.error = None;
            tenant.subscriptions = subscriptions;
            sign_in
                .tenant_bundles
                .insert(tenant_id.to_string(), arm_bundle);
            Ok(())
        }
        Err(error) => {
            let message = compact_tenant_error(&error);
            tenant.selected = false;
            tenant.needs_reauth = requires_tenant_reauth(&error);
            tenant.error = Some(message.clone());
            tenant.subscriptions.clear();
            Err(message)
        }
    };
    let sign_in_snapshot = sign_in.clone();
    drop(guard);

    if let Err(error) =
        persist_sign_in_session_snapshot(store, credential_store, snapshot_path, &sign_in_snapshot)
    {
        eprintln!("failed to refresh persisted sign-in `{sign_in_id}`: {error}");
    }

    outcome
}

fn write_oauth_callback_response(stream: &mut impl Write, ok: bool, title: &str, message: &str) {
    let status = if ok { "200 OK" } else { "400 Bad Request" };
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>body{{font-family:Segoe UI,Arial,sans-serif;background:#0f1115;color:#f3f3f5;padding:48px}}main{{max-width:640px;margin:0 auto;border:1px solid #2a2a33;border-radius:16px;padding:24px;background:#15151a}}h1{{margin:0 0 12px;font-size:28px}}p{{margin:0;color:#c6c6cd;line-height:1.6}}</style></head><body><main><h1>{title}</h1><p>{message}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

async fn mint_scoped_bundle(
    source_bundle: &TokenBundle,
    tenant: &str,
    scope: &str,
) -> Result<TokenBundle, String> {
    let refresh_context = source_bundle
        .refresh_context
        .clone()
        .ok_or_else(|| "refresh context is unavailable for this sign-in".to_string())?;
    let refresh_token = source_bundle
        .refresh_token
        .clone()
        .ok_or_else(|| {
            "Azure did not return a refresh token for this sign-in. Use browser OAuth sign-in or sign in again."
                .to_string()
        })?;

    let refreshed = refresh_access_token(
        &refresh_context.client,
        tenant,
        &refresh_context.client_id,
        &refresh_token,
        scope,
    )
    .await
    .map_err(error_to_string)?;

    Ok(TokenBundle {
        access_token: refreshed.access_token,
        refresh_token: refreshed.refresh_token.or(Some(refresh_token)),
        expires_at: OffsetDateTime::now_utc() + TimeDuration::seconds(refreshed.expires_in as i64),
        refresh_context: Some(RefreshContext {
            client: refresh_context.client.clone(),
            tenant: tenant.to_string(),
            client_id: refresh_context.client_id.clone(),
            scope: scope.to_string(),
        }),
    })
}

async fn mint_sign_in_scoped_bundle(
    sign_in: &SignInSession,
    tenant_id: &str,
    scope: &str,
) -> Result<TokenBundle, String> {
    if let Some(bundle) = sign_in.tenant_bundles.get(tenant_id) {
        if let Ok(scoped) = mint_scoped_bundle(bundle, tenant_id, scope).await {
            return Ok(scoped);
        }
    }

    mint_scoped_bundle(&sign_in.arm_bundle, tenant_id, scope).await
}

fn preferred_account_label(token: &TokenResponse) -> Option<String> {
    let claims = parse_id_token_claims(token.id_token.as_deref()?)?;
    [
        claims.preferred_username,
        claims.email,
        claims.upn,
        claims.name,
    ]
    .into_iter()
    .flatten()
    .map(|value| value.trim().to_string())
    .find(|value| !value.is_empty())
}

fn parse_id_token_claims(id_token: &str) -> Option<IdTokenClaims> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<IdTokenClaims>(&bytes).ok()
}

fn compact_tenant_error(error: &str) -> String {
    if error.contains("AADSTS50076") {
        return "Reauthentication required by this tenant's MFA or Conditional Access policy. Authenticate this tenant again to load its subscriptions.".into();
    }
    if error.contains("AADSTS50079") {
        return "This tenant requires additional security registration before subscriptions can be loaded. Authenticate this tenant again to continue.".into();
    }
    if error.contains("\"invalid_grant\"") || error.contains("invalid_grant") {
        return "This tenant requires a fresh sign-in before Arkived can load its subscriptions. Authenticate this tenant again to continue.".into();
    }
    if error.contains("ARM request returned 403") || error.contains("ARM request returned 401") {
        return "This account is signed in, but it does not currently have permission to enumerate subscriptions in this tenant.".into();
    }
    error.to_string()
}

fn compact_discovered_account_error(
    account_name: &str,
    error: &str,
    fallback_note: Option<&str>,
) -> String {
    if error.contains("AuthorizationPermissionMismatch")
        || error.contains("AuthorizationFailure")
        || error.contains("AuthenticationFailed")
    {
        if let Some(fallback_note) = fallback_note {
            return format!(
                "Could not open `{account_name}`. Blob browsing via Azure AD was denied, and Arkived could not switch this account onto a managed-key path: {fallback_note}"
            );
        }

        return format!(
            "Could not open `{account_name}`. Arkived discovered the account through Azure Resource Manager, but the signed-in identity does not currently have blob data permission for this storage account. Grant a Storage Blob Data role, or allow ARM key access so Arkived can fall back to shared-key browsing."
        );
    }

    if error.contains("AuthorizationPermissionDenied") {
        return format!(
            "Could not open `{account_name}` because this identity is not allowed to browse blob data for that storage account."
        );
    }

    error.to_string()
}

fn compact_live_browse_error(
    connection: &LiveConnection,
    operation: &str,
    resource: Option<&str>,
    error: &str,
) -> String {
    if let LiveConnection::Entra {
        account_name,
        fallback_note,
        ..
    } = connection
    {
        if is_storage_auth_error(error) {
            let subject = resource
                .map(|name| format!("`{name}` in `{account_name}`"))
                .unwrap_or_else(|| format!("`{account_name}`"));
            if let Some(fallback_note) = fallback_note.as_deref() {
                return format!(
                    "{operation} via Azure AD was denied for {subject}. Arkived stayed on Azure AD because managed-key fallback was unavailable: {fallback_note}"
                );
            }

            return format!(
                "{operation} via Azure AD was denied for {subject}. Arkived can see this storage account through Azure Resource Manager, but the blob data request itself was rejected."
            );
        }
    }

    if let LiveConnection::AccountKey { account_name, .. } = connection {
        if error.contains("KeyBasedAuthenticationNotPermitted") {
            return format!(
                "{operation} for `{account_name}` was rejected because this storage account disables Shared Key access."
            );
        }
    }

    error.to_string()
}

fn compact_key_fallback_note(errors: &[String]) -> String {
    if errors.is_empty() {
        return "Arkived could not retrieve a usable storage account key through Azure Resource Manager.".into();
    }

    let last = errors.last().expect("checked above");
    format!("Azure Resource Manager key fallback failed: {last}")
}

fn compact_arm_token_error(error: &str) -> String {
    if error.contains("AADSTS50076") {
        return "Azure required fresh MFA before it would mint a tenant-scoped Azure Resource Manager token.".into();
    }
    if error.contains("AADSTS50079") {
        return "Azure required additional security registration before it would mint a tenant-scoped Azure Resource Manager token.".into();
    }
    if error.contains("\"invalid_grant\"") || error.contains("invalid_grant") {
        return "Azure required a fresh sign-in before it would mint a tenant-scoped Azure Resource Manager token.".into();
    }

    error.to_string()
}

async fn request_storage_account_key(
    bundle: &TokenBundle,
    resource_id: &str,
) -> Result<String, String> {
    let refresh_context = bundle.refresh_context.as_ref().ok_or_else(|| {
        "this sign-in no longer has HTTP context for Azure Resource Manager key requests"
            .to_string()
    })?;
    let url = format!(
        "https://management.azure.com{resource_id}/listKeys?api-version={ARM_STORAGE_ACCOUNTS_API_VERSION}"
    );
    let response = refresh_context
        .client
        .post(&url)
        .bearer_auth(&bundle.access_token)
        // ARM `listKeys` is a POST action; send an explicit empty JSON body so
        // Azure does not reject the request with HTTP 411 on stricter edges.
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|error| format!("ARM `listKeys` request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("ARM `listKeys` response could not be read: {error}"))?;

    if !status.is_success() {
        return Err(compact_arm_list_keys_error(status.as_u16(), &body));
    }

    let keys = serde_json::from_str::<ArmListKeysResponse>(&body)
        .map_err(|error| format!("ARM `listKeys` response could not be parsed: {error}"))?;
    keys.keys
        .into_iter()
        .find_map(|key| {
            let value = key.value?.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .ok_or_else(|| {
            "ARM `listKeys` succeeded, but Azure did not return a usable storage account key."
                .to_string()
        })
}

fn compact_arm_list_keys_error(status: u16, body: &str) -> String {
    if body.contains("AuthorizationFailed") || body.contains("AuthorizationPermissionDenied") {
        return "Azure Resource Manager denied the `listKeys` action for this storage account."
            .into();
    }
    if body.contains("KeyBasedAuthenticationNotPermitted") {
        return "this storage account disables Shared Key access.".into();
    }
    if status == 401 {
        return "Azure Resource Manager rejected Arkived's tenant token while requesting storage account keys.".into();
    }
    if status == 403 {
        return "Azure Resource Manager returned 403 while requesting storage account keys.".into();
    }
    if status == 404 {
        return "Azure Resource Manager could not find this storage account while requesting account keys.".into();
    }
    if status == 411 {
        return "Azure Resource Manager rejected the key request because the POST body/content length was missing.".into();
    }

    format!("Azure Resource Manager returned HTTP {status} while requesting storage account keys.")
}

fn is_storage_auth_error(error: &str) -> bool {
    error.contains("AuthorizationPermissionMismatch")
        || error.contains("AuthorizationPermissionDenied")
        || error.contains("AuthorizationFailure")
        || error.contains("AuthenticationFailed")
}

fn requires_tenant_reauth(error: &str) -> bool {
    error.contains("AADSTS50076")
        || error.contains("AADSTS50079")
        || error.contains("\"invalid_grant\"")
        || error.contains("invalid_grant")
        || error.contains("interaction_required")
        || error.contains("AADSTS65001")
}

fn token_bundle_from_response(
    response: TokenResponse,
    client: reqwest::Client,
    tenant: String,
    scope: &str,
) -> TokenBundle {
    TokenBundle {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: OffsetDateTime::now_utc() + TimeDuration::seconds(response.expires_in as i64),
        refresh_context: Some(RefreshContext {
            client,
            tenant,
            client_id: DEFAULT_CLIENT_ID.into(),
            scope: scope.into(),
        }),
    }
}

fn scope_with_refresh(scope: &str) -> String {
    format!("{scope} offline_access openid profile")
}

fn pkce_code_verifier() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn pkce_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn build_authorize_url(
    tenant: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(&format!(
        "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"
    ))
    .map_err(|error| format!("failed to construct authorize URL: {error}"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("client_id", DEFAULT_CLIENT_ID);
        query.append_pair("response_type", "code");
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("response_mode", "query");
        query.append_pair("scope", scope);
        query.append_pair("state", state);
        query.append_pair("code_challenge", code_challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("prompt", "select_account");
    }
    Ok(url.to_string())
}

fn device_code_prompt(login_id: String, response: &DeviceCodeResponse) -> DeviceCodePrompt {
    DeviceCodePrompt {
        login_id,
        verification_uri: response.verification_uri.clone(),
        user_code: response.user_code.clone(),
        message: response.message.clone(),
        expires_in_seconds: response.expires_in,
        interval_seconds: response.interval,
    }
}

fn connection_summary(connection: LiveConnection) -> BrowserConnection {
    match connection {
        LiveConnection::ConnectionString {
            id,
            display_name,
            endpoint,
            fixed_container,
            ..
        } => BrowserConnection {
            id,
            display_name,
            account_name: account_name_from_endpoint(&endpoint),
            endpoint,
            auth_kind: "connection-string".into(),
            fixed_container,
            origin_sign_in_id: None,
            origin_subscription_id: None,
        },
        LiveConnection::AccountKey {
            id,
            display_name,
            endpoint,
            account_name,
            auth_kind,
            origin,
            ..
        } => BrowserConnection {
            id,
            display_name,
            account_name,
            endpoint,
            auth_kind,
            fixed_container: None,
            origin_sign_in_id: origin.as_ref().map(|value| value.sign_in_id.clone()),
            origin_subscription_id: origin.as_ref().map(|value| value.subscription_id.clone()),
        },
        LiveConnection::Sas {
            id,
            display_name,
            endpoint,
            fixed_container,
            ..
        } => BrowserConnection {
            id,
            display_name,
            account_name: account_name_from_endpoint(&endpoint),
            endpoint,
            auth_kind: "sas".into(),
            fixed_container,
            origin_sign_in_id: None,
            origin_subscription_id: None,
        },
        LiveConnection::Azurite { id, display_name } => BrowserConnection {
            id,
            display_name,
            account_name: arkived_core::auth::azurite::AZURITE_ACCOUNT.into(),
            endpoint: arkived_core::auth::azurite::AZURITE_BLOB_ENDPOINT.into(),
            auth_kind: "azurite".into(),
            fixed_container: None,
            origin_sign_in_id: None,
            origin_subscription_id: None,
        },
        LiveConnection::Entra {
            id,
            display_name,
            endpoint,
            account_name,
            tenant,
            auth_kind,
            origin,
            ..
        } => BrowserConnection {
            id,
            display_name: format!("{display_name} ({tenant})"),
            account_name,
            endpoint,
            auth_kind,
            fixed_container: None,
            origin_sign_in_id: origin.as_ref().map(|value| value.sign_in_id.clone()),
            origin_subscription_id: origin.as_ref().map(|value| value.subscription_id.clone()),
        },
    }
}

fn sign_in_summary(sign_in: SignInSession) -> BrowserSignIn {
    let tenant_count = sign_in.tenants.len();
    let selected_tenant_count = sign_in
        .tenants
        .iter()
        .filter(|tenant| tenant.selected)
        .count();
    let subscription_count = sign_in
        .tenants
        .iter()
        .map(|tenant| tenant.subscriptions.len())
        .sum();
    let selected_subscription_count = sign_in
        .tenants
        .iter()
        .filter(|tenant| tenant.selected)
        .map(|tenant| {
            tenant
                .subscriptions
                .iter()
                .filter(|subscription| subscription.selected)
                .count()
        })
        .sum();

    BrowserSignIn {
        id: sign_in.id,
        display_name: sign_in.display_name,
        tenant: sign_in.login_tenant,
        environment: sign_in.environment,
        subscription_count,
        selected_subscription_count,
        tenant_count,
        selected_tenant_count,
    }
}

fn tenant_summary(sign_in_id: &str, tenant: DiscoveredTenant) -> BrowserTenant {
    let tenant_label = tenant_label(&tenant);
    let subscription_count = tenant.subscriptions.len();
    let selected_subscription_count = tenant
        .subscriptions
        .iter()
        .filter(|subscription| subscription.selected)
        .count();
    let storage_account_count = tenant
        .subscriptions
        .iter()
        .map(|subscription| subscription.storage_accounts.len())
        .sum();
    let subscriptions = tenant
        .subscriptions
        .into_iter()
        .map(|subscription| subscription_summary(sign_in_id, tenant_label.as_str(), subscription))
        .collect();

    BrowserTenant {
        id: tenant.id,
        sign_in_id: sign_in_id.to_string(),
        display_name: tenant.display_name,
        default_domain: tenant.default_domain,
        selected: tenant.selected,
        needs_reauth: tenant.needs_reauth,
        error: tenant.error,
        subscription_count,
        selected_subscription_count,
        storage_account_count,
        subscriptions,
    }
}

fn subscription_summary(
    sign_in_id: &str,
    tenant_label: &str,
    subscription: DiscoveredSubscription,
) -> BrowserSubscription {
    BrowserSubscription {
        id: subscription.id,
        sign_in_id: sign_in_id.to_string(),
        name: subscription.name,
        tenant_id: subscription.tenant_id,
        tenant_label: tenant_label.to_string(),
        storage_account_count: subscription.storage_accounts.len(),
        selected: subscription.selected,
    }
}

fn tenant_label(tenant: &DiscoveredTenant) -> String {
    tenant
        .default_domain
        .clone()
        .unwrap_or_else(|| tenant.display_name.clone())
}

fn discovered_account_summary(
    sign_in_id: &str,
    account: DiscoveredStorageAccount,
) -> BrowserStorageAccount {
    BrowserStorageAccount {
        sign_in_id: sign_in_id.to_string(),
        subscription_id: account.subscription_id,
        name: account.name,
        kind: account.kind,
        region: account.region,
        replication: account.replication,
        tier: account.tier,
        hns: account.hns,
        endpoint: account.endpoint,
    }
}

fn blob_entry_to_row(entry: BlobEntry, current_prefix: Option<&str>) -> BrowserBlobRow {
    match entry {
        BlobEntry::Prefix { name } => BrowserBlobRow {
            path: name.clone(),
            name: leaf_name(&name, current_prefix),
            kind: "dir".into(),
            size: None,
            tier: None,
            modified: String::new(),
            etag: None,
            lease: None,
            icon: "folder".into(),
        },
        BlobEntry::Blob {
            name,
            size,
            tier,
            etag,
            last_modified,
            lease_state,
            ..
        } => BrowserBlobRow {
            path: name.clone(),
            name: leaf_name(&name, current_prefix),
            kind: "blob".into(),
            size: Some(format_bytes(size)),
            tier,
            modified: last_modified.map(|ts| ts.to_string()).unwrap_or_default(),
            etag,
            lease: lease_state,
            icon: icon_for_name(&name).into(),
        },
    }
}

fn icon_for_name(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".parquet") {
        "parquet"
    } else if lower.ends_with(".json") {
        "json"
    } else if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
    {
        "image"
    } else if lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".zst")
        || lower.ends_with(".7z")
    {
        "archive"
    } else {
        "file"
    }
}

fn leaf_name(path: &str, current_prefix: Option<&str>) -> String {
    let trimmed = path.trim_end_matches('/');
    let without_prefix = current_prefix
        .and_then(|prefix| trimmed.strip_prefix(prefix))
        .unwrap_or(trimmed)
        .trim_matches('/');
    without_prefix
        .rsplit('/')
        .next()
        .unwrap_or(without_prefix)
        .to_string()
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn normalized_display_name(display_name: &str, fallback: &str) -> String {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalized_tenant(tenant: Option<String>) -> String {
    tenant
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("common")
        .to_string()
}

fn parse_endpoint(raw: &str) -> Result<url::Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("blob endpoint is required".into());
    }
    url::Url::parse(trimmed).map_err(|error| format!("invalid blob endpoint `{trimmed}`: {error}"))
}

fn normalize_prefix(prefix: Option<String>) -> Option<String> {
    prefix.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.ends_with('/') {
            Some(trimmed.to_string())
        } else {
            Some(format!("{trimmed}/"))
        }
    })
}

fn account_name_from_endpoint(endpoint: &str) -> String {
    parse_endpoint(endpoint)
        .ok()
        .and_then(|url| {
            url.host_str()
                .and_then(|host| host.split('.').next())
                .map(|value| value.to_string())
        })
        .unwrap_or_else(|| endpoint.to_string())
}

fn replication_from_sku(sku_name: Option<&str>) -> String {
    sku_name
        .and_then(|value| value.rsplit('_').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown")
        .to_string()
}

fn tier_from_sku(sku_tier: Option<&str>, sku_name: Option<&str>) -> String {
    if let Some(tier) = sku_tier.filter(|value| !value.is_empty()) {
        return tier.to_string();
    }

    sku_name
        .and_then(|value| value.split('_').next())
        .filter(|value| !value.is_empty())
        .unwrap_or("Standard")
        .to_string()
}

fn error_to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}
