//! CRUD for the `subscription` table.

use crate::Error;
use crate::store::Store;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// An Azure subscription discovered under a sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// Azure subscription ID (UUID-shaped).
    pub id: String,
    /// Foreign key to the owning `sign_in.id`.
    pub sign_in_id: String,
    /// Display name of the subscription.
    pub name: String,
    /// Tenant ID the subscription resides in.
    pub tenant_id: String,
    /// When this subscription was discovered/refreshed (UTC).
    pub discovered_at: DateTime<Utc>,
}

impl Store {
    /// Insert or update a subscription (upsert on `id`).
    pub fn subscription_upsert(&self, s: &Subscription) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO subscription (id, sign_in_id, name, tenant_id, discovered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    sign_in_id = excluded.sign_in_id,
                    name = excluded.name,
                    tenant_id = excluded.tenant_id,
                    discovered_at = excluded.discovered_at",
                params![s.id, s.sign_in_id, s.name, s.tenant_id, s.discovered_at.to_rfc3339()],
            ).map_err(|e| Error::Other(anyhow::anyhow!("subscription upsert: {e}")))?;
            Ok(())
        })
    }

    /// Fetch a subscription by id. Returns `Ok(None)` if not found.
    pub fn subscription_get(&self, id: &str) -> Result<Option<Subscription>, Error> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT id, sign_in_id, name, tenant_id, discovered_at FROM subscription WHERE id = ?1",
                params![id],
                row_to_sub,
            ).optional().map_err(|e| Error::Other(anyhow::anyhow!("subscription get: {e}")))
        })
    }

    /// List all subscriptions under a specific sign-in, ordered by name.
    pub fn subscription_list_for_sign_in(&self, sign_in_id: &str) -> Result<Vec<Subscription>, Error> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(
                "SELECT id, sign_in_id, name, tenant_id, discovered_at FROM subscription
                 WHERE sign_in_id = ?1 ORDER BY name"
            ).map_err(|e| Error::Other(anyhow::anyhow!("subscription list prepare: {e}")))?;
            let rows = stmt.query_map(params![sign_in_id], row_to_sub)
                .map_err(|e| Error::Other(anyhow::anyhow!("subscription list query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| Error::Other(anyhow::anyhow!("subscription list row: {e}")))?);
            }
            Ok(out)
        })
    }
}

fn row_to_sub(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let discovered_at: String = row.get(4)?;
    Ok(Subscription {
        id: row.get(0)?,
        sign_in_id: row.get(1)?,
        name: row.get(2)?,
        tenant_id: row.get(3)?,
        discovered_at: DateTime::parse_from_rfc3339(&discovered_at)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Store, SignIn};

    fn parent_sign_in() -> SignIn {
        SignIn {
            id: "si-1".into(),
            display_name: "Hamza".into(),
            tenant_id: "tenant-abc".into(),
            environment: "azure".into(),
            user_principal: "h@x".into(),
            added_at: Utc::now(),
        }
    }

    fn child_sub() -> Subscription {
        Subscription {
            id: "sub-1".into(),
            sign_in_id: "si-1".into(),
            name: "dev".into(),
            tenant_id: "tenant-abc".into(),
            discovered_at: Utc::now(),
        }
    }

    #[test]
    fn upsert_get_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        s.sign_in_insert(&parent_sign_in()).unwrap();
        s.subscription_upsert(&child_sub()).unwrap();
        let got = s.subscription_get("sub-1").unwrap().unwrap();
        assert_eq!(got.name, "dev");
    }

    #[test]
    fn upsert_updates_existing() {
        let s = Store::open_in_memory().unwrap();
        s.sign_in_insert(&parent_sign_in()).unwrap();
        let mut first = child_sub();
        s.subscription_upsert(&first).unwrap();
        first.name = "prod".into();
        s.subscription_upsert(&first).unwrap();
        assert_eq!(s.subscription_get("sub-1").unwrap().unwrap().name, "prod");
    }

    #[test]
    fn list_for_sign_in() {
        let s = Store::open_in_memory().unwrap();
        s.sign_in_insert(&parent_sign_in()).unwrap();
        let a = child_sub();
        let mut b = child_sub();
        b.id = "sub-2".into();
        b.name = "prod".into();
        s.subscription_upsert(&a).unwrap();
        s.subscription_upsert(&b).unwrap();
        let subs = s.subscription_list_for_sign_in("si-1").unwrap();
        assert_eq!(subs.len(), 2);
        let names: Vec<_> = subs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["dev", "prod"]);
    }

    #[test]
    fn cascade_delete_from_sign_in() {
        let s = Store::open_in_memory().unwrap();
        s.sign_in_insert(&parent_sign_in()).unwrap();
        s.subscription_upsert(&child_sub()).unwrap();
        // Enable FK pragma locally for this test (Task 14 will make it default).
        s.with_conn(|c| { c.pragma_update(None, "foreign_keys", "ON").map_err(|e| Error::Other(anyhow::anyhow!(e))) }).unwrap();
        s.sign_in_delete("si-1").unwrap();
        assert!(s.subscription_get("sub-1").unwrap().is_none());
    }
}
