//! Azurite local emulator provider.
//!
//! Uses the well-known Azurite developer account name and key published by
//! Microsoft. These credentials are documented at:
//! <https://learn.microsoft.com/azure/storage/common/storage-use-azurite#well-known-storage-account-and-key>

use crate::auth::{AuthProvider, ResolvedCredential};
use crate::types::{AuthKind, ResourceKind};
use async_trait::async_trait;
use secrecy::SecretString;

/// Azurite's well-known developer account name.
pub const AZURITE_ACCOUNT: &str = "devstoreaccount1";

/// Azurite's well-known developer account key. Public and documented by
/// Microsoft; safe to embed in source.
pub const AZURITE_KEY: &str =
    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";

/// Default blob endpoint for a locally-running Azurite.
pub const AZURITE_BLOB_ENDPOINT: &str = "http://127.0.0.1:10000/devstoreaccount1";

/// `AuthProvider` yielding the Azurite well-known SharedKey.
#[derive(Debug, Clone, Default)]
pub struct AzuriteEmulatorProvider;

impl AzuriteEmulatorProvider {
    /// Construct a new Azurite emulator provider.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuthProvider for AzuriteEmulatorProvider {
    fn kind(&self) -> AuthKind {
        AuthKind::AzuriteEmulator
    }
    fn display_name(&self) -> &str {
        "azurite-emulator"
    }
    async fn resolve(&self) -> crate::Result<ResolvedCredential> {
        Ok(ResolvedCredential::SharedKey {
            account_name: AZURITE_ACCOUNT.into(),
            key: SecretString::new(AZURITE_KEY.into()),
        })
    }
    fn supports(&self, _resource: ResourceKind) -> bool {
        // Azurite supports all Azure storage resource types.
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[tokio::test]
    async fn resolves_well_known_shared_key() {
        let p = AzuriteEmulatorProvider::new();
        assert_eq!(p.kind(), AuthKind::AzuriteEmulator);
        assert_eq!(p.display_name(), "azurite-emulator");

        match p.resolve().await.unwrap() {
            ResolvedCredential::SharedKey { account_name, key } => {
                assert_eq!(account_name, AZURITE_ACCOUNT);
                assert_eq!(key.expose_secret(), AZURITE_KEY);
            }
            other => panic!("expected SharedKey, got {other:?}"),
        }
    }

    #[test]
    fn supports_all_resources() {
        let p = AzuriteEmulatorProvider::new();
        for kind in [
            ResourceKind::StorageAccount,
            ResourceKind::BlobContainer,
            ResourceKind::AdlsContainer,
            ResourceKind::AdlsDirectory,
            ResourceKind::FileShare,
            ResourceKind::Queue,
            ResourceKind::Table,
        ] {
            assert!(p.supports(kind), "azurite should support {kind:?}");
        }
    }
}
