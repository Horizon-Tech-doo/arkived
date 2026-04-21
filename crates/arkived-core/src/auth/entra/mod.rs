//! Microsoft Entra ID auth (OAuth 2.0 device-code flow, hand-rolled).
//!
//! `azure_identity 0.34` no longer exposes a user-facing `DeviceCodeCredential`,
//! so we implement the four-endpoint flow directly against
//! `login.microsoftonline.com`.

pub mod device_code;
pub mod credential;
pub mod cache;

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

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::auth::credentials::CredentialStore;
use crate::auth::entra::cache::{CachedRefresh, RefreshCache};
use crate::auth::entra::credential::{EntraTokenCredential, RefreshContext, TokenBundle};
use crate::auth::entra::device_code::{
    poll_for_token, refresh_access_token, start_device_code,
};
use crate::progress::{ProgressEvent, ProgressSink};
use crate::types::{AuthKind, ResourceKind};
use crate::Error;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

/// `AuthProvider` for Microsoft Entra ID via OAuth 2.0 device-code flow.
pub struct EntraDeviceCodeProvider {
    display_name: String,
    tenant: String,
    client_id: String,
    scope: String,
    sign_in_id: String,
    store: Arc<dyn CredentialStore>,
    progress: Arc<dyn ProgressSink>,
    http: reqwest::Client,
}

impl std::fmt::Debug for EntraDeviceCodeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntraDeviceCodeProvider")
            .field("display_name", &self.display_name)
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field("sign_in_id", &self.sign_in_id)
            .finish()
    }
}

impl EntraDeviceCodeProvider {
    /// Build with defaults (`DEFAULT_CLIENT_ID`, `STORAGE_SCOPE`).
    pub fn new(
        display_name: impl Into<String>,
        tenant: impl Into<String>,
        sign_in_id: impl Into<String>,
        store: Arc<dyn CredentialStore>,
        progress: Arc<dyn ProgressSink>,
    ) -> Self {
        Self {
            display_name: display_name.into(),
            tenant: tenant.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            scope: STORAGE_SCOPE.into(),
            sign_in_id: sign_in_id.into(),
            store,
            progress,
            http: reqwest::Client::new(),
        }
    }

    /// Override the default client ID (typically not needed).
    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    /// Override the default scope (rarely needed).
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    async fn try_cached_refresh(&self) -> Result<Option<TokenBundle>, Error> {
        let cache = RefreshCache::new(&*self.store);
        let Some(cached) = cache.get(&self.sign_in_id)? else {
            return Ok(None);
        };
        let response = refresh_access_token(
            &self.http,
            &cached.tenant,
            &cached.client_id,
            &cached.refresh_token,
            &cached.scope,
        )
        .await?;
        let expires_at =
            OffsetDateTime::now_utc() + time::Duration::seconds(response.expires_in as i64);
        let bundle = TokenBundle {
            access_token: response.access_token,
            refresh_token: response.refresh_token.or(Some(cached.refresh_token.clone())),
            expires_at,
            refresh_context: Some(RefreshContext {
                client: self.http.clone(),
                tenant: cached.tenant.clone(),
                client_id: cached.client_id.clone(),
                scope: cached.scope.clone(),
            }),
        };
        // Update cache with new refresh token if it changed.
        if let Some(new_rt) = &bundle.refresh_token {
            cache.put(
                &self.sign_in_id,
                &CachedRefresh {
                    refresh_token: new_rt.clone(),
                    tenant: cached.tenant,
                    client_id: cached.client_id,
                    scope: cached.scope,
                    obtained_at: OffsetDateTime::now_utc(),
                },
            )?;
        }
        Ok(Some(bundle))
    }

