//! Blob metadata — `HEAD {blob}` to read `x-ms-meta-*`, `PUT {blob}?comp=metadata`
//! to replace it (policy-gated).

use crate::backend::azure::http::{Body, HttpPipeline, RequestTemplate};
use crate::backend::azure::AzureBlobBackend;
use crate::backend::types::BlobPath;
use crate::policy::{Action, ActionContext, PolicyDecision};
use crate::{Ctx, Error};
use reqwest::Method;
use std::collections::HashMap;

const META_PREFIX: &str = "x-ms-meta-";

impl AzureBlobBackend {
    /// Read a blob's user-defined metadata (`x-ms-meta-*` headers), with the
    /// `x-ms-meta-` prefix stripped from each key.
    pub async fn get_metadata(&self, path: &BlobPath) -> crate::Result<HashMap<String, String>> {
        let mut url = self.endpoint.clone();
        url.set_path(&format!("/{}/{}", path.container, path.blob));
        url.set_query(Some("comp=metadata"));

        let pipeline = HttpPipeline {
            http: &self.http,
            credential: &self.credential,
        };
        let resp = pipeline
            .send(RequestTemplate {
                method: Method::GET,
                url,
                headers: vec![],
                body: Body::Empty,
            })
            .await?;

        let mut out = HashMap::new();
        for (name, value) in resp.headers() {
            let name = name.as_str().to_ascii_lowercase();
            if let Some(key) = name.strip_prefix(META_PREFIX) {
                if let Ok(v) = value.to_str() {
                    out.insert(key.to_string(), v.to_string());
                }
            }
        }
        Ok(out)
    }

    /// Replace a blob's user-defined metadata. Policy-gated (it overwrites all
    /// existing metadata with the supplied map).
    pub async fn set_metadata(
        &self,
        ctx: &Ctx,
        path: &BlobPath,
        metadata: &HashMap<String, String>,
    ) -> crate::Result<()> {
        let decision = ctx
            .policy
            .confirm(
                &Action {
                    verb: "set_metadata".into(),
                    target: format!("{}/{}", path.container, path.blob),
                    summary: format!(
                        "replace metadata of {}/{} ({} entries)",
                        path.container,
                        path.blob,
                        metadata.len()
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
        url.set_query(Some("comp=metadata"));

        let headers = metadata
            .iter()
            .map(|(k, v)| (format!("{META_PREFIX}{k}"), v.clone()))
            .collect();

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
    async fn get_metadata_strips_prefix_and_collects() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/c/file.txt?comp=metadata")
            .with_status(200)
            .with_header("x-ms-meta-author", "hamza")
            .with_header("x-ms-meta-project", "arkived")
            .with_header("x-ms-request-id", "ignored")
            .create_async()
            .await;

        let endpoint = url::Url::parse(&server.url()).unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();

        let md = backend
            .get_metadata(&BlobPath::new("c", "file.txt"))
            .await
            .unwrap();
        assert_eq!(md.get("author").map(String::as_str), Some("hamza"));
        assert_eq!(md.get("project").map(String::as_str), Some("arkived"));
        assert!(!md.contains_key("request-id"));
        assert_eq!(md.len(), 2);
    }

    #[tokio::test]
    async fn set_metadata_denied_short_circuits_before_http() {
        let endpoint = url::Url::parse("http://127.0.0.1:1/").unwrap();
        let backend = AzureBlobBackend::new(endpoint, ResolvedCredential::Anonymous).unwrap();
        let ctx =
            Ctx::new(Arc::new(FakeAuth), Arc::new(DenyAllPolicy)).with_progress(Arc::new(NoopSink));

        let mut md = HashMap::new();
        md.insert("k".to_string(), "v".to_string());
        let err = backend
            .set_metadata(&ctx, &BlobPath::new("c", "b"), &md)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::PolicyDenied(_)));
    }
}
