//! Blob snapshot + undelete — `PUT {blob}?comp=snapshot` / `?comp=undelete`.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::BlobPath;
use crate::{Ctx, Error};
use reqwest::Method;

impl AzureBlobBackend {
    /// Create a read-only snapshot of a blob. Returns the snapshot timestamp
    /// (`x-ms-snapshot`), which identifies the snapshot for later read/delete.
    /// Non-destructive, so not policy-gated.
    pub async fn create_snapshot(&self, path: &BlobPath) -> crate::Result<String> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=snapshot"));

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;
        let snapshot = resp
            .headers()
            .get("x-ms-snapshot")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Backend("snapshot response missing x-ms-snapshot".into()))?;
        Ok(snapshot)
    }

    /// Restore a soft-deleted blob (and its snapshots). Recovery is
    /// non-destructive, so this is not policy-gated.
    pub async fn undelete_blob(&self, path: &BlobPath) -> crate::Result<()> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=undelete"));

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;
        Ok(())
    }

    /// Delete a specific snapshot of a blob, identified by its timestamp.
    /// Policy-gated (destructive).
    pub async fn delete_snapshot(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        snapshot: &str,
    ) -> crate::Result<()> {
        use crate::policy::{Action, ActionContext, PolicyDecision};
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "delete_snapshot".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!(
                        "delete snapshot {snapshot} of {}/{}",
                        path.container, path.blob
                    ),
                    reversible: false,
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
        url.set_query(Some(&format!("snapshot={}", urlencoding::encode(snapshot))));

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method: Method::DELETE,
                url,
                headers: vec![],
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
    async fn create_snapshot_returns_timestamp() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("PUT", "/c/file.txt?comp=snapshot")
            .with_status(201)
            .with_header("x-ms-snapshot", "2026-06-22T10:00:00.0000000Z")
            .create_async()
            .await;
        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let snap = backend
            .create_snapshot(&BlobPath::new("c", "file.txt"))
            .await
            .unwrap();
        assert_eq!(snap, "2026-06-22T10:00:00.0000000Z");
    }

    #[tokio::test]
    async fn delete_snapshot_denied_short_circuits() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        let err = backend
            .delete_snapshot(
                &ctx,
                &BlobPath::new("c", "b"),
                "2026-06-22T10:00:00.0000000Z",
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
