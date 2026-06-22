//! Blob leases — `PUT {blob}?comp=lease` with `x-ms-lease-action`.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::BlobPath;
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::Method;

impl AzureBlobBackend {
    /// Acquire a lease on a blob. `duration_secs` is 15–60, or -1 for an
    /// infinite lease. Returns the lease id. Not policy-gated (non-destructive).
    pub async fn acquire_lease(
        &self,
        path: &BlobPath,
        duration_secs: i32,
    ) -> crate::Result<String> {
        let headers = vec![
            ("x-ms-lease-action".to_string(), "acquire".to_string()),
            ("x-ms-lease-duration".to_string(), duration_secs.to_string()),
        ];
        let resp = self.lease_request(path, headers).await?;
        resp.headers()
            .get("x-ms-lease-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| Error::Backend("acquire-lease response missing x-ms-lease-id".into()))
    }

    /// Release a held lease. Not policy-gated.
    pub async fn release_lease(&self, path: &BlobPath, lease_id: &str) -> crate::Result<()> {
        let headers = vec![
            ("x-ms-lease-action".to_string(), "release".to_string()),
            ("x-ms-lease-id".to_string(), lease_id.to_string()),
        ];
        self.lease_request(path, headers).await.map(|_| ())
    }

    /// Forcibly break a lease (regardless of who holds it). Policy-gated because
    /// it disrupts another holder's exclusive access.
    pub async fn break_lease(&self, ctx: &Ctx, path: &BlobPath) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "break_lease".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!("break the lease on {}/{}", path.container, path.blob),
                    reversible: true,
                },
                &ActionContext::default(),
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }
        let headers = vec![("x-ms-lease-action".to_string(), "break".to_string())];
        self.lease_request(path, headers).await.map(|_| ())
    }

    async fn lease_request(
        &self,
        path: &BlobPath,
        headers: Vec<(String, String)>,
    ) -> crate::Result<reqwest::Response> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=lease"));
        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        pipeline
            .send(RequestTemplate {
                method: Method::PUT,
                url,
                headers,
                body: Body::Empty,
            })
            .await
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
    async fn acquire_lease_returns_id() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("PUT", "/c/file.txt?comp=lease")
            .with_status(201)
            .with_header("x-ms-lease-id", "11111111-2222-3333-4444-555555555555")
            .create_async()
            .await;
        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let id = backend
            .acquire_lease(&BlobPath::new("c", "file.txt"), 30)
            .await
            .unwrap();
        assert_eq!(id, "11111111-2222-3333-4444-555555555555");
    }

    #[tokio::test]
    async fn break_lease_denied_short_circuits() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        let err = backend
            .break_lease(&ctx, &BlobPath::new("c", "b"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
