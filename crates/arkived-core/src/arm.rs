//! A minimal Azure Resource Manager (ARM) client for account discovery.
//!
//! Given an ARM-scoped access token it enumerates subscriptions and their
//! storage accounts, and can fetch an account's access key. Only the handful of
//! read endpoints needed to populate the local catalog are implemented; the
//! base URL is injectable so the HTTP/JSON handling is unit-testable.

use crate::auth::credentials::CredentialStore;
use crate::auth::entra::cache::{CachedRefresh, RefreshCache};
use crate::auth::entra::device_code::refresh_access_token;
use crate::Error;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use time::OffsetDateTime;

/// Default ARM endpoint (Azure public cloud).
pub const ARM_BASE: &str = "https://management.azure.com";
/// Scope to request an ARM access token for.
pub const ARM_SCOPE: &str = "https://management.azure.com/.default";

const SUBSCRIPTIONS_API: &str = "2020-01-01";
const STORAGE_API: &str = "2023-01-01";

/// A subscription returned by discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSubscription {
    /// Subscription GUID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Owning tenant id, if reported.
    pub tenant_id: Option<String>,
}

/// A storage account returned by discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredAccount {
    /// Account name (Azure-unique).
    pub name: String,
    /// Full ARM resource id (used for `list_keys`).
    pub resource_id: String,
    /// Azure region.
    pub location: String,
    /// SKU kind (e.g. `StorageV2`).
    pub kind: String,
    /// Replication / SKU name (e.g. `Standard_LRS`).
    pub sku_name: String,
    /// Performance tier (`Standard` / `Premium`).
    pub sku_tier: String,
    /// Whether hierarchical namespace (ADLS Gen2) is enabled.
    pub hns: bool,
    /// Primary blob endpoint, if reported.
    pub blob_endpoint: Option<String>,
}

// ---- Wire models (ARM camelCase JSON) ---------------------------------------

#[derive(Deserialize)]
struct Paged<T> {
    #[serde(default = "Vec::new")]
    value: Vec<T>,
    #[serde(rename = "nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct RawSubscription {
    #[serde(rename = "subscriptionId")]
    subscription_id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "tenantId")]
    tenant_id: Option<String>,
}

#[derive(Deserialize)]
struct RawStorageAccount {
    id: String,
    name: String,
    #[serde(default)]
    location: String,
    kind: Option<String>,
    sku: Option<RawSku>,
    properties: Option<RawStorageProps>,
}

#[derive(Deserialize)]
struct RawSku {
    name: Option<String>,
    tier: Option<String>,
}

#[derive(Deserialize)]
struct RawStorageProps {
    #[serde(rename = "isHnsEnabled")]
    is_hns_enabled: Option<bool>,
    #[serde(rename = "primaryEndpoints")]
    primary_endpoints: Option<RawEndpoints>,
}

#[derive(Deserialize)]
struct RawEndpoints {
    blob: Option<String>,
}

#[derive(Deserialize)]
struct ListKeysResponse {
    #[serde(default = "Vec::new")]
    keys: Vec<RawKey>,
}

#[derive(Deserialize)]
struct RawKey {
    value: String,
}

/// A read-only ARM client bound to a single access token.
pub struct ArmClient {
    http: reqwest::Client,
    token: String,
    base: String,
}

