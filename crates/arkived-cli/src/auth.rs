//! Resolving a backend from CLI flags / environment.
//!
//! Resolution order (first match wins):
//! 1. `--azurite` — the local Azure Storage emulator.
//! 2. `--connection-string` / `ARKIVED_CONNECTION_STRING`.
//! 3. `--sas` / `ARKIVED_SAS` (needs `--account` or `--endpoint`).
//! 4. `--account-key` / `ARKIVED_ACCOUNT_KEY` (needs `--account`).
//!
//! Shared `Store` + keyring resolution (a saved sign-in / current context) is a
//! follow-up; see the completion design spec.

use anyhow::{bail, Context, Result};
use arkived_core::auth::azurite::AZURITE_BLOB_ENDPOINT;
use arkived_core::auth::{
    AccountKeyProvider, AuthProvider, AzuriteEmulatorProvider, ConnectionStringParts,
    ConnectionStringProvider, SasTokenProvider,
};
use arkived_core::AzureBlobBackend;
use clap::Args;
use secrecy::SecretString;

/// Global connection flags, available on every subcommand.
#[derive(Args, Debug, Clone)]
pub struct AuthArgs {
    /// Full Azure Storage connection string.
    #[arg(long, env = "ARKIVED_CONNECTION_STRING", global = true)]
    pub connection_string: Option<String>,

    /// Storage account name (used with --account-key or --sas).
    #[arg(long, env = "ARKIVED_ACCOUNT", global = true)]
    pub account: Option<String>,

    /// Storage account key (base64). Pairs with --account.
    #[arg(long, env = "ARKIVED_ACCOUNT_KEY", global = true)]
    pub account_key: Option<String>,

    /// A SAS token (with or without leading '?'). Pairs with --account/--endpoint.
    #[arg(long, env = "ARKIVED_SAS", global = true)]
    pub sas: Option<String>,

    /// Explicit blob endpoint URL (overrides the one derived from --account).
    #[arg(long, env = "ARKIVED_ENDPOINT", global = true)]
    pub endpoint: Option<String>,

    /// Use the local Azurite emulator.
    #[arg(long, global = true)]
    pub azurite: bool,
}

impl AuthArgs {
    /// Resolve these flags into a connected [`AzureBlobBackend`].
    pub async fn resolve_backend(&self) -> Result<AzureBlobBackend> {
        if self.azurite {
            let provider = AzuriteEmulatorProvider::new();
            let credential = provider.resolve().await?;
            let endpoint = parse_endpoint(AZURITE_BLOB_ENDPOINT)?;
            return AzureBlobBackend::new(endpoint, credential).map_err(Into::into);
        }

        if let Some(cs) = &self.connection_string {
            let parts = ConnectionStringParts::parse(cs)?;
            let endpoint = self
                .endpoint
                .clone()
                .or_else(|| parts.blob_endpoint())
                .context("connection string lacks a blob endpoint; pass --endpoint")?;
            let provider =
                ConnectionStringProvider::new("connection-string", SecretString::new(cs.clone()))?;
            let credential = provider.resolve().await?;
            return AzureBlobBackend::new(parse_endpoint(&endpoint)?, credential)
                .map_err(Into::into);
        }

        if let Some(sas) = &self.sas {
            let endpoint = self.endpoint_for_account("--sas")?;
            let provider = SasTokenProvider::new("sas", SecretString::new(sas.clone()))?;
            let credential = provider.resolve().await?;
            return AzureBlobBackend::new(parse_endpoint(&endpoint)?, credential)
                .map_err(Into::into);
        }

        if let Some(key) = &self.account_key {
            let account = self
                .account
                .clone()
                .context("--account-key requires --account")?;
            let endpoint = self.endpoint_for_account("--account-key")?;
            let provider = AccountKeyProvider::new(account, SecretString::new(key.clone()));
            let credential = provider.resolve().await?;
            return AzureBlobBackend::new(parse_endpoint(&endpoint)?, credential)
                .map_err(Into::into);
        }

        bail!(
            "no credentials provided. Use one of: --azurite, --connection-string, \
             --sas (+ --account/--endpoint), or --account-key --account. \
             Env vars ARKIVED_CONNECTION_STRING / ARKIVED_SAS / ARKIVED_ACCOUNT_KEY also work."
        )
    }

    /// A short human label for the active connection (for `doctor`).
    pub fn describe(&self) -> String {
        if self.azurite {
            "azurite emulator".into()
        } else if self.connection_string.is_some() {
            "connection string".into()
        } else if self.sas.is_some() {
            format!(
                "SAS ({})",
                self.account.as_deref().unwrap_or("via --endpoint")
            )
        } else if self.account_key.is_some() {
            format!("account key ({})", self.account.as_deref().unwrap_or("?"))
        } else {
            "none".into()
        }
    }

    /// Build the blob endpoint from `--endpoint`, or from `--account` as
    /// `https://<account>.blob.core.windows.net`.
    fn endpoint_for_account(&self, who: &str) -> Result<String> {
        if let Some(e) = &self.endpoint {
            return Ok(e.clone());
        }
        let account = self
            .account
            .clone()
            .with_context(|| format!("{who} requires --account or --endpoint"))?;
        Ok(format!("https://{account}.blob.core.windows.net"))
    }
}

fn parse_endpoint(s: &str) -> Result<url::Url> {
    url::Url::parse(s).with_context(|| format!("invalid endpoint URL: {s}"))
}
