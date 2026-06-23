//! Container lifecycle — create, delete, and set public-access level.

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::PublicAccess;
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::Method;

impl AzureBlobBackend {
    /// Create a container. Granting anonymous (`public_access` other than
    /// `Private`) is an elevated action and is policy-gated; a private create is
    /// not gated.
    pub async fn create_container(
        &self,
        ctx: &Ctx,
        name: &str,
        public_access: PublicAccess,
    ) -> crate::Result<()> {
        if let Some(level) = public_access.header_value() {
            let decision = ctx
                .policy
                .confirm(
                    &Action {
                        verb: "create_container".into(),
                        target: name.to_string(),
                        summary: format!("create container '{name}' with public access '{level}'"),
                        reversible: true,
                    },
                    &ActionContext::default(),
                )
                .await;
            match decision {
                PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
                PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
            }
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{name}"));
        url.set_query(Some("restype=container"));

        let mut headers = Vec::<(String, String)>::new();
        if let Some(level) = public_access.header_value() {
            headers.push(("x-ms-blob-public-access".into(), level.into()));
        }

        self.send_container(Method::PUT, url, headers).await
    }

    /// Delete a container and all its blobs. Always policy-gated (destructive,
    /// irreversible without soft-delete).
    pub async fn delete_container(&self, ctx: &Ctx, name: &str) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "delete_container".into(),
                    target: name.to_string(),
                    summary: format!("delete container '{name}' and all its blobs"),
                    reversible: false,
                },
                &ActionContext::default(),
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{name}"));
        url.set_query(Some("restype=container"));

        self.send_container(Method::DELETE, url, vec![]).await
    }

    /// Change a container's public-access level. Always policy-gated (it changes
    /// who can read the data anonymously).
    pub async fn set_container_public_access(
        &self,
        ctx: &Ctx,
        name: &str,
        public_access: PublicAccess,
    ) -> crate::Result<()> {
        let level_desc = public_access.header_value().unwrap_or("private");
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "set_container_public_access".into(),
                    target: name.to_string(),
                    summary: format!("set public access of container '{name}' to '{level_desc}'"),
                    reversible: true,
                },
                &ActionContext::default(),
            )
            .await;
        match decision {
            PolicyDecision::Allow | PolicyDecision::AllowAlways => {}
            PolicyDecision::Deny(reason) => return Err(Error::PolicyDenied(reason)),
        }

        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{name}"));
        url.set_query(Some("restype=container&comp=acl"));

        let mut headers = Vec::<(String, String)>::new();
        if let Some(level) = public_access.header_value() {
            headers.push(("x-ms-blob-public-access".into(), level.into()));
        }

        self.send_container(Method::PUT, url, headers).await
    }

    /// Shared sender for container ops with no response body of interest.
    async fn send_container(
        &self,
        method: Method,
        url: url::Url,
        headers: Vec<(String, String)>,
    ) -> crate::Result<()> {
        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let _ = pipeline
            .send(RequestTemplate {
                method,
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

    fn denied_backend_ctx() -> (AzureBlobBackend, Ctx) {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));
        (backend, ctx)
    }

    #[tokio::test]
    async fn delete_container_denied_short_circuits() {
        let (backend, ctx) = denied_backend_ctx();
        let err = backend.delete_container(&ctx, "c").await.unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn set_public_access_denied_short_circuits() {
        let (backend, ctx) = denied_backend_ctx();
        let err = backend
            .set_container_public_access(&ctx, "c", PublicAccess::Blob)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn create_container_with_public_access_is_gated() {
        let (backend, ctx) = denied_backend_ctx();
        let err = backend
            .create_container(&ctx, "c", PublicAccess::Container)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn create_private_container_is_not_gated() {
        // DenyAll would reject if the gate fired; instead a private create skips
        // the gate and proceeds to HTTP, failing on the unroutable endpoint.
        let (backend, ctx) = denied_backend_ctx();
        let err = backend
            .create_container(&ctx, "c", PublicAccess::Private)
            .await
            .unwrap_err();
        assert!(!matches!(err, Error::PolicyDenied(_)));
    }
}