impl ArmClient {
    /// Build a client for the Azure public cloud.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            token: token.into(),
            base: ARM_BASE.to_string(),
        }
    }

    /// Build a client against a custom base URL (sovereign clouds, tests).
    pub fn with_base(token: impl Into<String>, base: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            token: token.into(),
            base: base.into(),
        }
    }

    /// GET a single ARM resource as JSON.
    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| Error::NetworkTransient(format!("arm get: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Backend(format!("ARM {status}: {body}")));
        }
        resp.json::<T>()
            .await
            .map_err(|e| Error::Backend(format!("arm decode: {e}")))
    }

    /// Collect all pages of a paged ARM list, following `nextLink`.
    async fn get_paged<T: DeserializeOwned>(&self, first_url: String) -> Result<Vec<T>, Error> {
        let mut out = Vec::new();
        let mut next = Some(first_url);
        while let Some(url) = next {
            let page: Paged<T> = self.get_json(&url).await?;
            out.extend(page.value);
            next = page.next_link;
        }
        Ok(out)
    }

    /// List the subscriptions the token can see.
    pub async fn list_subscriptions(&self) -> Result<Vec<DiscoveredSubscription>, Error> {
        let url = format!(
            "{}/subscriptions?api-version={SUBSCRIPTIONS_API}",
            self.base
        );
        let raw: Vec<RawSubscription> = self.get_paged(url).await?;
        Ok(raw
            .into_iter()
            .map(|s| DiscoveredSubscription {
                name: s.display_name.unwrap_or_else(|| s.subscription_id.clone()),
                id: s.subscription_id,
                tenant_id: s.tenant_id,
            })
            .collect())
    }

    /// List storage accounts in a subscription.
    pub async fn list_storage_accounts(
        &self,
        subscription_id: &str,
    ) -> Result<Vec<DiscoveredAccount>, Error> {
        let url = format!(
            "{}/subscriptions/{subscription_id}/providers/Microsoft.Storage/storageAccounts?api-version={STORAGE_API}",
            self.base
        );
        let raw: Vec<RawStorageAccount> = self.get_paged(url).await?;
        Ok(raw.into_iter().map(map_account).collect())
    }

    /// Fetch an account's primary access key via `listKeys`. Returns `Ok(None)`
    /// when the caller lacks permission (a 403), so discovery can record the
    /// account as metadata-only rather than failing the whole run.
    pub async fn list_keys(&self, resource_id: &str) -> Result<Option<String>, Error> {
        let url = format!(
            "{}{resource_id}/listKeys?api-version={STORAGE_API}",
            self.base
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.token)
            .header("content-length", "0")
            .send()
            .await
            .map_err(|e| Error::NetworkTransient(format!("arm listKeys: {e}")))?;
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Backend(format!("ARM listKeys {status}: {body}")));
        }
        let parsed: ListKeysResponse = resp
            .json()
            .await
            .map_err(|e| Error::Backend(format!("arm listKeys decode: {e}")))?;
        Ok(parsed.keys.into_iter().next().map(|k| k.value))
    }
}

/// Obtain an ARM-scoped access token for a signed-in user, using the refresh
/// token cached under `sign_in_id`. The Azure CLI public client issues
/// multi-resource refresh tokens, so a storage-scoped sign-in can mint an ARM
/// token. Rotates the cached refresh token if the authority returns a new one.
pub async fn arm_token_for(
    secrets: &dyn CredentialStore,
    sign_in_id: &str,
) -> Result<String, Error> {
    let cache = RefreshCache::new(secrets);
    let cached = cache
        .get(sign_in_id)?
        .ok_or_else(|| Error::AuthFailed("not signed in; run `arkived login` first".into()))?;

    let scope = format!("{ARM_SCOPE} offline_access");
    let http = reqwest::Client::new();
    let resp = refresh_access_token(
        &http,
        &cached.tenant,
        &cached.client_id,
        &cached.refresh_token,
        &scope,
    )
    .await?;

    if let Some(new_rt) = resp.refresh_token {
        cache.put(
            sign_in_id,
            &CachedRefresh {
                refresh_token: new_rt,
                tenant: cached.tenant,
                client_id: cached.client_id,
                scope: cached.scope,
                obtained_at: OffsetDateTime::now_utc(),
            },
        )?;
    }
    Ok(resp.access_token)
}

