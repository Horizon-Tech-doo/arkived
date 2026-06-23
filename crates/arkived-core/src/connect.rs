//! Resolving an [`AzureBlobBackend`] from plain connection parts.
//!
//! Both the CLI (from flags/env) and the MCP server (from env) build a
//! [`ConnectionParts`] and call [`ConnectionParts::resolve`], so credential →
//! backend resolution lives in exactly one place.

use crate::auth::{
    azurite::AZURITE_BLOB_ENDPOINT, AccountKeyProvider, AuthProvider, AzuriteEmulatorProvider,
    ConnectionStringParts, ConnectionStringProvider, SasTokenProvider,
};
use crate::{AzureBlobBackend, Error};
use secrecy::SecretString;

/// The set of credential inputs a surface can supply. Resolution tries them in
/// the documented priority order; the first populated one wins.
#[derive(Debug, Clone, Default)]
pub struct ConnectionParts {
    /// Full Azure Storage connection string.
    pub connection_string: Option<String>,
    /// Storage account name (used with `account_key` or `sas`).
    pub account: Option<String>,
    /// Storage account key (base64).
    pub account_key: Option<String>,
    /// A SAS token (with or without leading `?`).
    pub sas: Option<String>,
    /// Explicit blob endpoint URL (overrides the one derived from `account`).
    pub endpoint: Option<String>,
    /// Use the local Azurite emulator.
    pub azurite: bool,
}

impl ConnectionParts {
    /// Read connection parts from the `ARKIVED_*` environment variables:
    /// `ARKIVED_CONNECTION_STRING`, `ARKIVED_ACCOUNT`, `ARKIVED_ACCOUNT_KEY`,
    /// `ARKIVED_SAS`, `ARKIVED_ENDPOINT`, and `ARKIVED_AZURITE` (any non-empty
    /// value enables the emulator).
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            connection_string: var("ARKIVED_CONNECTION_STRING"),
            account: var("ARKIVED_ACCOUNT"),
            account_key: var("ARKIVED_ACCOUNT_KEY"),
            sas: var("ARKIVED_SAS"),
            endpoint: var("ARKIVED_ENDPOINT"),
            azurite: var("ARKIVED_AZURITE").is_some(),
        }
    }

    /// Whether any credential input is present.
    pub fn is_empty(&self) -> bool {
        !self.azurite
            && self.connection_string.is_none()
            && self.sas.is_none()
            && self.account_key.is_none()
    }

    /// A short human label for the active connection.
    pub fn describe(&self) -> String {
        if self.azurite {
            "azurite emulator".into()
        } else if self.connection_string.is_some() {
            "connection string".into()
        } else if self.sas.is_some() {
            format!(
                "SAS ({})",
                self.account.as_deref().unwrap_or("via endpoint")
            )
        } else if self.account_key.is_some() {
            format!("account key ({})", self.account.as_deref().unwrap_or("?"))
        } else {
            "none".into()
        }
    }

    /// Resolve these parts into a connected [`AzureBlobBackend`].
    pub async fn resolve(&self) -> crate::Result<AzureBlobBackend> {
        let (credential, endpoint) = self.materialize().await?;
        AzureBlobBackend::new(endpoint, credential)
    }

    /// Resolve these parts into a connected [`AzureQueueBackend`].
    ///
    /// The queue endpoint is derived from the blob endpoint (swapping `.blob.`
    /// for `.queue.`), except for Azurite where the emulator's queue port is used.
    pub async fn resolve_queue(&self) -> crate::Result<crate::AzureQueueBackend> {
        if self.azurite {
            let credential = AzuriteEmulatorProvider::new().resolve().await?;
            let endpoint = parse_endpoint("http://127.0.0.1:10001/devstoreaccount1")?;
            return crate::AzureQueueBackend::new(endpoint, credential);
        }
        let (credential, blob_endpoint) = self.materialize().await?;
        crate::AzureQueueBackend::from_blob_endpoint(&blob_endpoint, credential)
    }

    /// Resolve the credential and blob endpoint shared by both backends.
    async fn materialize(&self) -> crate::Result<(crate::auth::ResolvedCredential, url::Url)> {
        if self.azurite {
            let credential = AzuriteEmulatorProvider::new().resolve().await?;
            return Ok((credential, parse_endpoint(AZURITE_BLOB_ENDPOINT)?));
        }

        if let Some(cs) = &self.connection_string {
            let parts = ConnectionStringParts::parse(cs)?;
            let endpoint = self
                .endpoint
                .clone()
                .or_else(|| parts.blob_endpoint())
                .ok_or_else(|| {
                    Error::AuthFailed("connection string lacks a blob endpoint".into())
                })?;
            let credential = ConnectionStringProvider::new("connection-string", secret(cs))?
                .resolve()
                .await?;
            return Ok((credential, parse_endpoint(&endpoint)?));
        }

        if let Some(sas) = &self.sas {
            let endpoint = self.endpoint_for_account("a SAS token")?;
            let credential = SasTokenProvider::new("sas", secret(sas))?.resolve().await?;
            return Ok((credential, parse_endpoint(&endpoint)?));
        }

        if let Some(key) = &self.account_key {
            let account = self
                .account
                .clone()
                .ok_or_else(|| Error::AuthFailed("account key requires an account name".into()))?;
            let endpoint = self.endpoint_for_account("an account key")?;
            let credential = AccountKeyProvider::new(account, secret(key))
                .resolve()
                .await?;
            return Ok((credential, parse_endpoint(&endpoint)?));
        }

        Err(Error::AuthFailed(
            "no credentials provided (connection string, SAS, account key, or azurite)".into(),
        ))
    }

    fn endpoint_for_account(&self, who: &str) -> crate::Result<String> {
        if let Some(e) = &self.endpoint {
            return Ok(e.clone());
        }
        let account = self.account.clone().ok_or_else(|| {
            Error::AuthFailed(format!(
                "{who} requires an account name or explicit endpoint"
            ))
        })?;
        Ok(format!("https://{account}.blob.core.windows.net"))
    }
}

