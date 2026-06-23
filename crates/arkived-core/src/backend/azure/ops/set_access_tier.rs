//! `PUT /{container}/{blob}?comp=tier` — policy-gated access-tier change.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, Tier};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::Method;

impl AzureBlobBackend {
    /// Change a blob's access tier (Hot/Cool/Cold/Archive). Calls
    /// `ctx.policy.confirm("set_access_tier", ...)` before any HTTP is sent, and
    /// denies the operation on `Deny`.
    pub async fn set_access_tier(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        tier: Tier,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "set_access_tier".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!(
                        "set tier of {}/{} to {}",
                        path.container,
                        path.blob,
                        tier.as_str()
                    ),
                    reversible: !matches!(tier, Tier::Archive),
                },
                &ActionContext {
                    item_count: Some(1),
                    ..Default::default()
                },
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=tier"));

        let headers = vec![("x-ms-access-tier".to_string(), tier.as_str().to_string())];

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Empty,
            })
            .await?;
        Ok(())
    }

    /// Rehydrate an archived blob to an online tier (Hot/Cool/Cold) with a
    /// rehydration priority. `high_priority` requests the (more expensive)
    /// High priority; otherwise Standard. Policy-gated like a tier change.
    pub async fn rehydrate_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        target_tier: Tier,
        high_priority: bool,
    ) -> crate::Result<()> {
        let priority = if high_priority { "High" } else { "Standard" };
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "rehydrate_blob".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!(
                        "rehydrate {}/{} to {} ({priority} priority)",
                        path.container,
                        path.blob,
                        target_tier.as_str()
                    ),
                    reversible: true,
                },
                &ActionContext {
                    item_count: Some(1),
                    ..Default::default()
                },
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=tier"));

        let headers = vec![
            (
                "x-ms-access-tier".to_string(),
                target_tier.as_str().to_string(),
            ),
            ("x-ms-rehydrate-priority".to_string(), priority.to_string()),
        ];

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Empty,
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ResolvedCredential;
    use crate::policy::DenyAllPolicy;
    use crate::progress::NoopSink;
    use crate::types::{AuthKind, ResourceKind};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct FakeAuth;
    #[async_trait]
    impl crate::auth::AuthProvider for FakeAuth {
        fn kind(&self) -> AuthKind {
            AuthKind::Anonymous
        }
        fn display_name(&self) -> &str {
            "fake"
        }
        async fn resolve(&self) -> crate::Result<ResolvedCredential> {
            Ok(ResolvedCredential::Anonymous)
        }
        fn supports(&self, _: ResourceKind) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn deny_all_policy_short_circuits_before_http() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));

        let err = backend
            .set_access_tier(&ctx, &BlobPath::new("c", "b"), Tier::Archive)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
