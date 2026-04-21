//! `DELETE /{container}/{blob}` — policy-gated blob deletion.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::{BlobPath, DeleteOpts};
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::Method;

impl AzureBlobBackend {
    /// Delete a blob. Calls `ctx.policy.confirm("delete_blob", ...)` before
    /// any HTTP is sent. Denies the operation on `Deny`.
    pub async fn delete_blob(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        opts: DeleteOpts,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "delete_blob".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!("delete {}/{}", path.container, path.blob),
                    reversible: true,
                },
                &ActionContext {
                    item_count: Some(1),
                    ..Default::default()
                },
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways { .. } => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(None);

        let mut headers = Vec::<(String, String)>::new();
        if opts.include_snapshots {
            headers.push(("x-ms-delete-snapshots".into(), "include".into()));
        }

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::DELETE,
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
    use crate::backend::azure::AzureBlobBackend;
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
        let ctx = Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy))
            .with_progress(Arc::new(NoopSink));

        let err = backend
            .delete_blob(&ctx, &BlobPath::new("c", "b"), DeleteOpts::default())
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
