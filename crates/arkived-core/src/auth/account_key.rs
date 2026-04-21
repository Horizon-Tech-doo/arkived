//! Storage-account-key auth provider.
//!
//! Wraps an account name + base64-encoded key. Actual HMAC signing happens
//! in the [`shared_key`](super::shared_key) pipeline policy once a request
//! is being built — this provider only resolves the raw material.

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;
use secrecy::SecretString;

/// Shared-key (account-name + account-key) auth.
#[derive(Debug, Clone)]
pub struct AccountKeyProvider {
    account_name: String,
    key: SecretString,
}

impl AccountKeyProvider {
    /// Construct from an account name and its base64-encoded key.
    pub fn new(account_name: impl Into<String>, key: SecretString) -> Self {
        Self {
            account_name: account_name.into(),
            key,
        }
    }

    /// Storage account name.
    pub fn account_name(&self) -> &str {
        &self.account_name
    }
}

#[async_trait]
impl AuthProvider for AccountKeyProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::AccountKey
    }
    fn display_name(&self) -> &str {
        &self.account_name
    }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        Ok(ResolvedCredential::SharedKey {
            account_name: self.account_name.clone(),
            key: self.key.clone(),
        })
    }
    fn supports(&self, resource: ResourceKind) -> bool {
        // Account-key works against the whole storage account.
        matches!(
            resource,
            ResourceKind::StorageAccount
                | ResourceKind::BlobContainer
                | ResourceKind::AdlsContainer
                | ResourceKind::AdlsDirectory
                | ResourceKind::Queue
                | ResourceKind::Table
                | ResourceKind::FileShare
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[tokio::test]
    async fn resolves_shared_key_variant() {
        let p =
            AccountKeyProvider::new("acmeprod", SecretString::new("dGVzdGtleWJhc2U2NA==".into()));
        assert_eq!(p.kind(), AuthKind::AccountKey);
        assert_eq!(p.display_name(), "acmeprod");
        assert_eq!(p.account_name(), "acmeprod");

        match p.resolve().await.unwrap() {
            ResolvedCredential::SharedKey { account_name, key } => {
                assert_eq!(account_name, "acmeprod");
                assert_eq!(key.expose_secret(), "dGVzdGtleWJhc2U2NA==");
            }
            other => panic!("expected SharedKey, got {other:?}"),
        }
    }

    #[test]
    fn supports_all_storage_resource_kinds() {
        let p = AccountKeyProvider::new("x", SecretString::new("k".into()));
        assert!(p.supports(ResourceKind::StorageAccount));
        assert!(p.supports(ResourceKind::BlobContainer));
        assert!(p.supports(ResourceKind::Queue));
    }
}
