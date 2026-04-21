//! Session-scoped allow-list used by `Policy` impls to remember
//! "always allow X for this session" decisions. Truncated on Store::open.

use crate::store::Store;
use crate::Error;
use chrono::{DateTime, Utc};
use rusqlite::params;

/// An entry in the session-scope policy allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyAllowEntry {
    /// The action kind this allow covers (e.g. `"delete_blob"`).
    pub action_kind: String,
    /// The specific target this allow applies to, or `None` = any target.
    pub target: Option<String>,
    /// When the allow was recorded (this session).
    pub allowed_at: DateTime<Utc>,
}

impl Store {
    /// Record an allow for the given action kind. If `target` is `None`, the
    /// allow applies to any target.
    pub fn policy_memory_allow(
        &self,
        action_kind: &str,
        target: Option<&str>,
    ) -> Result<(), Error> {
        self.with_conn(|c| {
            c.execute(
                "INSERT INTO policy_memory (action_kind, target, allowed_at) VALUES (?1, ?2, ?3)",
                params![action_kind, target, Utc::now().to_rfc3339()],
            )
            .map_err(|e| Error::Other(anyhow::anyhow!("policy_memory allow: {e}")))?;
            Ok(())
        })
    }

    /// Check whether an action is allowed for this session. Matches a prior
    /// allow either with identical (action_kind, target) or with target = NULL.
    pub fn policy_memory_is_allowed(
        &self,
        action_kind: &str,
        target: Option<&str>,
    ) -> Result<bool, Error> {
        self.with_conn(|c| {
            let count: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM policy_memory
                 WHERE action_kind = ?1
                   AND (target IS ?2 OR target IS NULL)",
                    params![action_kind, target],
                    |r| r.get(0),
                )
                .map_err(|e| Error::Other(anyhow::anyhow!("policy_memory check: {e}")))?;
            Ok(count > 0)
        })
    }

    /// List all session-scope allow entries in chronological order.
    pub fn policy_memory_list(&self) -> Result<Vec<PolicyAllowEntry>, Error> {
        self.with_conn(|c| {
            let mut stmt = c
                .prepare(
                    "SELECT action_kind, target, allowed_at FROM policy_memory ORDER BY allowed_at",
                )
                .map_err(|e| Error::Other(anyhow::anyhow!("policy_memory list prepare: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let at: String = row.get(2)?;
                    Ok(PolicyAllowEntry {
                        action_kind: row.get(0)?,
                        target: row.get(1)?,
                        allowed_at: DateTime::parse_from_rfc3339(&at)
                            .map(|d| d.with_timezone(&Utc))
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    2,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })?,
                    })
                })
                .map_err(|e| Error::Other(anyhow::anyhow!("policy_memory list query: {e}")))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(
                    r.map_err(|e| Error::Other(anyhow::anyhow!("policy_memory list row: {e}")))?,
                );
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    #[test]
    fn allow_then_check_exact_target() {
        let s = Store::open_in_memory().unwrap();
        s.policy_memory_allow("delete_blob", Some("acmeprod"))
            .unwrap();
        assert!(s
            .policy_memory_is_allowed("delete_blob", Some("acmeprod"))
            .unwrap());
        assert!(!s
            .policy_memory_is_allowed("delete_blob", Some("other"))
            .unwrap());
        assert!(!s
            .policy_memory_is_allowed("set_tier", Some("acmeprod"))
            .unwrap());
    }

    #[test]
    fn any_target_allow_matches_everything() {
        let s = Store::open_in_memory().unwrap();
        s.policy_memory_allow("set_tier", None).unwrap();
        assert!(s.policy_memory_is_allowed("set_tier", Some("a")).unwrap());
        assert!(s.policy_memory_is_allowed("set_tier", Some("b")).unwrap());
        assert!(s.policy_memory_is_allowed("set_tier", None).unwrap());
    }

    #[test]
    fn list_returns_entries_in_order() {
        let s = Store::open_in_memory().unwrap();
        s.policy_memory_allow("delete_blob", Some("a")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        s.policy_memory_allow("set_tier", None).unwrap();
        let list = s.policy_memory_list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].action_kind, "delete_blob");
        assert_eq!(list[1].action_kind, "set_tier");
    }
}
