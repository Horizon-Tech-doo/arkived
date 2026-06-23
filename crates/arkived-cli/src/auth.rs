//! Global connection flags, mapped onto the shared core resolver.
//!
//! Resolution order (first match wins): `--azurite`, `--connection-string`,
//! `--sas` (+ `--account`/`--endpoint`), `--account-key` (+ `--account`). The
//! actual credential → backend logic lives in [`arkived_core::ConnectionParts`]
//! so the CLI and the MCP server share one path.

use anyhow::Result;
use arkived_core::{AzureBlobBackend, AzureQueueBackend, ConnectionParts};
use clap::Args;

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
    fn parts(&self) -> ConnectionParts {
        ConnectionParts {
            connection_string: self.connection_string.clone(),
            account: self.account.clone(),
            account_key: self.account_key.clone(),
            sas: self.sas.clone(),
            endpoint: self.endpoint.clone(),
            azurite: self.azurite,
        }
    }

    /// The connection parts as supplied by flags/env (no saved-account fallback).
    /// Used by `account add` to persist exactly what the user passed.
    pub fn connection_parts(&self) -> ConnectionParts {
        self.parts()
    }

    /// The effective connection parts: explicit flags/env if present, otherwise
    /// the active saved account (`account use`). Bails if neither is available.
    fn effective_parts(&self) -> Result<ConnectionParts> {
        let parts = self.parts();
        if !parts.is_empty() {
            return Ok(parts);
        }
        if let Some((name, saved)) = crate::account::active_parts(
            &crate::account::open_store()?,
            &crate::account::keyring(),
        )? {
            tracing::debug!(account = %name, "using saved account");
            return Ok(saved);
        }
        anyhow::bail!(
            "no credentials provided. Use one of: --azurite, --connection-string, \
             --sas (+ --account/--endpoint), or --account-key --account; or select a saved \
             account with `arkived account use <name>`. \
             Env vars ARKIVED_CONNECTION_STRING / ARKIVED_SAS / ARKIVED_ACCOUNT_KEY also work."
        )
    }

    /// Resolve into a connected [`AzureBlobBackend`].
    pub async fn resolve_backend(&self) -> Result<AzureBlobBackend> {
        self.effective_parts()?.resolve().await.map_err(Into::into)
    }

    /// Resolve into a connected [`AzureQueueBackend`].
    pub async fn resolve_queue_backend(&self) -> Result<AzureQueueBackend> {
        self.effective_parts()?
            .resolve_queue()
            .await
            .map_err(Into::into)
    }

    /// A short human label for the active connection (for `doctor`).
    pub fn describe(&self) -> String {
        self.parts().describe()
    }
}
