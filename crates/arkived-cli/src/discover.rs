//! ARM account discovery for the active sign-in.
//!
//! Uses the signed-in user's cached refresh token to mint an ARM token, then
//! enumerates subscriptions and their storage accounts into the local catalog.
//! For each account it best-effort fetches an access key (`listKeys`) and saves
//! usable credentials to the keychain, so a discovered account can be selected
//! with `account use` and browsed immediately.

use crate::account;
use anyhow::{bail, Result};
use arkived_core::{
    ArmClient, ConnectionParts, CredentialStore, StorageAccount, Store, Subscription,
};

/// Outcome counts for a discovery run.
#[derive(Debug, Default)]
pub struct Summary {
    /// Subscriptions catalogued.
    pub subscriptions: usize,
    /// Storage accounts catalogued.
    pub accounts: usize,
    /// Accounts for which a key was retrieved (immediately usable).
    pub with_keys: usize,
    /// Accounts saved as metadata only (no key — insufficient permission).
    pub metadata_only: usize,
}

/// Discover subscriptions + storage accounts for the active sign-in and persist
/// them. Requires a prior `arkived login`.
pub async fn discover(store: &Store, secrets: &dyn CredentialStore) -> Result<Summary> {
    let Some(sign_in_id) = store.context_get()?.sign_in_id else {
        bail!("no active sign-in; run `arkived login` first");
    };

    let token = arkived_core::arm::arm_token_for(secrets, &sign_in_id).await?;
    let arm = ArmClient::new(token);

    let mut summary = Summary::default();
    for sub in arm.list_subscriptions().await? {
        store.subscription_upsert(&Subscription::now(
            &sub.id,
            &sign_in_id,
            &sub.name,
            sub.tenant_id.clone().unwrap_or_default(),
        ))?;
        summary.subscriptions += 1;

        for acct in arm.list_storage_accounts(&sub.id).await? {
            store.storage_account_upsert(&StorageAccount {
                name: acct.name.clone(),
                subscription_id: Some(sub.id.clone()),
                kind: acct.kind.clone(),
                region: acct.location.clone(),
                replication: acct.sku_name.clone(),
                tier: acct.sku_tier.clone(),
                hns: acct.hns,
                endpoint: acct.blob_endpoint.clone().unwrap_or_default(),
                attached_directly: false,
            })?;
            summary.accounts += 1;

            // Best-effort: fetch a key so the account is immediately usable.
            match arm.list_keys(&acct.resource_id).await {
                Ok(Some(key)) => {
                    let parts = ConnectionParts {
                        account: Some(acct.name.clone()),
                        account_key: Some(key),
                        endpoint: acct.blob_endpoint.clone(),
                        ..Default::default()
                    };
                    account::save_credentials(secrets, &acct.name, &parts)?;
                    summary.with_keys += 1;
                }
                Ok(None) | Err(_) => summary.metadata_only += 1,
            }
        }
    }
    Ok(summary)
}
