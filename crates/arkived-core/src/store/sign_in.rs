//! CRUD for the `sign_in` table.

use crate::store::Store;
use crate::Error;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// A persisted Microsoft Entra sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignIn {
    /// Opaque stable identifier (caller-chosen — typically a UUID).
    pub id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Entra tenant ID associated with this sign-in.
    pub tenant_id: String,
    /// Azure environment (`"azure"`, `"china"`, `"usgov"`, …).
    pub environment: String,
    /// Signed-in user principal (e.g. `hamza@example.com`).
    pub user_principal: String,
    /// When this sign-in was added to the store (UTC).
    pub added_at: DateTime<Utc>,
}

impl Store {
    /// Insert a new sign-in. Fails on duplicate `id`.
    pub fn sign_in_insert(&self, s: &SignIn) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO sign_in (id, display_name, tenant_id, environment, user_principal, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![s.id, s.display_name, s.tenant_id, s.environment, s.user_principal, s.added_at.to_rfc3339()],
            ).map_err(|e| Error::Other(anyhow::anyhow!("sign_in insert: {e}")))?;
            Ok(())
        })
    }

    /// Fetch a sign-in by id. Returns `Ok(None)` if not found.
    pub fn sign_in_get(&self, id: &str) -> Result<Option<SignIn>, Error> {
        self.with_conn(|c| {
            c.query_row(
                "SELECT id, display_name, tenant_id, environment, user_principal, added_at
                 FROM sign_in WHERE id = ?1",
                params![id],
                row_to_sign_in,
            )
            .optional()
            .map_err(|e| Error::Other(anyhow::anyhow!("sign_in get: {e}")))
        })
    }

    /// List all sign-ins ordered by `added_at` ascending.
    pub fn sign_in_list(&self) -> Result<Vec<SignIn>, Error> {
        self.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT id, display_name, tenant_id, environment, user_principal, added_at
                 FROM sign_in ORDER BY added_at",
                )
                .map_err(|e| Error::Other(anyhow::anyhow!("sign_in list prepare: {e}")))?;
            let rows = stmt
                .query_map([], row_to_sign_in)
                .map_err(|e| Error::Other(anyhow::anyhow!("sign_in list query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r.map_err(|e| Error::Other(anyhow::anyhow!("sign_in list row: {e}")))?);
            }
            Ok(out)
        })
    }

    /// Delete a sign-in by id. Cascades to subscriptions via FK.
    pub fn sign_in_delete(&self, id: &str) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute("DELETE FROM sign_in WHERE id = ?1", params![id])
                .map_err(|e| Error::Other(anyhow::anyhow!("sign_in delete: {e}")))?;
            Ok(())
        })
    }
}

fn row_to_sign_in(row: &rusqlite::Row<'_>) -> rusqlite::Result<SignIn> {
    let added_at: String = row.get(5)?;
    Ok(SignIn {
        id: row.get(0)?,
        display_name: row.get(1)?,
        tenant_id: row.get(2)?,
        environment: row.get(3)?,
        user_principal: row.get(4)?,
        added_at: DateTime::parse_from_rfc3339(&added_at)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn sample() -> SignIn {
        SignIn {
            id: "sign-1".into(),
            display_name: "Hamza".into(),
            tenant_id: "tenant-abc".into(),
            environment: "azure".into(),
            user_principal: "hamza@horizon-tech.io".into(),
            added_at: Utc::now(),
        }
    }

    #[test]
    fn insert_get_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        let a = sample();
        s.sign_in_insert(&a).unwrap();
        let got = s.sign_in_get(&a.id).unwrap().unwrap();
        assert_eq!(got.id, a.id);
        assert_eq!(got.display_name, a.display_name);
        assert_eq!(got.tenant_id, a.tenant_id);
    }

    #[test]
    fn get_missing_is_none() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.sign_in_get("nope").unwrap().is_none());
    }

    #[test]
    fn list_and_delete() {
        let s = Store::open_in_memory().unwrap();
        let a = sample();
        let mut b = sample();
        b.id = "sign-2".into();
        b.user_principal = "other@horizon-tech.io".into();
        b.added_at = a.added_at + chrono::Duration::seconds(1);

        s.sign_in_insert(&a).unwrap();
        s.sign_in_insert(&b).unwrap();
        let list = s.sign_in_list().unwrap();
        assert_eq!(list.len(), 2);

        s.sign_in_delete(&a.id).unwrap();
        let list = s.sign_in_list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "sign-2");
    }
}
