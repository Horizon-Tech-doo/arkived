//! CRUD for the `storage_account` table.

use crate::store::Store;
use crate::Error;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// An Azure storage account — either discovered under a subscription or
/// directly attached via SAS/connection-string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageAccount {
    /// Azure-unique storage account name.
    pub name: String,
    /// Owning subscription, or `None` if attached directly (not inside a sign-in).
    pub subscription_id: Option<String>,
    /// SKU kind (`StorageV2`, `StorageV2 (ADLS Gen2)`, …).
    pub kind: String,
    /// Azure region.
    pub region: String,
    /// Replication strategy (`LRS`, `GRS`, `ZRS`, `RA-GZRS`).
    pub replication: String,
    /// Performance tier (`Standard` or `Premium`).
    pub tier: String,
    /// Whether hierarchical namespace (ADLS Gen2) is enabled.
    pub hns: bool,
    /// Blob endpoint URL.
    pub endpoint: String,
    /// Whether this account was attached directly (outside a sign-in) via SAS/key.
    pub attached_directly: bool,
}

impl Store {
    /// Insert or update a storage account (upsert on `name`).
    pub fn storage_account_upsert(&self, a: &StorageAccount) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO storage_account
                   (name, subscription_id, kind, region, replication, tier, hns, endpoint, attached_directly)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(name) DO UPDATE SET
                   subscription_id = excluded.subscription_id,
                   kind = excluded.kind,
                   region = excluded.region,
                   replication = excluded.replication,
                   tier = excluded.tier,
                   hns = excluded.hns,
                   endpoint = excluded.endpoint,
                   attached_directly = excluded.attached_directly",
                params![
                    a.name, a.subscription_id, a.kind, a.region, a.replication,
                    a.tier, a.hns as i32, a.endpoint, a.attached_directly as i32,
                ],
            ).map_err(|e| Error::Other(anyhow::anyhow!("storage_account upsert: {e}")))?;
            Ok(())
        })
    }

    /// Fetch a storage account by name. Returns `Ok(None)` if not found.
    pub fn storage_account_get(&self, name: &str) -> Result<Option<StorageAccount>, Error> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT name, subscription_id, kind, region, replication, tier, hns, endpoint, attached_directly
                 FROM storage_account WHERE name = ?1",
                params![name],
                row_to_account,
            ).optional().map_err(|e| Error::Other(anyhow::anyhow!("storage_account get: {e}")))
        })
    }

    /// List all storage accounts under a subscription, ordered by name.
    pub fn storage_account_list_for_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<Vec<StorageAccount>, Error> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT name, subscription_id, kind, region, replication, tier, hns, endpoint, attached_directly
                 FROM storage_account WHERE subscription_id = ?1 ORDER BY name"
            ).map_err(|e| Error::Other(anyhow::anyhow!("storage_account list prepare: {e}")))?;
            let rows = stmt.query_map(params![subscription_id], row_to_account)
                .map_err(|e| Error::Other(anyhow::anyhow!("storage_account list query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| Error::Other(anyhow::anyhow!("storage_account list row: {e}")))?);
            }
            Ok(out)
        })
    }

    /// List all storage accounts attached directly (outside any sign-in).
    pub fn storage_account_list_attached_directly(&self) -> Result<Vec<StorageAccount>, Error> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT name, subscription_id, kind, region, replication, tier, hns, endpoint, attached_directly
                 FROM storage_account WHERE attached_directly = 1 ORDER BY name"
            ).map_err(|e| Error::Other(anyhow::anyhow!("storage_account list_attached prepare: {e}")))?;
            let rows = stmt.query_map([], row_to_account)
                .map_err(|e| Error::Other(anyhow::anyhow!("storage_account list_attached query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| Error::Other(anyhow::anyhow!("storage_account list_attached row: {e}")))?);
            }
            Ok(out)
        })
    }

    /// Delete a storage account by name.
    pub fn storage_account_delete(&self, name: &str) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute("DELETE FROM storage_account WHERE name = ?1", params![name])
                .map_err(|e| Error::Other(anyhow::anyhow!("storage_account delete: {e}")))?;
            Ok(())
        })
    }
}

fn row_to_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<StorageAccount> {
    Ok(StorageAccount {
        name: row.get(0)?,
        subscription_id: row.get(1)?,
        kind: row.get(2)?,
        region: row.get(3)?,
        replication: row.get(4)?,
        tier: row.get(5)?,
        hns: row.get::<_, i32>(6)? != 0,
        endpoint: row.get(7)?,
        attached_directly: row.get::<_, i32>(8)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{SignIn, Store, Subscription};
    use chrono::Utc;

    fn seed(store: &Store) {
        store
            .sign_in_insert(&SignIn {
                id: "si-1".into(),
                display_name: "Hamza".into(),
                tenant_id: "t".into(),
                environment: "azure".into(),
                user_principal: "h@x".into(),
                added_at: Utc::now(),
            })
            .unwrap();
        store
            .subscription_upsert(&Subscription {
                id: "sub-1".into(),
                sign_in_id: "si-1".into(),
                name: "dev".into(),
                tenant_id: "t".into(),
                discovered_at: Utc::now(),
            })
            .unwrap();
    }

    fn account(name: &str) -> StorageAccount {
        StorageAccount {
            name: name.into(),
            subscription_id: Some("sub-1".into()),
            kind: "StorageV2 (ADLS Gen2)".into(),
            region: "West Europe".into(),
            replication: "LRS".into(),
            tier: "Premium".into(),
            hns: true,
            endpoint: format!("https://{name}.blob.core.windows.net"),
            attached_directly: false,
        }
    }

    #[test]
    fn upsert_get_and_hns_bool_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        seed(&s);
        s.storage_account_upsert(&account("acmeprod")).unwrap();
        let got = s.storage_account_get("acmeprod").unwrap().unwrap();
        assert!(got.hns);
        assert!(!got.attached_directly);
    }

    #[test]
    fn list_for_subscription() {
        let s = Store::open_in_memory().unwrap();
        seed(&s);
        s.storage_account_upsert(&account("acmea")).unwrap();
        s.storage_account_upsert(&account("acmeb")).unwrap();
        let list = s.storage_account_list_for_subscription("sub-1").unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn list_attached_directly_only() {
        let s = Store::open_in_memory().unwrap();
        seed(&s);
        let normal = account("acmeprod");
        let mut attached = account("attached");
        attached.subscription_id = None;
        attached.attached_directly = true;
        s.storage_account_upsert(&normal).unwrap();
        s.storage_account_upsert(&attached).unwrap();
        let list = s.storage_account_list_attached_directly().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "attached");
    }

    #[test]
    fn delete() {
        let s = Store::open_in_memory().unwrap();
        seed(&s);
        s.storage_account_upsert(&account("acmeprod")).unwrap();
        s.storage_account_delete("acmeprod").unwrap();
        assert!(s.storage_account_get("acmeprod").unwrap().is_none());
    }
}