fn secret(s: &str) -> SecretString {
    SecretString::new(s.to_string())
}

fn parse_endpoint(s: &str) -> crate::Result<url::Url> {
    url::Url::parse(s).map_err(|e| Error::Backend(format!("invalid endpoint URL '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_parts_error() {
        let parts = ConnectionParts::default();
        assert!(parts.is_empty());
        assert!(parts.resolve().await.is_err());
    }

    #[tokio::test]
    async fn azurite_resolves() {
        let parts = ConnectionParts {
            azurite: true,
            ..Default::default()
        };
        assert!(!parts.is_empty());
        let backend = parts.resolve().await.unwrap();
        assert!(backend.endpoint().as_str().contains("devstoreaccount1"));
    }

    #[tokio::test]
    async fn account_key_without_account_errors() {
        let parts = ConnectionParts {
            account_key: Some("a2V5".into()),
            ..Default::default()
        };
        assert!(parts.resolve().await.is_err());
    }

    #[tokio::test]
    async fn connection_string_resolves_with_endpoint() {
        let parts = ConnectionParts {
            connection_string: Some(
                "DefaultEndpointsProtocol=https;AccountName=acme;AccountKey=dGVzdA==;\
                 EndpointSuffix=core.windows.net"
                    .into(),
            ),
            ..Default::default()
        };
        let backend = parts.resolve().await.unwrap();
        assert_eq!(
            backend.endpoint().as_str(),
            "https://acme.blob.core.windows.net/"
        );
    }

    #[test]
    fn describe_reflects_active_method() {
        assert_eq!(
            ConnectionParts {
                azurite: true,
                ..Default::default()
            }
            .describe(),
            "azurite emulator"
        );
    }
}