fn map_account(a: RawStorageAccount) -> DiscoveredAccount {
    let props = a.properties;
    let hns = props
        .as_ref()
        .and_then(|p| p.is_hns_enabled)
        .unwrap_or(false);
    let blob_endpoint = props.and_then(|p| p.primary_endpoints).and_then(|e| e.blob);
    let (sku_name, sku_tier) = a
        .sku
        .map(|s| (s.name.unwrap_or_default(), s.tier.unwrap_or_default()))
        .unwrap_or_default();
    DiscoveredAccount {
        name: a.name,
        resource_id: a.id,
        location: a.location,
        kind: a.kind.unwrap_or_default(),
        sku_name,
        sku_tier,
        hns,
        blob_endpoint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lists_subscriptions_across_pages() {
        let mut server = mockito::Server::new_async().await;
        let base = server.url();
        let page2 = format!("{base}/subscriptions/page2");
        let _m1 = server
            .mock("GET", "/subscriptions")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_body(format!(
                r#"{{"value":[{{"subscriptionId":"sub-1","displayName":"Prod","tenantId":"t-1"}}],"nextLink":"{page2}"}}"#
            ))
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/subscriptions/page2")
            .with_status(200)
            .with_body(r#"{"value":[{"subscriptionId":"sub-2","displayName":"Dev"}]}"#)
            .create_async()
            .await;

        let arm = ArmClient::with_base("tok", base);
        let subs = arm.list_subscriptions().await.unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].id, "sub-1");
        assert_eq!(subs[0].name, "Prod");
        assert_eq!(subs[0].tenant_id.as_deref(), Some("t-1"));
        // Missing displayName falls back to the id.
        assert_eq!(subs[1].id, "sub-2");
        assert_eq!(subs[1].name, "Dev");
    }

    #[tokio::test]
    async fn lists_storage_accounts_with_props() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex("/storageAccounts".into()))
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_body(
                r#"{"value":[{
                  "id":"/subscriptions/sub-1/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/acme",
                  "name":"acme","location":"westeurope","kind":"StorageV2",
                  "sku":{"name":"Standard_LRS","tier":"Standard"},
                  "properties":{"isHnsEnabled":true,"primaryEndpoints":{"blob":"https://acme.blob.core.windows.net/"}}
                }]}"#,
            )
            .create_async()
            .await;

        let arm = ArmClient::with_base("tok", server.url());
        let accts = arm.list_storage_accounts("sub-1").await.unwrap();
        assert_eq!(accts.len(), 1);
        let a = &accts[0];
        assert_eq!(a.name, "acme");
        assert_eq!(a.location, "westeurope");
        assert_eq!(a.kind, "StorageV2");
        assert_eq!(a.sku_name, "Standard_LRS");
        assert!(a.hns);
        assert_eq!(
            a.blob_endpoint.as_deref(),
            Some("https://acme.blob.core.windows.net/")
        );
        assert!(a.resource_id.ends_with("/storageAccounts/acme"));
    }

    #[tokio::test]
    async fn list_keys_returns_first_key() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", mockito::Matcher::Regex("/listKeys".into()))
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_body(r#"{"keys":[{"keyName":"key1","value":"SECRET=="},{"keyName":"key2","value":"other"}]}"#)
            .create_async()
            .await;

        let arm = ArmClient::with_base("tok", server.url());
        let key = arm
            .list_keys("/subscriptions/s/resourceGroups/rg/providers/Microsoft.Storage/storageAccounts/acme")
            .await
            .unwrap();
        assert_eq!(key.as_deref(), Some("SECRET=="));
    }

    #[tokio::test]
    async fn list_keys_forbidden_is_none() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", mockito::Matcher::Regex("/listKeys".into()))
            .match_query(mockito::Matcher::Any)
            .with_status(403)
            .with_body(r#"{"error":{"code":"AuthorizationFailed"}}"#)
            .create_async()
            .await;

        let arm = ArmClient::with_base("tok", server.url());
        let key = arm.list_keys("/x/storageAccounts/acme").await.unwrap();
        assert_eq!(key, None);
    }
}