    async fn run_device_code_flow(&self) -> Result<TokenBundle, Error> {
        let dc =
            start_device_code(&self.http, &self.tenant, &self.client_id, &self.scope).await?;

        // User-visible instructions: route through `tracing` so the CLI can
        // display via tracing-subscriber to stderr. The Backend/CLI plan will
        // ensure a tracing-subscriber is installed that surfaces info-level
        // records during device-code flows.
        tracing::info!(
            verification_uri = %dc.verification_uri,
            user_code = %dc.user_code,
            expires_in_seconds = dc.expires_in,
            "Entra sign-in required"
        );
        // Keep a progress heartbeat so any attached sink sees activity.
        self.progress.emit(ProgressEvent::Start { total: None }).await;

        let response = poll_for_token(
            &self.http,
            &self.tenant,
            &self.client_id,
            &dc.device_code,
            Duration::from_secs(dc.interval),
            Duration::from_secs(dc.expires_in),
        )
        .await?;

        let expires_at =
            OffsetDateTime::now_utc() + time::Duration::seconds(response.expires_in as i64);
        let refresh_token = response.refresh_token.clone();

        if let Some(rt) = &refresh_token {
            let cache = RefreshCache::new(&*self.store);
            cache.put(
                &self.sign_in_id,
                &CachedRefresh {
                    refresh_token: rt.clone(),
                    tenant: self.tenant.clone(),
                    client_id: self.client_id.clone(),
                    scope: self.scope.clone(),
                    obtained_at: OffsetDateTime::now_utc(),
                },
            )?;
        }

        Ok(TokenBundle {
            access_token: response.access_token,
            refresh_token,
            expires_at,
            refresh_context: Some(RefreshContext {
                client: self.http.clone(),
                tenant: self.tenant.clone(),
                client_id: self.client_id.clone(),
                scope: self.scope.clone(),
            }),
        })
    }
}

#[async_trait]
impl AuthProvider for EntraDeviceCodeProvider {
    fn kind(&self) -> AuthKind { AuthKind::EntraDeviceCode }
    fn display_name(&self) -> &str { &self.display_name }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        let bundle = match self.try_cached_refresh().await? {
            Some(b) => b,
            None => self.run_device_code_flow().await?,
        };
        let cred = Arc::new(EntraTokenCredential::new(bundle));
        Ok(ResolvedCredential::Entra(cred))
    }
    fn supports(&self, resource: ResourceKind) -> bool {
        !matches!(resource, ResourceKind::Queue | ResourceKind::Table)
            // Entra auth for Queues/Tables requires different scope; out of v0.1.0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::credentials::CredentialStore;
    use crate::progress::NoopSink;
    use secrecy::{ExposeSecret, SecretString};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeStore(Mutex<HashMap<String, String>>);
    impl FakeStore { fn new() -> Self { Self(Mutex::new(HashMap::new())) } }
    impl CredentialStore for FakeStore {
        fn put(&self, key: &str, secret: &SecretString) -> Result<(), Error> {
            self.0.lock().unwrap().insert(key.into(), secret.expose_secret().into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<SecretString, Error> {
            self.0.lock().unwrap().get(key)
                .map(|s| SecretString::new(s.clone().into()))
                .ok_or_else(|| Error::NotFound { resource: key.into() })
        }
        fn delete(&self, key: &str) -> Result<(), Error> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[test]
    fn builds_with_defaults() {
        let store: Arc<dyn CredentialStore> = Arc::new(FakeStore::new());
        let sink: Arc<dyn ProgressSink> = Arc::new(NoopSink);
        let p = EntraDeviceCodeProvider::new(
            "hamza@horizon-tech.io",
            "common",
            "si-1",
            store,
            sink,
        );
        assert_eq!(p.kind(), AuthKind::EntraDeviceCode);
        assert_eq!(p.display_name(), "hamza@horizon-tech.io");
        assert_eq!(p.client_id, DEFAULT_CLIENT_ID);
        assert_eq!(p.scope, STORAGE_SCOPE);
    }

    #[test]
    fn supports_blob_and_adls_rejects_queue_table() {
        let store: Arc<dyn CredentialStore> = Arc::new(FakeStore::new());
        let sink: Arc<dyn ProgressSink> = Arc::new(NoopSink);
        let p = EntraDeviceCodeProvider::new("x", "common", "si-1", store, sink);
        assert!(p.supports(ResourceKind::BlobContainer));
        assert!(p.supports(ResourceKind::AdlsContainer));
        assert!(!p.supports(ResourceKind::Queue));
        assert!(!p.supports(ResourceKind::Table));
    }
}
